use crate::database::repositories::{
    analysis::AnalysisProcessesRepository, meeting::MeetingsRepository,
    setting::SettingsRepository,
};
use crate::ollama::metadata::ModelMetadataCache;
use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::processor::{chunk_text, clean_llm_markdown_output, rough_token_count};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use once_cell::sync::Lazy;

static METADATA_CACHE: Lazy<ModelMetadataCache> =
    Lazy::new(|| ModelMetadataCache::new(Duration::from_secs(300)));

static CANCELLATION_REGISTRY: Lazy<Arc<Mutex<HashMap<String, CancellationToken>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

const ANALYSIS_SYSTEM_PROMPT: &str = r#"You are an expert analyst reviewing a meeting transcript.
Your task is to produce a DEEP ANALYSIS — going beyond what was said to reveal patterns, implications, and actionable insights.

Structure your analysis with these sections:

## Discussion Themes
Identify recurring topics, conversational threads, and how emphasis shifted throughout the meeting.

## Key Findings
Extract specific facts, data points, or conclusions that were established during the discussion.

## Interpretations
Describe what the discussion implies beyond the explicit words — subtext, priorities, tensions, unstated assumptions.

## Risks & Open Questions
Identify unresolved issues, assumptions that may not hold, potential blockers, and topics needing follow-up.

## Recommendations
Provide concrete, specific next steps and strategic or process improvements based on the discussion.

Rules:
- Do NOT include a title or heading at the top — begin directly with the first section heading
- Base all interpretations on evidence from the transcript; cite specific moments when helpful
- Flag uncertainty with "likely", "possibly", or "the transcript suggests"
- Be direct and specific — avoid generic advice
- If a section has nothing to add, write: "Nothing notable."
- Output only the completed analysis in Markdown format"#;

pub struct AnalysisService;

impl AnalysisService {
    fn register_cancellation_token(meeting_id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        if let Ok(mut registry) = CANCELLATION_REGISTRY.lock() {
            registry.insert(meeting_id.to_string(), token.clone());
        }
        token
    }

    pub fn cancel_analysis(meeting_id: &str) -> bool {
        if let Ok(registry) = CANCELLATION_REGISTRY.lock() {
            if let Some(token) = registry.get(meeting_id) {
                token.cancel();
                return true;
            }
        }
        false
    }

    fn cleanup_cancellation_token(meeting_id: &str) {
        if let Ok(mut registry) = CANCELLATION_REGISTRY.lock() {
            registry.remove(meeting_id);
        }
    }

    pub async fn process_analysis_background<R: tauri::Runtime>(
        app: AppHandle<R>,
        pool: SqlitePool,
        meeting_id: String,
        text: String,
    ) {
        let start_time = Instant::now();
        info!("Starting analysis for meeting_id: {}", meeting_id);

        if let Err(e) = AnalysisProcessesRepository::create_or_reset_process(&pool, &meeting_id).await {
            error!("Failed to initialize analysis process for {}: {}", meeting_id, e);
            return;
        }

        let cancellation_token = Self::register_cancellation_token(&meeting_id);

        // Load provider settings
        let model_config = match SettingsRepository::get_model_config(&pool).await {
            Ok(Some(cfg)) => cfg,
            Ok(None) => {
                Self::fail(&pool, &meeting_id, "No model configuration found").await;
                return;
            }
            Err(e) => {
                Self::fail(&pool, &meeting_id, &format!("Failed to load model config: {}", e)).await;
                return;
            }
        };

        let provider = match LLMProvider::from_str(&model_config.provider) {
            Ok(p) => p,
            Err(e) => {
                Self::fail(&pool, &meeting_id, &e).await;
                return;
            }
        };

        let api_key = if provider == LLMProvider::Ollama
            || provider == LLMProvider::BuiltInAI
            || provider == LLMProvider::CustomOpenAI
        {
            String::new()
        } else {
            match SettingsRepository::get_api_key(&pool, &model_config.provider).await {
                Ok(Some(key)) if !key.is_empty() => key,
                _ => {
                    Self::fail(
                        &pool,
                        &meeting_id,
                        &format!("API key not found for {}", model_config.provider),
                    )
                    .await;
                    return;
                }
            }
        };

        let ollama_endpoint = if provider == LLMProvider::Ollama {
            model_config.ollama_endpoint.clone()
        } else {
            None
        };

        let (custom_openai_endpoint, custom_openai_api_key, custom_openai_max_tokens, custom_openai_temperature, custom_openai_top_p) =
            if provider == LLMProvider::CustomOpenAI {
                match SettingsRepository::get_custom_openai_config(&pool).await {
                    Ok(Some(cfg)) => (
                        Some(cfg.endpoint),
                        cfg.api_key,
                        cfg.max_tokens.map(|t| t as u32),
                        cfg.temperature,
                        cfg.top_p,
                    ),
                    _ => {
                        Self::fail(&pool, &meeting_id, "Custom OpenAI config not found").await;
                        return;
                    }
                }
            } else {
                (None, None, None, None, None)
            };

        let final_api_key = if provider == LLMProvider::CustomOpenAI {
            custom_openai_api_key.unwrap_or_default()
        } else {
            api_key
        };

        let token_threshold = if provider == LLMProvider::Ollama {
            match METADATA_CACHE
                .get_or_fetch(&model_config.model, ollama_endpoint.as_deref())
                .await
            {
                Ok(metadata) => metadata.context_size.saturating_sub(600),
                Err(_) => 4000,
            }
        } else if provider == LLMProvider::BuiltInAI {
            use crate::summary::summary_engine::models;
            match models::get_model_by_name(&model_config.model) {
                Some(m) => m.context_size.saturating_sub(600) as usize,
                None => 1748,
            }
        } else {
            100000
        };

        let app_data_dir = app.path().app_data_dir().ok();
        let client = reqwest::Client::new();

        // Build transcript content — chunk if needed
        let total_tokens = rough_token_count(&text);
        let content_to_analyze: String;

        if provider != LLMProvider::Ollama && provider != LLMProvider::BuiltInAI
            || total_tokens < token_threshold
        {
            content_to_analyze = text.clone();
        } else {
            // Multi-level: summarize chunks first, then analyse
            let chunks = chunk_text(&text, token_threshold.saturating_sub(300), 100);
            let mut chunk_summaries = Vec::new();

            for (i, chunk) in chunks.iter().enumerate() {
                if cancellation_token.is_cancelled() {
                    Self::cancel_db(&pool, &meeting_id).await;
                    return;
                }
                let user_prompt = format!(
                    "Provide a concise but comprehensive summary of this transcript segment, capturing all key points, decisions, and action items.\n\n<segment>\n{}\n</segment>",
                    chunk
                );
                match generate_summary(
                    &client,
                    &provider,
                    &model_config.model,
                    &final_api_key,
                    "You are an expert meeting summarizer.",
                    &user_prompt,
                    ollama_endpoint.as_deref(),
                    custom_openai_endpoint.as_deref(),
                    custom_openai_max_tokens,
                    custom_openai_temperature,
                    custom_openai_top_p,
                    app_data_dir.as_ref(),
                    Some(&cancellation_token),
                )
                .await
                {
                    Ok(summary) => {
                        info!("Analysis pre-processing: chunk {}/{} done", i + 1, chunks.len());
                        chunk_summaries.push(summary);
                    }
                    Err(e) if e.contains("cancelled") => {
                        Self::cancel_db(&pool, &meeting_id).await;
                        return;
                    }
                    Err(e) => {
                        error!("Analysis chunk {} failed: {}", i + 1, e);
                    }
                }
            }

            if chunk_summaries.is_empty() {
                Self::fail(&pool, &meeting_id, "All transcript chunks failed to process").await;
                return;
            }

            content_to_analyze = chunk_summaries.join("\n---\n");
        }

        if cancellation_token.is_cancelled() {
            Self::cancel_db(&pool, &meeting_id).await;
            return;
        }

        let user_prompt = format!(
            "<transcript>\n{}\n</transcript>",
            content_to_analyze
        );

        let raw = match generate_summary(
            &client,
            &provider,
            &model_config.model,
            &final_api_key,
            ANALYSIS_SYSTEM_PROMPT,
            &user_prompt,
            ollama_endpoint.as_deref(),
            custom_openai_endpoint.as_deref(),
            custom_openai_max_tokens,
            custom_openai_temperature,
            custom_openai_top_p,
            app_data_dir.as_ref(),
            Some(&cancellation_token),
        )
        .await
        {
            Ok(r) => r,
            Err(e) if e.contains("cancelled") => {
                Self::cancel_db(&pool, &meeting_id).await;
                Self::cleanup_cancellation_token(&meeting_id);
                return;
            }
            Err(e) => {
                Self::fail(&pool, &meeting_id, &e).await;
                Self::cleanup_cancellation_token(&meeting_id);
                return;
            }
        };

        Self::cleanup_cancellation_token(&meeting_id);

        let analysis_markdown = clean_llm_markdown_output(&raw);
        let duration = start_time.elapsed().as_secs_f64();

        if let Err(e) = AnalysisProcessesRepository::update_process_completed(
            &pool,
            &meeting_id,
            &analysis_markdown,
            duration,
        )
        .await
        {
            error!("Failed to save analysis for {}: {}", meeting_id, e);
            return;
        }

        // Write analysis.md alongside summary.md
        match MeetingsRepository::get_meeting_metadata(&pool, &meeting_id).await {
            Ok(Some(meeting)) => {
                if let Some(folder_path) = meeting.folder_path {
                    let analysis_path =
                        std::path::Path::new(&folder_path).join("analysis.md");
                    if let Err(e) = std::fs::write(&analysis_path, &analysis_markdown) {
                        warn!("Failed to write analysis.md for {}: {}", meeting_id, e);
                    } else {
                        info!("analysis.md written to {}", analysis_path.display());
                    }
                }
            }
            Ok(None) => warn!("Meeting {} not found when writing analysis.md", meeting_id),
            Err(e) => warn!("Failed to look up folder_path for {}: {}", meeting_id, e),
        }

        info!(
            "Analysis completed for meeting_id: {} in {:.2}s",
            meeting_id, duration
        );
    }

    async fn fail(pool: &SqlitePool, meeting_id: &str, msg: &str) {
        error!("Analysis failed for {}: {}", meeting_id, msg);
        if let Err(e) = AnalysisProcessesRepository::update_process_failed(pool, meeting_id, msg).await {
            error!("Failed to update analysis status for {}: {}", meeting_id, e);
        }
    }

    async fn cancel_db(pool: &SqlitePool, meeting_id: &str) {
        info!("Analysis cancelled for meeting_id: {}", meeting_id);
        if let Err(e) =
            AnalysisProcessesRepository::update_process_cancelled(pool, meeting_id).await
        {
            error!("Failed to update analysis cancelled status for {}: {}", meeting_id, e);
        }
    }
}

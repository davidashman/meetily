use crate::database::repositories::{
    analysis::AnalysisProcessesRepository, meeting::MeetingsRepository,
    summary::SummaryProcessesRepository, transcript_chunk::TranscriptChunksRepository,
};
use crate::state::AppState;
use crate::summary::analysis_service::AnalysisService;
use crate::summary::service::SummaryService;
use log::{error as log_error, info as log_info, warn as log_warn};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};

#[derive(Debug, Serialize, Deserialize)]
pub struct SummaryResponse {
    pub status: String,
    #[serde(rename = "meetingName")]
    pub meeting_name: Option<String>,
    pub meeting_id: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessTranscriptResponse {
    pub message: String,
    pub process_id: String,
}

/// Saves a meeting summary (Native SQLx implementation)
///
/// Expected format: { "markdown": "...", "summary_json": [...BlockNote blocks...] }
#[tauri::command]
pub async fn api_save_meeting_summary<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    summary: serde_json::Value,
    _auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
    log_info!(
        "api_save_meeting_summary (native) called for meeting_id: {}",
        meeting_id
    );
    let pool = state.db_manager.pool();

    match SummaryProcessesRepository::update_meeting_summary(pool, &meeting_id, &summary).await {
        Ok(true) => {
            log_info!("Summary saved successfully for meeting_id: {}", meeting_id);

            // Write summary.md to the meeting folder alongside transcripts.json and metadata.json
            if let Some(markdown) = summary.get("markdown").and_then(|v| v.as_str()) {
                match MeetingsRepository::get_meeting_metadata(pool, &meeting_id).await {
                    Ok(Some(meeting)) => {
                        if let Some(folder_path) = meeting.folder_path {
                            let summary_path = std::path::Path::new(&folder_path).join("summary.md");
                            if let Err(e) = std::fs::write(&summary_path, markdown) {
                                log_warn!("Failed to write summary.md for {}: {}", meeting_id, e);
                            } else {
                                log_info!("summary.md written to {}", summary_path.display());
                            }
                        }
                    }
                    Ok(None) => log_warn!("Meeting {} not found when writing summary.md", meeting_id),
                    Err(e) => log_warn!("Failed to look up folder_path for {}: {}", meeting_id, e),
                }
            }

            Ok(serde_json::json!({
                "message": "Meeting summary saved successfully"
            }))
        }
        Ok(false) => {
            log_warn!(
                "Meeting not found or invalid JSON for meeting_id: {}",
                meeting_id
            );
            Err("Meeting not found or can't convert the json".into())
        }
        Err(e) => {
            log_error!("Failed to save meeting summary for {}: {}", meeting_id, e);
            Err(e.to_string())
        }
    }
}

/// Gets summary status and data (Native SQLx implementation)
///
/// Returns summary status (pending/processing/completed/failed) and parsed result data
#[tauri::command]
pub async fn api_get_summary<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    _auth_token: Option<String>,
) -> Result<SummaryResponse, String> {
    log_info!(
        "api_get_summary (native) called for meeting_id: {}",
        meeting_id
    );
    let pool = state.db_manager.pool();

    match SummaryProcessesRepository::get_summary_data_for_meeting(pool, &meeting_id).await {
        Ok(Some(process)) => {
            let status = process.status.to_lowercase();
            let error = process.error;

            // Parse result data if it exists (regardless of status)
            // This allows displaying restored summaries after cancellation or failure
            let data = if let Some(result_str) = process.result {
                match serde_json::from_str::<serde_json::Value>(&result_str) {
                    Ok(parsed) => Some(parsed),
                    Err(e) => {
                        log_error!("Failed to parse summary result JSON: {}", e);
                        None
                    }
                }
            } else {
                None
            };

            // Fetch meeting title from database
            let meeting_name = match MeetingsRepository::get_meeting(pool, &meeting_id).await {
                Ok(Some(meeting_details)) => {
                    log_info!("Fetched meeting title: {}", &meeting_details.title);
                    Some(meeting_details.title)
                }
                Ok(None) => {
                    log_warn!("Meeting not found for meeting_id: {}", meeting_id);
                    None
                }
                Err(e) => {
                    log_error!("Failed to fetch meeting title: {}", e);
                    None
                }
            };

            let response = SummaryResponse {
                status: status.clone(),
                meeting_name,
                meeting_id: meeting_id.clone(),
                start: process.start_time.map(|t| t.to_rfc3339()),
                end: process.end_time.map(|t| t.to_rfc3339()),
                data,
                error,
            };

            log_info!(
                "Summary status for {}: {}, has_data: {}, meeting_name: {:?}",
                meeting_id,
                status,
                response.data.is_some(),
                response.meeting_name
            );
            Ok(response)
        }
        Ok(None) => {
            log_info!("No summary process found for meeting_id: {}", meeting_id);

            // Still fetch meeting title for idle state
            let meeting_name = match MeetingsRepository::get_meeting(pool, &meeting_id).await {
                Ok(Some(meeting_details)) => Some(meeting_details.title),
                _ => None,
            };

            Ok(SummaryResponse {
                status: "idle".to_string(),
                meeting_name,
                meeting_id,
                start: None,
                end: None,
                data: None,
                error: None,
            })
        }
        Err(e) => {
            log_error!("Error retrieving summary for {}: {}", meeting_id, e);
            Err(format!("Failed to retrieve summary: {}", e))
        }
    }
}

/// Processes transcript and generates summary (Native SQLx implementation)
///
/// Spawns a background task and returns immediately with process_id
#[tauri::command]
pub async fn api_process_transcript<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    text: String,
    model: String,
    model_name: String,
    meeting_id: Option<String>,
    _chunk_size: Option<i32>,
    _overlap: Option<i32>,
    custom_prompt: Option<String>,
    template_id: Option<String>,
    _auth_token: Option<String>,
) -> Result<ProcessTranscriptResponse, String> {
    use uuid::Uuid;

    let m_id = meeting_id.unwrap_or_else(|| format!("meeting-{}", Uuid::new_v4()));
    log_info!(
        "api_process_transcript (native) called for meeting_id: {}, model: {}",
        &m_id,
        &model
    );

    let pool = state.db_manager.pool().clone();
    let final_prompt = custom_prompt.unwrap_or_else(|| "".to_string());
    let final_template_id = template_id.unwrap_or_else(|| "daily_standup".to_string());

    // Create or reset the process entry in the database
    SummaryProcessesRepository::create_or_reset_process(&pool, &m_id)
        .await
        .map_err(|e| format!("Failed to initialize process: {}", e))?;

    log_info!("✓ Summary process initialized for meeting_id: {}", &m_id);

    // Save transcript chunks data (matching Python backend behavior)
    let chunk_size = _chunk_size.unwrap_or(40000);
    let overlap = _overlap.unwrap_or(1000);

    TranscriptChunksRepository::save_transcript_data(
        &pool,
        &m_id,
        &text,
        &model,
        &model_name,
        chunk_size,
        overlap,
    )
    .await
    .map_err(|e| format!("Failed to save transcript data: {}", e))?;

    log_info!("✓ Transcript chunks saved for meeting_id: {}", &m_id);

    // Clone what both background tasks need
    let app_for_analysis = app.clone();
    let pool_for_analysis = pool.clone();
    let text_for_analysis = text.clone();
    let analysis_meeting_id = m_id.clone();

    // Spawn summary background task
    let meeting_id_clone = m_id.clone();
    tauri::async_runtime::spawn(async move {
        SummaryService::process_transcript_background(
            app,
            pool,
            meeting_id_clone,
            text,
            model,
            model_name,
            final_prompt,
            final_template_id,
        )
        .await;
    });

    // Spawn analysis in parallel from the same transcript
    tauri::async_runtime::spawn(async move {
        AnalysisService::process_analysis_background(
            app_for_analysis,
            pool_for_analysis,
            analysis_meeting_id,
            text_for_analysis,
        )
        .await;
    });

    log_info!("🚀 Background tasks spawned for meeting_id: {}", &m_id);

    Ok(ProcessTranscriptResponse {
        message: "Summary generation started".to_string(),
        process_id: m_id,
    })
}

/// Cancels an ongoing summary generation process
///
/// This command triggers the cancellation token for the specified meeting,
/// stopping the summary generation gracefully.
#[tauri::command]
pub async fn api_cancel_summary<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<serde_json::Value, String> {
    log_info!("api_cancel_summary called for meeting_id: {}", meeting_id);

    // Trigger cancellation via the service
    let cancelled = SummaryService::cancel_summary(&meeting_id);

    if cancelled {
        // Update database status to cancelled
        let pool = state.db_manager.pool();
        if let Err(e) = SummaryProcessesRepository::update_process_cancelled(pool, &meeting_id).await {
            log_error!("Failed to update DB status to cancelled for {}: {}", meeting_id, e);
            return Err(format!("Failed to update cancellation status: {}", e));
        }

        log_info!("Successfully cancelled summary generation for meeting_id: {}", meeting_id);
        Ok(serde_json::json!({
            "message": "Summary generation cancelled successfully",
            "meeting_id": meeting_id,
        }))
    } else {
        log_warn!("No active summary generation found for meeting_id: {}", meeting_id);
        Ok(serde_json::json!({
            "message": "No active summary generation to cancel",
            "meeting_id": meeting_id,
        }))
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AnalysisResponse {
    pub status: String,
    pub markdown: Option<String>,
    pub error: Option<String>,
}

/// Retrieves the current analysis status and result for a meeting
#[tauri::command]
pub async fn api_get_analysis<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<AnalysisResponse, String> {
    log_info!("api_get_analysis called for meeting_id: {}", meeting_id);
    let pool = state.db_manager.pool();

    match AnalysisProcessesRepository::get_analysis(pool, &meeting_id).await {
        Ok(Some(process)) => Ok(AnalysisResponse {
            status: process.status.to_lowercase(),
            markdown: process.result,
            error: process.error,
        }),
        Ok(None) => Ok(AnalysisResponse {
            status: "idle".to_string(),
            markdown: None,
            error: None,
        }),
        Err(e) => Err(format!("Failed to retrieve analysis: {}", e)),
    }
}

/// Manually triggers analysis generation for a meeting (re-run)
#[tauri::command]
pub async fn api_process_analysis<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    text: String,
) -> Result<serde_json::Value, String> {
    log_info!("api_process_analysis called for meeting_id: {}", meeting_id);

    let pool = state.db_manager.pool().clone();

    tauri::async_runtime::spawn(async move {
        AnalysisService::process_analysis_background(app, pool, meeting_id, text).await;
    });

    Ok(serde_json::json!({ "message": "Analysis started" }))
}

/// Cancels an ongoing analysis generation
#[tauri::command]
pub async fn api_cancel_analysis<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<serde_json::Value, String> {
    log_info!("api_cancel_analysis called for meeting_id: {}", meeting_id);

    let cancelled = AnalysisService::cancel_analysis(&meeting_id);

    if cancelled {
        let pool = state.db_manager.pool();
        if let Err(e) =
            AnalysisProcessesRepository::update_process_cancelled(pool, &meeting_id).await
        {
            return Err(format!("Failed to update cancellation status: {}", e));
        }
        Ok(serde_json::json!({ "message": "Analysis cancelled", "meeting_id": meeting_id }))
    } else {
        Ok(serde_json::json!({ "message": "No active analysis to cancel", "meeting_id": meeting_id }))
    }
}

/// Saves edited analysis markdown back to the database and disk
#[tauri::command]
pub async fn api_save_analysis<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    markdown: String,
) -> Result<serde_json::Value, String> {
    log_info!("api_save_analysis called for meeting_id: {}", meeting_id);
    let pool = state.db_manager.pool();

    AnalysisProcessesRepository::save_analysis_result(pool, &meeting_id, &markdown)
        .await
        .map_err(|e| format!("Failed to save analysis: {}", e))?;

    // Write analysis.md to the meeting folder
    match MeetingsRepository::get_meeting_metadata(pool, &meeting_id).await {
        Ok(Some(meeting)) => {
            if let Some(folder_path) = meeting.folder_path {
                let analysis_path = std::path::Path::new(&folder_path).join("analysis.md");
                if let Err(e) = std::fs::write(&analysis_path, &markdown) {
                    log_warn!("Failed to write analysis.md for {}: {}", meeting_id, e);
                } else {
                    log_info!("analysis.md updated at {}", analysis_path.display());
                }
            }
        }
        Ok(None) => log_warn!("Meeting {} not found when saving analysis.md", meeting_id),
        Err(e) => log_warn!("Failed to look up folder_path for {}: {}", meeting_id, e),
    }

    Ok(serde_json::json!({ "message": "Analysis saved successfully" }))
}

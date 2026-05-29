use crate::database::models::AnalysisProcess;
use chrono::Utc;
use sqlx::SqlitePool;
use tracing::info;

pub struct AnalysisProcessesRepository;

impl AnalysisProcessesRepository {
    pub async fn get_analysis(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<AnalysisProcess>, sqlx::Error> {
        sqlx::query_as::<_, AnalysisProcess>(
            "SELECT * FROM analysis_processes WHERE meeting_id = ?",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn create_or_reset_process(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO analysis_processes (meeting_id, status, created_at, updated_at, start_time, result, error)
            VALUES (?, 'PENDING', ?, ?, ?, NULL, NULL)
            ON CONFLICT(meeting_id) DO UPDATE SET
                status = 'PENDING',
                updated_at = excluded.updated_at,
                start_time = excluded.start_time,
                result = NULL,
                error = NULL
            "#,
        )
        .bind(meeting_id)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
        info!("Analysis process initialized for meeting_id: {}", meeting_id);
        Ok(())
    }

    pub async fn update_process_completed(
        pool: &SqlitePool,
        meeting_id: &str,
        result: &str,
        processing_time: f64,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE analysis_processes
            SET status = 'completed', result = ?, updated_at = ?, end_time = ?, processing_time = ?, error = NULL
            WHERE meeting_id = ?
            "#,
        )
        .bind(result)
        .bind(now)
        .bind(now)
        .bind(processing_time)
        .bind(meeting_id)
        .execute(pool)
        .await?;
        info!("Analysis completed for meeting_id: {}", meeting_id);
        Ok(())
    }

    pub async fn update_process_failed(
        pool: &SqlitePool,
        meeting_id: &str,
        error: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE analysis_processes
            SET status = 'failed', error = ?, updated_at = ?, end_time = ?
            WHERE meeting_id = ?
            "#,
        )
        .bind(error)
        .bind(now)
        .bind(now)
        .bind(meeting_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_process_cancelled(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE analysis_processes
            SET status = 'cancelled', updated_at = ?, end_time = ?, error = 'Cancelled by user'
            WHERE meeting_id = ?
            "#,
        )
        .bind(now)
        .bind(now)
        .bind(meeting_id)
        .execute(pool)
        .await?;
        Ok(())
    }
}

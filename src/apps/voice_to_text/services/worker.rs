use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use sqlx::SqlitePool;

use crate::config::Config;
use crate::services::notify;
use crate::services::whisper;

#[derive(sqlx::FromRow)]
struct PendingJob {
    id: i64,
    audio_path: String,
    model_used: String,
    original_filename: String,
}

/// Spawn the background voice transcription worker.
/// Polls for pending jobs every 5 seconds and processes one at a time.
pub fn spawn(pool: SqlitePool, config: Arc<Config>) {
    tokio::spawn(async move {
        tracing::info!("Voice transcription worker started");
        loop {
            if let Err(e) = process_next(&pool, &config).await {
                tracing::error!("Voice worker error: {e}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

async fn process_next(pool: &SqlitePool, config: &Config) -> anyhow::Result<()> {
    // Claim the oldest pending job
    let job: Option<PendingJob> = sqlx::query_as(
        "UPDATE voice_to_text_jobs
         SET status = 'processing'
         WHERE id = (
             SELECT id FROM voice_to_text_jobs WHERE status = 'pending' ORDER BY created_at ASC LIMIT 1
         )
         RETURNING id, audio_path, model_used, original_filename",
    )
    .fetch_optional(pool)
    .await?;

    let Some(job) = job else {
        return Ok(());
    };

    tracing::info!(
        job_id = job.id,
        file = %job.original_filename,
        model = %job.model_used,
        "Processing voice job"
    );

    let start = Instant::now();
    let result = run_transcription(config, &job.audio_path, &job.model_used).await;
    let elapsed = start.elapsed().as_secs_f64();

    match result {
        Ok(text) => {
            sqlx::query(
                "UPDATE voice_to_text_jobs
                 SET status = 'done', transcription = ?, duration_secs = ?,
                     completed_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(&text)
            .bind(elapsed)
            .bind(job.id)
            .execute(pool)
            .await?;

            tracing::info!(
                job_id = job.id,
                elapsed_secs = elapsed,
                "Transcription complete"
            );
            notify::send(
                pool,
                config,
                "VoiceToText",
                &format!(
                    "Transcription complete: {} ({:.0}s)",
                    job.original_filename, elapsed
                ),
            )
            .await;
        }
        Err(e) => {
            let msg = format!("{e}");
            sqlx::query(
                "UPDATE voice_to_text_jobs
                 SET status = 'failed', error_message = ?, duration_secs = ?,
                     completed_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(&msg)
            .bind(elapsed)
            .bind(job.id)
            .execute(pool)
            .await?;

            tracing::warn!(job_id = job.id, error = %e, "Transcription failed");
            notify::send(
                pool,
                config,
                "VoiceToText",
                &format!("Transcription failed: {} — {}", job.original_filename, msg),
            )
            .await;
        }
    }

    Ok(())
}

async fn run_transcription(
    config: &Config,
    audio_path: &str,
    model: &str,
) -> anyhow::Result<String> {
    let input = Path::new(audio_path);

    // Convert to 16 kHz mono WAV
    let wav_path = whisper::convert_to_wav(input).await?;

    // Run whisper
    let text = whisper::transcribe(config, &wav_path, model).await?;

    // Clean up converted file
    let _ = tokio::fs::remove_file(&wav_path).await;

    Ok(text)
}

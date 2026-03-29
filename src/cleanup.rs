use std::path::PathBuf;
use std::sync::Arc;

use tokio::task::JoinHandle;
use tokio::time::Duration;
use tracing::{info, warn};

use crate::api::settings::RetentionPolicy;
use crate::db::database::Database;

/// Spawn a background task that periodically cleans up old records and audio files.
/// First cycle runs after 1 hour, then repeats hourly.
pub fn spawn_retention_cleanup(db: Arc<Database>, data_dir: PathBuf) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;

            let audio_dir = data_dir.join("audio");

            // Load settings directly (async, no blocking pool needed)
            let settings = match db.load_settings().await {
                Ok(s) => s,
                Err(e) => {
                    warn!(error = %e, "failed to load settings for retention cleanup");
                    continue;
                }
            };

            run_audio_cleanup(&db, &audio_dir, &settings.audio_retention).await;
            run_record_cleanup(&db, &settings.record_retention).await;
        }
    })
}

async fn run_audio_cleanup(
    db: &Arc<Database>,
    audio_dir: &std::path::Path,
    policy: &RetentionPolicy,
) {
    let paths = match policy {
        RetentionPolicy::Days(days) if *days > 0 => {
            db.get_audio_paths_older_than_days(*days as i64).await.ok()
        }
        RetentionPolicy::Count(max) => db.get_audio_paths_exceeding_count(*max).await.ok(),
        RetentionPolicy::DiskLimitMb(limit_mb) => {
            let limit = *limit_mb;
            let audio_dir_owned = audio_dir.to_path_buf();
            match db.get_audio_paths_oldest_first().await {
                Ok(entries) => {
                    // Calculate total size and collect paths to delete
                    // (file I/O is fast enough for this use case; runs hourly)
                    let mut total: u64 = 0;
                    let mut sized: Vec<(String, u64)> = Vec::new();
                    for (_, filename) in &entries {
                        let path = audio_dir_owned.join(filename);
                        let size = tokio::fs::metadata(&path)
                            .await
                            .map(|m| m.len())
                            .unwrap_or(0);
                        total += size;
                        sized.push((filename.clone(), size));
                    }
                    let limit_bytes = limit * 1024 * 1024;
                    let mut to_delete = Vec::new();
                    for (filename, size) in sized {
                        if total <= limit_bytes {
                            break;
                        }
                        total = total.saturating_sub(size);
                        to_delete.push(filename);
                    }
                    Some(to_delete)
                }
                Err(_) => None,
            }
        }
        _ => None,
    };

    if let Some(paths) = paths {
        for filename in &paths {
            let path = audio_dir.join(filename);
            let _ = tokio::fs::remove_file(&path).await;
        }
        if !paths.is_empty() {
            info!(count = paths.len(), "cleaned up audio files");
        }
    }
}

async fn run_record_cleanup(db: &Arc<Database>, policy: &RetentionPolicy) {
    let result = match policy {
        RetentionPolicy::Days(days) if *days > 0 => {
            db.cleanup_records_older_than_days(*days as i64).await.ok()
        }
        RetentionPolicy::Count(max) => db.cleanup_records_by_count(*max).await.ok(),
        _ => None,
    };

    if let Some(count) = result {
        if count > 0 {
            info!(count, "cleaned up transcription records");
        }
    }
}

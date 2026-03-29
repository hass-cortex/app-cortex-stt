use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::task::JoinHandle;
use tokio::time::{Duration, interval};
use tracing::{info, warn};

use crate::api::settings::RetentionPolicy;
use crate::db::database::Database;

/// Spawn a background task that periodically cleans up old records and audio files
/// based on the retention settings stored in the database.
///
/// Runs once per hour. Loads current settings each cycle so retention changes
/// take effect without a restart.
pub fn spawn_retention_cleanup(db: Arc<Database>, data_dir: PathBuf) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(3600));
        loop {
            ticker.tick().await;

            let settings = match db.load_settings() {
                Ok(s) => s,
                Err(e) => {
                    warn!(error = %e, "failed to load settings for retention cleanup");
                    continue;
                }
            };

            let audio_dir = data_dir.join("audio");

            // Clean up audio files according to the audio retention policy.
            run_audio_cleanup(&db, &audio_dir, &settings.audio_retention).await;

            // Clean up transcription records according to the record retention policy.
            run_record_cleanup(&db, &settings.record_retention).await;
        }
    })
}

/// Apply the audio retention policy: delete audio files from disk.
async fn run_audio_cleanup(db: &Database, audio_dir: &Path, policy: &RetentionPolicy) {
    match policy {
        RetentionPolicy::Days(days) => {
            let days = *days as i64;
            if days <= 0 {
                return;
            }
            match db.get_audio_paths_older_than_days(days) {
                Ok(paths) => {
                    for filename in &paths {
                        let path = audio_dir.join(filename);
                        let _ = tokio::fs::remove_file(&path).await;
                    }
                    if !paths.is_empty() {
                        info!(
                            count = paths.len(),
                            days, "cleaned up old audio files (days policy)"
                        );
                    }
                }
                Err(e) => warn!(error = %e, "audio retention cleanup query failed"),
            }
        }
        RetentionPolicy::Count(max_count) => match db.get_audio_paths_exceeding_count(*max_count) {
            Ok(paths) => {
                for filename in &paths {
                    let path = audio_dir.join(filename);
                    let _ = tokio::fs::remove_file(&path).await;
                }
                if !paths.is_empty() {
                    info!(
                        count = paths.len(),
                        max_count, "cleaned up audio files (count policy)"
                    );
                }
            }
            Err(e) => warn!(error = %e, "audio retention count cleanup failed"),
        },
        RetentionPolicy::DiskLimitMb(limit_mb) => {
            let limit_bytes = *limit_mb * 1024 * 1024;
            match db.get_audio_paths_oldest_first() {
                Ok(entries) => {
                    // Calculate total size of all audio files.
                    let mut sizes: Vec<(String, String, u64)> = Vec::new();
                    let mut total_size: u64 = 0;
                    for (record_id, filename) in &entries {
                        let path = audio_dir.join(filename);
                        let size = tokio::fs::metadata(&path)
                            .await
                            .map(|m| m.len())
                            .unwrap_or(0);
                        total_size += size;
                        sizes.push((record_id.clone(), filename.clone(), size));
                    }

                    if total_size <= limit_bytes {
                        return;
                    }

                    // Delete oldest files until under limit.
                    let mut deleted_count = 0usize;
                    for (_, filename, size) in &sizes {
                        if total_size <= limit_bytes {
                            break;
                        }
                        let path = audio_dir.join(filename);
                        let _ = tokio::fs::remove_file(&path).await;
                        total_size = total_size.saturating_sub(*size);
                        deleted_count += 1;
                    }

                    if deleted_count > 0 {
                        info!(
                            count = deleted_count,
                            limit_mb, "cleaned up audio files (disk limit policy)"
                        );
                    }
                }
                Err(e) => warn!(error = %e, "audio retention disk limit cleanup failed"),
            }
        }
        RetentionPolicy::Unlimited => {
            // No cleanup needed.
        }
    }
}

/// Apply the record retention policy: delete transcription records from the database.
async fn run_record_cleanup(db: &Database, policy: &RetentionPolicy) {
    match policy {
        RetentionPolicy::Days(days) => {
            let days = *days as i64;
            if days <= 0 {
                return;
            }
            match db.cleanup_records_older_than_days(days) {
                Ok(count) if count > 0 => {
                    info!(
                        count,
                        days, "cleaned up old transcription records (days policy)"
                    );
                }
                Ok(_) => {}
                Err(e) => warn!(error = %e, "record retention cleanup failed"),
            }
        }
        RetentionPolicy::Count(max_count) => match db.cleanup_records_by_count(*max_count) {
            Ok(count) if count > 0 => {
                info!(
                    count,
                    max_count, "cleaned up transcription records (count policy)"
                );
            }
            Ok(_) => {}
            Err(e) => warn!(error = %e, "record retention count cleanup failed"),
        },
        RetentionPolicy::DiskLimitMb(_) => {
            // Disk-limit policy for records is handled via the audio cleanup.
            // The records themselves are tiny (in SQLite), so we only enforce
            // disk limits on the audio files. Records without audio are not
            // meaningful to limit by disk size.
        }
        RetentionPolicy::Unlimited => {
            // No cleanup needed.
        }
    }
}

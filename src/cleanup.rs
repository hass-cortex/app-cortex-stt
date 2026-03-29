use std::path::PathBuf;
use std::sync::Arc;

use tokio::task::JoinHandle;
use tokio::time::{Duration, interval};
use tracing::{info, warn};

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

            // Delete old audio files.
            let audio_days = settings.audio_retention_days as i64;
            if audio_days > 0 {
                match db.get_audio_paths_older_than_days(audio_days) {
                    Ok(paths) => {
                        for filename in &paths {
                            let path = audio_dir.join(filename);
                            let _ = tokio::fs::remove_file(&path).await;
                        }
                        if !paths.is_empty() {
                            info!(
                                count = paths.len(),
                                days = audio_days,
                                "cleaned up old audio files"
                            );
                        }
                    }
                    Err(e) => warn!(error = %e, "audio retention cleanup query failed"),
                }
            }

            // Delete old records.
            let record_days = settings.record_retention_days as i64;
            if record_days > 0 {
                match db.cleanup_records_older_than_days(record_days) {
                    Ok(count) if count > 0 => {
                        info!(
                            count,
                            days = record_days,
                            "cleaned up old transcription records"
                        );
                    }
                    Ok(_) => {}
                    Err(e) => warn!(error = %e, "record retention cleanup failed"),
                }
            }
        }
    })
}

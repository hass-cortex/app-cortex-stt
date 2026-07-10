use std::sync::Arc;

use tokio::task::JoinHandle;
use tokio::time::Duration;
use tracing::{info, warn};

use crate::db::database::Database;
use crate::history::History;

/// Spawn a background task that periodically applies retention policies
/// to the transcription history. First cycle runs after 1 hour, then
/// repeats hourly.
///
/// `record_retention` drives Delete record; `audio_retention` drives
/// Drop audio. The two policies are independent — see CONTEXT.md. The
/// gather → select → apply flow lives on `History::run_retention_sweep`;
/// this task only owns the schedule and settings load.
pub fn spawn_retention_cleanup(db: Arc<Database>, history: Arc<History>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;

            let settings = match db.load_settings().await {
                Ok(s) => s,
                Err(e) => {
                    warn!(error = %e, "failed to load settings for retention sweep");
                    continue;
                }
            };

            let outcome = history
                .run_retention_sweep(&settings.record_retention, &settings.audio_retention)
                .await;
            if outcome.deleted_records > 0 || outcome.dropped_audios > 0 {
                info!(
                    deleted_records = outcome.deleted_records,
                    dropped_audios = outcome.dropped_audios,
                    "retention sweep completed"
                );
            }
        }
    })
}

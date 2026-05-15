use std::sync::Arc;

use tokio::task::JoinHandle;
use tokio::time::Duration;
use tracing::{info, warn};

use crate::db::database::Database;
use crate::history::History;
use crate::retention::select_to_delete;

/// Spawn a background task that periodically applies retention policies
/// to the transcription history. First cycle runs after 1 hour, then
/// repeats hourly.
///
/// `record_retention` drives Delete record; `audio_retention` drives
/// Drop audio. The two policies are independent — see CONTEXT.md.
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

            run_record_retention(&history, &settings.record_retention).await;
            run_audio_retention(&history, &settings.audio_retention).await;
        }
    })
}

async fn run_record_retention(history: &Arc<History>, policy: &crate::retention::RetentionPolicy) {
    let candidates = match history.list_record_candidates().await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to enumerate record retention candidates");
            return;
        }
    };
    let ids = select_to_delete(&candidates, policy);
    if ids.is_empty() {
        return;
    }
    match history.delete_many(&ids).await {
        Ok(deleted) if deleted > 0 => info!(count = deleted, "deleted history records"),
        Ok(_) => {}
        Err(e) => warn!(error = %e, "failed to delete history records"),
    }
}

async fn run_audio_retention(history: &Arc<History>, policy: &crate::retention::RetentionPolicy) {
    let candidates = match history.list_audio_candidates().await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to enumerate audio retention candidates");
            return;
        }
    };
    let ids = select_to_delete(&candidates, policy);
    if ids.is_empty() {
        return;
    }
    match history.drop_audios(&ids).await {
        Ok(dropped) if dropped > 0 => info!(count = dropped, "dropped audio for history records"),
        Ok(_) => {}
        Err(e) => warn!(error = %e, "failed to drop audio for history records"),
    }
}

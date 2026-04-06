use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{RwLock, broadcast};

use crate::db::database::Database;
use crate::engine::manager::EngineManager;
use crate::model::manager::ModelManager;

/// Status of an asynchronous transcription job.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum AsyncJobStatus {
    /// Job is queued or actively running.
    Processing,
    /// Job completed successfully.
    Completed {
        result: crate::api::transcribe::TranscribeResponse,
    },
    /// Job failed with an error.
    Failed { error: String },
    /// Job was cancelled by the client.
    Cancelled,
}

/// An asynchronous transcription job.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AsyncJob {
    pub id: String,
    pub model: String,
    #[serde(flatten)]
    pub status: AsyncJobStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// In-memory store for async transcription jobs.
#[derive(Debug, Default)]
pub struct JobStore {
    jobs: RwLock<HashMap<String, AsyncJob>>,
}

impl JobStore {
    /// Create a new empty job store.
    pub fn new() -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
        }
    }

    /// Insert a new job.
    pub async fn insert(&self, job: AsyncJob) {
        self.jobs.write().await.insert(job.id.clone(), job);
    }

    /// Get a job by ID.
    pub async fn get(&self, id: &str) -> Option<AsyncJob> {
        self.jobs.read().await.get(id).cloned()
    }

    /// Update a job's status.
    pub async fn update_status(&self, id: &str, status: AsyncJobStatus) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(id) {
            let completed_at = match &status {
                AsyncJobStatus::Completed { .. }
                | AsyncJobStatus::Failed { .. }
                | AsyncJobStatus::Cancelled => Some(chrono::Utc::now()),
                AsyncJobStatus::Processing => None,
            };
            job.status = status;
            job.completed_at = completed_at;
        }
    }

    /// Remove a job by ID. Returns `true` if it existed.
    pub async fn remove(&self, id: &str) -> bool {
        self.jobs.write().await.remove(id).is_some()
    }
}

/// Shared application state accessible across the HTTP server.
#[derive(Clone)]
pub struct AppState {
    pub engine_manager: Arc<EngineManager>,
    pub model_manager: Arc<ModelManager>,
    pub db: Arc<Database>,
    pub job_store: Arc<JobStore>,
    pub data_dir: PathBuf,
    pub default_model: String,
    pub version: String,
    pub started_at: Instant,
    /// Broadcast channel for notifying SSE clients of new history records.
    pub history_tx: broadcast::Sender<()>,
}

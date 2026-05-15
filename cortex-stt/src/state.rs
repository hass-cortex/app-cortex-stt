use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::db::database::Database;
use crate::engine::manager::EngineManager;
use crate::history::History;
use crate::model::catalog::ModelCatalog;
use crate::model::downloads::Downloads;
use crate::transcriber::Transcriber;

/// Default maximum number of jobs (any status) retained in memory.
pub const JOB_STORE_DEFAULT_MAX: usize = 100;
/// Default time-to-live for terminal jobs after their `completed_at`.
pub const JOB_STORE_DEFAULT_TTL_SECS: u64 = 600;

/// Status of an asynchronous transcription job.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum AsyncJobStatus {
    /// Job is queued or actively running.
    Processing,
    /// Job completed successfully.
    Completed {
        result: crate::transcriber::TranscribeResponse,
    },
    /// Job failed with an error.
    Failed { error: String },
    /// Job was cancelled by the client.
    Cancelled,
}

impl AsyncJobStatus {
    /// Whether the job has reached a terminal state.
    fn is_terminal(&self) -> bool {
        !matches!(self, AsyncJobStatus::Processing)
    }
}

/// An asynchronous transcription job.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AsyncJob {
    pub id: String,
    pub model: String,
    #[serde(flatten)]
    pub status: AsyncJobStatus,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

/// In-memory store for async transcription jobs.
///
/// Jobs accumulate as transcription requests come in. To prevent unbounded
/// growth, terminal jobs (Completed / Failed / Cancelled) are pruned by:
///   - **TTL**: removed once `completed_at` is older than `completed_ttl`.
///   - **Cap**: when the store exceeds `max_jobs`, the oldest terminal jobs
///     are evicted first; in-flight `Processing` jobs are never evicted.
///
/// Sweep is invoked automatically on `insert` and by an external periodic
/// task (see [`spawn_job_sweeper`]).
#[derive(Debug)]
pub struct JobStore {
    jobs: RwLock<HashMap<String, AsyncJob>>,
    max_jobs: usize,
    completed_ttl: Duration,
}

impl JobStore {
    /// Create a new job store with explicit retention parameters.
    pub fn new(max_jobs: usize, completed_ttl: Duration) -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
            max_jobs,
            completed_ttl,
        }
    }

    /// Create a job store with default retention parameters.
    pub fn with_defaults() -> Self {
        Self::new(
            JOB_STORE_DEFAULT_MAX,
            Duration::from_secs(JOB_STORE_DEFAULT_TTL_SECS),
        )
    }

    /// Insert a new job. Runs a sweep first if the store is at capacity so
    /// the new job has room. The whole insert+sweep operation happens under
    /// a single write lock to avoid the read→sweep→write race window.
    ///
    /// The sweep targets `max_jobs - 1` (saturating) so there is room for
    /// the incoming job afterwards — without this, hitting the cap with
    /// fresh terminal jobs would leave the store at `max_jobs + 1` until
    /// the next periodic sweep.
    pub async fn insert(&self, job: AsyncJob) {
        let mut jobs = self.jobs.write().await;
        if jobs.len() >= self.max_jobs {
            let target = self.max_jobs.saturating_sub(1);
            Self::sweep_locked(&mut jobs, target, self.completed_ttl);
        }
        jobs.insert(job.id.clone(), job);
    }

    /// Get a job by ID.
    pub async fn get(&self, id: &str) -> Option<AsyncJob> {
        self.jobs.read().await.get(id).cloned()
    }

    /// Update a job's status.
    pub async fn update_status(&self, id: &str, status: AsyncJobStatus) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(id) {
            let completed_at = if status.is_terminal() {
                Some(Utc::now())
            } else {
                None
            };
            job.status = status;
            job.completed_at = completed_at;
        }
    }

    /// Remove a job by ID. Returns `true` if it existed.
    pub async fn remove(&self, id: &str) -> bool {
        self.jobs.write().await.remove(id).is_some()
    }

    /// Remove terminal jobs older than `completed_ttl`, then if still over
    /// capacity, drop the oldest terminal jobs (by `created_at`) until back
    /// under `max_jobs`. `Processing` jobs are never evicted.
    ///
    /// Takes a read lock first to short-circuit when nothing needs cleaning
    /// — the periodic sweeper on an idle system pays only one atomic read.
    pub async fn sweep(&self) {
        let cutoff =
            Utc::now() - chrono::Duration::from_std(self.completed_ttl).unwrap_or_default();
        let needs_work = {
            let jobs = self.jobs.read().await;
            jobs.len() > self.max_jobs
                || jobs.values().any(|j| {
                    j.status.is_terminal() && matches!(j.completed_at, Some(ts) if ts <= cutoff)
                })
        };
        if !needs_work {
            return;
        }
        let mut jobs = self.jobs.write().await;
        Self::sweep_locked(&mut jobs, self.max_jobs, self.completed_ttl);
    }

    /// Same as [`sweep`](Self::sweep) but operates on an already-locked map.
    /// Factored out so [`insert`](Self::insert) can reuse the write lock it
    /// already holds.
    ///
    /// `target_size` is the maximum permitted `jobs.len()` after the cap
    /// pass. Periodic sweeps pass `max_jobs`; insert-time sweeps pass
    /// `max_jobs - 1` so there is room for the incoming job.
    fn sweep_locked(
        jobs: &mut HashMap<String, AsyncJob>,
        target_size: usize,
        completed_ttl: Duration,
    ) {
        let cutoff = Utc::now() - chrono::Duration::from_std(completed_ttl).unwrap_or_default();

        // TTL pass: drop terminal jobs whose completed_at is past the cutoff.
        // Terminal jobs without a completed_at timestamp are kept and will
        // age out on a subsequent sweep once `update_status` stamps them.
        jobs.retain(|_, job| {
            !job.status.is_terminal() || job.completed_at.is_none_or(|ts| ts > cutoff)
        });

        // Capacity pass: evict oldest terminal jobs until at or below target.
        if jobs.len() > target_size {
            let mut terminal: Vec<(String, DateTime<Utc>)> = jobs
                .iter()
                .filter(|(_, j)| j.status.is_terminal())
                .map(|(id, j)| (id.clone(), j.created_at))
                .collect();
            terminal.sort_by_key(|(_, ts)| *ts);

            let excess = jobs.len().saturating_sub(target_size);
            for (id, _) in terminal.into_iter().take(excess) {
                jobs.remove(&id);
            }
        }
    }

    /// Current number of jobs in the store (any status). Exposed for tests
    /// and for diagnostics.
    pub async fn len(&self) -> usize {
        self.jobs.read().await.len()
    }

    /// Whether the store currently holds no jobs.
    pub async fn is_empty(&self) -> bool {
        self.jobs.read().await.is_empty()
    }
}

impl Default for JobStore {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Spawn a background task that periodically calls [`JobStore::sweep`].
/// Runs every 60 seconds — short enough to keep memory bounded, long
/// enough that the cost is negligible.
pub fn spawn_job_sweeper(job_store: Arc<JobStore>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        ticker.tick().await; // first tick is immediate; skip it
        loop {
            ticker.tick().await;
            job_store.sweep().await;
        }
    })
}

/// Shared application state accessible across the HTTP server.
#[derive(Clone)]
pub struct AppState {
    pub engine_manager: Arc<EngineManager>,
    pub catalog: Arc<ModelCatalog>,
    pub downloads: Arc<Downloads>,
    pub db: Arc<Database>,
    pub job_store: Arc<JobStore>,
    pub data_dir: PathBuf,
    pub default_model: String,
    pub version: String,
    pub http_port: u16,
    pub started_at: Instant,
    /// Transcription history store (DB rows + WAV files + live updates).
    pub history: Arc<History>,
    /// Transcription pipeline (engine acquire → inference → save).
    pub transcriber: Arc<Transcriber>,
}

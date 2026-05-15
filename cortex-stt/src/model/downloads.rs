//! Download coordination: a concurrency-bounded queue, live progress
//! snapshots, and active task handles (so we can cancel).
//!
//! The actual HTTP + SHA-256 + archive-extract pipeline lives in
//! [`crate::model::download`]; this module hands work to it and
//! tracks the state.
//!
//! Lifecycle:
//!   1. Caller submits a [`QueuedDownloadRequest`] to
//!      [`Downloads::try_claim_slot`]. If concurrency is below the
//!      cap, the request is returned for the caller to launch and the
//!      active count is bumped. Otherwise the request is parked in
//!      the queue and `None` is returned.
//!   2. Once the task is spawned, the caller registers it with
//!      [`Downloads::register_active`] so cancellation can find both
//!      the handle and the destination path (for `.part` cleanup).
//!   3. While downloading, the task reports progress via
//!      [`set_progress`](Downloads::set_progress) — read back by SSE
//!      via [`get_progress`](Downloads::get_progress).
//!   4. On completion or failure, the task calls
//!      [`on_finished`](Downloads::on_finished); the returned next
//!      request (if any) is launched by the caller.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::error::AsrError;
use crate::model::types::DownloadProgress;

/// Maximum number of concurrent model downloads.
const MAX_CONCURRENT_DOWNLOADS: usize = 3;

/// A pending download request waiting in the queue.
pub struct QueuedDownloadRequest {
    pub model_id: String,
    pub url: String,
    pub dest_path: PathBuf,
    pub sha256: String,
}

/// Internal state for the download queue.
struct DownloadQueue {
    pending: VecDeque<QueuedDownloadRequest>,
    active: usize,
}

/// Tracking entry for an active (running) download.
struct ActiveDownload {
    handle: JoinHandle<()>,
    dest_path: PathBuf,
}

/// Coordinates concurrent model downloads.
pub struct Downloads {
    model_dir: PathBuf,
    queue: Mutex<DownloadQueue>,
    progress: RwLock<HashMap<String, DownloadProgress>>,
    active: RwLock<HashMap<String, ActiveDownload>>,
}

impl Downloads {
    pub fn new(model_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            model_dir,
            queue: Mutex::new(DownloadQueue {
                pending: VecDeque::new(),
                active: 0,
            }),
            progress: RwLock::new(HashMap::new()),
            active: RwLock::new(HashMap::new()),
        })
    }

    /// Directory under which model files (and their `.part` siblings)
    /// live. Exposed for callers that need to build a destination path
    /// from a builtin registry filename.
    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    // -----------------------------------------------------------------
    // Queue + slot management
    // -----------------------------------------------------------------

    /// Try to claim an active download slot. If the concurrency limit
    /// is reached, queues the request (and records its progress as
    /// `Queued`) and returns `None`. Otherwise increments the active
    /// count and returns the original request for the caller to launch.
    pub async fn try_claim_slot(
        &self,
        request: QueuedDownloadRequest,
    ) -> Option<QueuedDownloadRequest> {
        let mut q = self.queue.lock().await;
        if q.active < MAX_CONCURRENT_DOWNLOADS {
            q.active += 1;
            Some(request)
        } else {
            let model_id = request.model_id.clone();
            q.pending.push_back(request);
            drop(q);
            self.set_progress(DownloadProgress {
                model_id,
                status: crate::model::types::DownloadPhase::Queued,
                downloaded_bytes: 0,
                total_bytes: 0,
                speed_bps: 0.0,
                eta_secs: None,
                error: None,
            })
            .await;
            None
        }
    }

    /// Called when a download task finishes (success OR failure).
    /// Decrements the active count and returns the next queued request
    /// (with an active slot already claimed for it) if any.
    pub async fn on_finished(&self) -> Option<QueuedDownloadRequest> {
        let mut q = self.queue.lock().await;
        q.active = q.active.saturating_sub(1);
        if let Some(request) = q.pending.pop_front() {
            q.active += 1;
            Some(request)
        } else {
            None
        }
    }

    /// Release an active slot WITHOUT popping the queue. Used when a
    /// download fails to start after claiming a slot.
    pub async fn release_slot(&self) {
        let mut q = self.queue.lock().await;
        q.active = q.active.saturating_sub(1);
    }

    // -----------------------------------------------------------------
    // Progress tracking
    // -----------------------------------------------------------------

    pub async fn set_progress(&self, progress: DownloadProgress) {
        self.progress
            .write()
            .await
            .insert(progress.model_id.clone(), progress);
    }

    pub async fn get_progress(&self, model_id: &str) -> Option<DownloadProgress> {
        self.progress.read().await.get(model_id).cloned()
    }

    pub async fn remove_progress(&self, model_id: &str) {
        self.progress.write().await.remove(model_id);
    }

    /// Whether a model is currently being downloaded or queued.
    pub async fn is_downloading(&self, model_id: &str) -> bool {
        self.progress.read().await.contains_key(model_id)
    }

    // -----------------------------------------------------------------
    // Active task registration + cancellation
    // -----------------------------------------------------------------

    /// Register a running download task so cancellation can find it.
    /// Stores both the abort handle and the destination path; the
    /// latter is used to clean up the `.part` file on cancel without
    /// re-deriving the path from registry metadata.
    pub async fn register_active(
        &self,
        model_id: String,
        handle: JoinHandle<()>,
        dest_path: PathBuf,
    ) {
        self.active
            .write()
            .await
            .insert(model_id, ActiveDownload { handle, dest_path });
    }

    /// Cancel an in-progress or queued download.
    ///
    /// - If the model is in the queue (not yet started), removes it.
    ///   Returns `Ok(false)` since no active slot was occupied.
    /// - If it's running, aborts the task, cleans up the `.part` file,
    ///   releases the slot, and returns `Ok(true)` (caller should
    ///   trigger the next queued download).
    /// - If neither, returns [`AsrError::ModelNotFound`].
    pub async fn cancel(&self, model_id: &str) -> Result<bool, AsrError> {
        // Queued case: remove from pending, no slot to release.
        {
            let mut q = self.queue.lock().await;
            if let Some(idx) = q.pending.iter().position(|r| r.model_id == model_id) {
                q.pending.remove(idx);
                drop(q);
                self.remove_progress(model_id).await;
                info!(model_id = %model_id, "queued download cancelled");
                return Ok(false);
            }
        }

        // Active case.
        let entry = self.active.write().await.remove(model_id);
        let entry = match entry {
            Some(e) => e,
            None => {
                if !self.is_downloading(model_id).await {
                    return Err(AsrError::ModelNotFound {
                        model_id: model_id.to_string(),
                    });
                }
                // Progress exists but no handle — clean up progress
                // and release a slot anyway. Slot-release without
                // handle is safest.
                self.remove_progress(model_id).await;
                self.release_slot().await;
                return Ok(true);
            }
        };

        entry.handle.abort();
        self.remove_progress(model_id).await;
        cleanup_part_file(&entry.dest_path).await;
        self.release_slot().await;

        info!(model_id = %model_id, "active download cancelled");
        Ok(true)
    }
}

/// Remove the `.part` sibling of `dest_path`, if present. Missing
/// files are silent (idempotent cleanup).
async fn cleanup_part_file(dest_path: &Path) {
    let part_path = dest_path.with_extension(
        dest_path
            .extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    );

    match tokio::fs::remove_file(&part_path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warn!(
            path = %part_path.display(),
            error = %e,
            "failed to remove .part file during cancellation"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::DownloadPhase;

    #[tokio::test]
    async fn progress_tracking_lifecycle() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = Downloads::new(tmp.path().to_path_buf());

        assert!(!downloads.is_downloading("test-model").await);

        downloads
            .set_progress(DownloadProgress {
                model_id: "test-model".to_string(),
                status: DownloadPhase::Downloading,
                downloaded_bytes: 100,
                total_bytes: 1000,
                speed_bps: 50.0,
                eta_secs: Some(18.0),
                error: None,
            })
            .await;

        assert!(downloads.is_downloading("test-model").await);
        let progress = downloads.get_progress("test-model").await.unwrap();
        assert_eq!(progress.downloaded_bytes, 100);

        downloads.remove_progress("test-model").await;
        assert!(!downloads.is_downloading("test-model").await);
    }

    #[tokio::test]
    async fn cancel_queued_request_removes_it_from_pending() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = Downloads::new(tmp.path().to_path_buf());

        // Fill the active slots so the next request is queued.
        for i in 0..MAX_CONCURRENT_DOWNLOADS {
            let req = QueuedDownloadRequest {
                model_id: format!("active-{i}"),
                url: "https://example.com/a".to_string(),
                dest_path: tmp.path().join(format!("a-{i}.bin")),
                sha256: String::new(),
            };
            assert!(downloads.try_claim_slot(req).await.is_some());
        }
        // This one queues.
        let queued = QueuedDownloadRequest {
            model_id: "queued-1".to_string(),
            url: "https://example.com/q".to_string(),
            dest_path: tmp.path().join("q.bin"),
            sha256: String::new(),
        };
        assert!(downloads.try_claim_slot(queued).await.is_none());
        assert!(downloads.is_downloading("queued-1").await);

        // Cancel the queued one — returns false (no active slot released).
        let released = downloads.cancel("queued-1").await.unwrap();
        assert!(!released);
        assert!(!downloads.is_downloading("queued-1").await);
    }
}

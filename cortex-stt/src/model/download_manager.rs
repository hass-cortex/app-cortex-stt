//! Download coordination: a concurrency-bounded queue, live progress
//! snapshots, and cooperative cancellation of in-flight downloads.
//!
//! The actual HTTP + SHA-256 pipeline lives in
//! [`crate::model::download`]; this module hands work to it and
//! tracks the state.
//!
//! ## Slot accounting
//!
//! A download occupies one concurrency slot from the moment its slot is
//! claimed until it finishes. The set of in-flight downloads lives in a
//! single map under one lock, so `active.len()` IS the live slot count —
//! there is no separate counter that could drift out of sync with it.
//! Releasing a slot is *idempotent per download*: it is keyed on removing
//! that download's entry, so a normal completion and a racing cancel can
//! never both free the same slot. At most one download per model is in
//! flight at a time: [`try_claim_slot`](DownloadManager::try_claim_slot)
//! does the duplicate check AND the claim under the one lock, so two
//! concurrent submissions for the same model can't both start, and the
//! model_id is a safe key for every later operation.
//!
//! ## Cancellation
//!
//! Each download carries a shared [`AtomicBool`] cancel flag, recorded in
//! its `active` entry at claim time and also held by the running task —
//! so a cancel can reach a download even before its task has spawned, and
//! there is no window in which one is unstoppable.
//!
//! [`cancel`](DownloadManager::cancel) owns the ENTIRE teardown; the task
//! only reads the flag and stops, touching no shared state (so a stalled
//! or mid-verify task can't leave a `.part` behind, and a draining task
//! can't tear down a same-model re-download). For an active download
//! cancel runs in three phases so a re-download can't race the cleanup:
//! signal the flag while LEAVING the entry as a barrier (re-downloads stay
//! rejected); remove progress + delete the `.part`; then remove the entry
//! and promote the next queued request — skipping that last step if a
//! racing completion already removed the entry.
//!
//! Lifecycle:
//!   1. Caller submits a [`QueuedDownloadRequest`] to
//!      [`try_claim_slot`](DownloadManager::try_claim_slot), which returns
//!      [`ClaimOutcome::Claimed`] (launch it), `Queued` (parked, `Queued`
//!      progress recorded), or `AlreadyActive` (a duplicate → reject).
//!   2. The caller launches the download task, passing it the same cancel
//!      flag the request carries.
//!   3. While downloading, the task reports progress via
//!      [`set_progress`](DownloadManager::set_progress) — read back by SSE
//!      via [`get_progress`](DownloadManager::get_progress).
//!   4. On completion or failure the task calls
//!      [`finish`](DownloadManager::finish); on user cancel the handler
//!      calls [`cancel`](DownloadManager::cancel). Both release the slot
//!      exactly once and hand back the next queued request (if any) for
//!      the caller to launch.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{Mutex, RwLock};
use tracing::info;

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
    /// Shared with the running download task. Setting it asks the task to
    /// stop cooperatively at its next chunk boundary. Recorded in the
    /// `active` map at claim time so a cancel can reach the download even
    /// before its task has spawned.
    pub cancel_flag: Arc<AtomicBool>,
}

impl QueuedDownloadRequest {
    /// Build a request with a fresh (un-cancelled) cancel flag.
    pub fn new(model_id: String, url: String, dest_path: PathBuf, sha256: String) -> Self {
        Self {
            model_id,
            url,
            dest_path,
            sha256,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Per-download state held while a download occupies a slot: its cancel
/// flag and where its file lands (so `cancel` can remove the `.part`
/// without re-deriving it from registry metadata).
struct ActiveDownload {
    cancel_flag: Arc<AtomicBool>,
    dest_path: PathBuf,
}

impl ActiveDownload {
    fn from_request(request: &QueuedDownloadRequest) -> Self {
        Self {
            cancel_flag: Arc::clone(&request.cancel_flag),
            dest_path: request.dest_path.clone(),
        }
    }
}

/// Outcome of [`DownloadManager::try_claim_slot`].
pub enum ClaimOutcome {
    /// A slot was claimed; the caller must launch this request.
    Claimed(QueuedDownloadRequest),
    /// The cap was reached; the request was parked in the queue.
    Queued,
    /// A download for this model is already active or queued.
    AlreadyActive,
}

/// Slot + queue state, guarded as a unit by a single lock so the live
/// download set and the queue can never disagree. `active.len()` is the
/// authoritative concurrency count.
struct DownloadQueue {
    pending: VecDeque<QueuedDownloadRequest>,
    active: HashMap<String, ActiveDownload>,
}

impl DownloadQueue {
    /// Whether a download for `model_id` is already active or queued.
    fn contains(&self, model_id: &str) -> bool {
        self.active.contains_key(model_id) || self.pending.iter().any(|r| r.model_id == model_id)
    }

    /// Promote the next queued request to active, claiming its slot, and
    /// return it for the caller to launch. `None` if the queue is empty.
    fn claim_next(&mut self) -> Option<QueuedDownloadRequest> {
        let next = self.pending.pop_front()?;
        self.active
            .insert(next.model_id.clone(), ActiveDownload::from_request(&next));
        Some(next)
    }
}

/// Coordinates concurrent model downloads.
pub struct DownloadManager {
    model_dir: PathBuf,
    queue: Mutex<DownloadQueue>,
    progress: RwLock<HashMap<String, DownloadProgress>>,
}

impl DownloadManager {
    pub fn new(model_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            model_dir,
            queue: Mutex::new(DownloadQueue {
                pending: VecDeque::new(),
                active: HashMap::new(),
            }),
            progress: RwLock::new(HashMap::new()),
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

    /// Try to claim a download slot for `request`. The duplicate check and
    /// the claim happen under one lock, so two concurrent submissions for
    /// the same model can't both slip through:
    ///
    /// - already active/queued → [`ClaimOutcome::AlreadyActive`];
    /// - below the cap → records the download as active (claiming its slot
    ///   and cancel flag) and returns [`ClaimOutcome::Claimed`] for the
    ///   caller to launch;
    /// - at the cap → parks it (recording `Queued` progress) and returns
    ///   [`ClaimOutcome::Queued`].
    pub async fn try_claim_slot(&self, request: QueuedDownloadRequest) -> ClaimOutcome {
        let mut q = self.queue.lock().await;
        if q.contains(&request.model_id) {
            return ClaimOutcome::AlreadyActive;
        }
        if q.active.len() < MAX_CONCURRENT_DOWNLOADS {
            q.active.insert(
                request.model_id.clone(),
                ActiveDownload::from_request(&request),
            );
            ClaimOutcome::Claimed(request)
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
            ClaimOutcome::Queued
        }
    }

    /// Release the slot held by `model_id` (normal completion or failure
    /// to start) and hand back the next queued request — with its slot
    /// already claimed — if any.
    ///
    /// Idempotent per download: the slot is freed only by the caller that
    /// actually removes the active entry. A second call for an already
    /// released download is a no-op (`None`), so a normal completion and a
    /// concurrent [`cancel`](Self::cancel) can never double-free a slot.
    pub async fn finish(&self, model_id: &str) -> Option<QueuedDownloadRequest> {
        let mut q = self.queue.lock().await;
        q.active.remove(model_id)?;
        q.claim_next()
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
    // Cancellation
    // -----------------------------------------------------------------

    /// Cancel an in-progress or queued download, returning the next queued
    /// request to launch (if cancelling an active download freed a slot and
    /// one was waiting), else `None`.
    ///
    /// Cancel owns the whole teardown so the signalled task can touch
    /// nothing (see the wrapper in `download.rs`). For an active download
    /// it runs in three phases so a same-model re-download can never race
    /// the cleanup:
    ///
    /// 1. Under the lock, set the cancel flag but LEAVE the active entry in
    ///    place — [`try_claim_slot`] treats the model as still in-flight, so
    ///    a re-download is rejected until teardown completes.
    /// 2. With the entry still blocking, remove progress and delete the
    ///    `.part` file.
    /// 3. Under the lock again, remove the entry (freeing the slot) and
    ///    promote the next queued request. Skipped if the entry already
    ///    vanished — a normal completion won the race and owns the release,
    ///    so we must not double-promote.
    ///
    /// Cancelling a download that already finished (entry gone, progress
    /// lingering) is a successful no-op; an unknown id is
    /// [`AsrError::ModelNotFound`].
    pub async fn cancel(&self, model_id: &str) -> Result<Option<QueuedDownloadRequest>, AsrError> {
        // Phase 1: classify + signal, keeping the active entry as a barrier.
        let dest_path = {
            let mut q = self.queue.lock().await;

            // Queued (not yet started): drop it from pending; no slot held,
            // no file on disk yet.
            if let Some(idx) = q.pending.iter().position(|r| r.model_id == model_id) {
                q.pending.remove(idx);
                drop(q);
                self.remove_progress(model_id).await;
                info!(model_id = %model_id, "queued download cancelled");
                return Ok(None);
            }

            match q.active.get(model_id) {
                Some(entry) => {
                    entry.cancel_flag.store(true, Ordering::Relaxed);
                    entry.dest_path.clone()
                }
                None => {
                    // No slot held: either already finished (progress may
                    // still linger) — a no-op — or a genuinely unknown id.
                    drop(q);
                    if self.is_downloading(model_id).await {
                        self.remove_progress(model_id).await;
                        return Ok(None);
                    }
                    return Err(AsrError::ModelNotFound {
                        model_id: model_id.to_string(),
                    });
                }
            }
        };

        // Phase 2: clean side effects while the entry still blocks a
        // re-download of this model.
        self.remove_progress(model_id).await;
        remove_part_file(&dest_path).await;

        // Phase 3: release the slot and promote the next queued request —
        // unless a racing completion already removed the entry (then it
        // owns the release and we must not promote a second time).
        let mut q = self.queue.lock().await;
        let next = if q.active.remove(model_id).is_some() {
            q.claim_next()
        } else {
            None
        };
        info!(model_id = %model_id, "active download cancelled");
        Ok(next)
    }
}

/// Remove the `.part` sibling of `dest_path`, if present. Missing files
/// are silent (idempotent). Shares [`crate::model::download::part_path`]
/// with the writer so the path can't drift.
async fn remove_part_file(dest_path: &Path) {
    let part = crate::model::download::part_path(dest_path);
    match tokio::fs::remove_file(&part).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(
            path = %part.display(),
            error = %e,
            "failed to remove .part file during cancellation"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::DownloadPhase;

    fn request(tmp: &std::path::Path, id: &str) -> QueuedDownloadRequest {
        QueuedDownloadRequest::new(
            id.to_string(),
            "https://example.com/n".to_string(),
            tmp.join(format!("{id}.bin")),
            String::new(),
        )
    }

    fn claimed(outcome: ClaimOutcome) -> bool {
        matches!(outcome, ClaimOutcome::Claimed(_))
    }

    fn queued(outcome: ClaimOutcome) -> bool {
        matches!(outcome, ClaimOutcome::Queued)
    }

    /// Claim every concurrency slot with placeholder active entries.
    async fn fill_slots(downloads: &DownloadManager, tmp: &std::path::Path) {
        for i in 0..MAX_CONCURRENT_DOWNLOADS {
            assert!(claimed(
                downloads
                    .try_claim_slot(request(tmp, &format!("active-{i}")))
                    .await
            ));
        }
    }

    #[tokio::test]
    async fn progress_tracking_lifecycle() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = DownloadManager::new(tmp.path().to_path_buf());

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
    async fn queued_when_full_then_claimed_on_finish() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = DownloadManager::new(tmp.path().to_path_buf());

        fill_slots(&downloads, tmp.path()).await;

        // Cap reached: the next request queues.
        assert!(queued(
            downloads
                .try_claim_slot(request(tmp.path(), "queued-1"))
                .await
        ));
        assert!(downloads.is_downloading("queued-1").await);

        // Finishing an active download frees its slot and promotes the
        // queued request (slot already claimed for it).
        let next = downloads.finish("active-0").await.unwrap();
        assert_eq!(next.model_id, "queued-1");
        // Still at the cap (one finished, one promoted), so a fresh claim queues.
        assert!(queued(
            downloads.try_claim_slot(request(tmp.path(), "new")).await
        ));
    }

    #[tokio::test]
    async fn cancel_queued_request_removes_it_from_pending() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = DownloadManager::new(tmp.path().to_path_buf());

        fill_slots(&downloads, tmp.path()).await;
        assert!(queued(
            downloads
                .try_claim_slot(request(tmp.path(), "queued-1"))
                .await
        ));
        assert!(downloads.is_downloading("queued-1").await);

        // Cancelling a queued download frees no slot, so nothing to launch.
        assert!(downloads.cancel("queued-1").await.unwrap().is_none());
        assert!(!downloads.is_downloading("queued-1").await);
    }

    #[tokio::test]
    async fn cancel_signals_the_download_via_shared_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = DownloadManager::new(tmp.path().to_path_buf());

        let req = request(tmp.path(), "m");
        // The same Arc the running download task would hold.
        let flag = Arc::clone(&req.cancel_flag);
        assert!(claimed(downloads.try_claim_slot(req).await));
        assert!(!flag.load(Ordering::Relaxed));

        assert!(downloads.cancel("m").await.unwrap().is_none());
        assert!(
            flag.load(Ordering::Relaxed),
            "cancel must set the shared flag so the task stops cooperatively"
        );
    }

    #[tokio::test]
    async fn cancel_active_download_frees_exactly_one_slot() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = DownloadManager::new(tmp.path().to_path_buf());

        fill_slots(&downloads, tmp.path()).await;

        // No queued work, so cancel frees the slot and returns nothing to launch.
        assert!(downloads.cancel("active-0").await.unwrap().is_none());

        // Exactly one slot is now free: the first new claim succeeds and
        // the next must queue.
        assert!(claimed(
            downloads.try_claim_slot(request(tmp.path(), "new-1")).await
        ));
        assert!(queued(
            downloads.try_claim_slot(request(tmp.path(), "new-2")).await
        ));
    }

    /// The defect the slot-accounting rework fixes: a download that
    /// finished normally (slot already freed) but is cancelled during the
    /// brief post-completion progress window must NOT free a second slot.
    #[tokio::test]
    async fn cancel_after_finish_does_not_double_free() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = DownloadManager::new(tmp.path().to_path_buf());

        fill_slots(&downloads, tmp.path()).await;

        // active-0 finishes normally (its task called finish), but its
        // terminal progress still lingers as it would for ~2s.
        downloads
            .set_progress(DownloadProgress {
                model_id: "active-0".to_string(),
                status: DownloadPhase::Completed,
                downloaded_bytes: 0,
                total_bytes: 0,
                speed_bps: 0.0,
                eta_secs: None,
                error: None,
            })
            .await;
        assert!(downloads.finish("active-0").await.is_none());

        // A cancel landing in that window is a successful no-op: the slot
        // is already free, so it must not be freed again.
        assert!(downloads.cancel("active-0").await.unwrap().is_none());

        // Net effect of finish+cancel is exactly one freed slot. A
        // double-free would let two fresh claims through.
        assert!(claimed(
            downloads.try_claim_slot(request(tmp.path(), "new-1")).await
        ));
        assert!(queued(
            downloads.try_claim_slot(request(tmp.path(), "new-2")).await
        ));
    }

    #[tokio::test]
    async fn cancel_unknown_model_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = DownloadManager::new(tmp.path().to_path_buf());
        assert!(downloads.cancel("nope").await.is_err());
    }

    #[tokio::test]
    async fn cancel_removes_the_part_file() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = DownloadManager::new(tmp.path().to_path_buf());

        let req = request(tmp.path(), "m");
        let part = crate::model::download::part_path(&req.dest_path);
        tokio::fs::write(&part, b"partial").await.unwrap();
        assert!(part.exists());

        assert!(claimed(downloads.try_claim_slot(req).await));
        assert!(downloads.cancel("m").await.unwrap().is_none());

        assert!(
            !part.exists(),
            "cancel must delete the .part file so it can't leak or clobber a re-download"
        );
    }

    #[tokio::test]
    async fn second_claim_for_same_model_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = DownloadManager::new(tmp.path().to_path_buf());

        // First claim takes a slot; a second for the SAME model is rejected
        // atomically (not queued, not a second slot) so two tasks can't run.
        assert!(claimed(
            downloads.try_claim_slot(request(tmp.path(), "m")).await
        ));
        assert!(matches!(
            downloads.try_claim_slot(request(tmp.path(), "m")).await,
            ClaimOutcome::AlreadyActive
        ));

        // A queued same-model request is also rejected (not double-queued).
        for i in 1..MAX_CONCURRENT_DOWNLOADS {
            assert!(claimed(
                downloads
                    .try_claim_slot(request(tmp.path(), &format!("other-{i}")))
                    .await
            ));
        }
        assert!(queued(
            downloads.try_claim_slot(request(tmp.path(), "q")).await
        ));
        assert!(matches!(
            downloads.try_claim_slot(request(tmp.path(), "q")).await,
            ClaimOutcome::AlreadyActive
        ));
    }
}

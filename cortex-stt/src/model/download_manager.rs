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
//! Lifecycle (all owned by this manager — callers see only
//! [`start`](DownloadManager::start) and
//! [`cancel_download`](DownloadManager::cancel_download)):
//!   1. [`start`](DownloadManager::start) resolves the catalog entry +
//!      destination path and submits a [`QueuedDownloadRequest`] to
//!      [`try_claim_slot`](DownloadManager::try_claim_slot) — claimed
//!      requests launch immediately, over-cap requests park in the queue,
//!      duplicates are rejected.
//!   2. While downloading, the task reports progress via
//!      [`set_progress`](DownloadManager::set_progress) — read back by SSE
//!      via [`get_progress`](DownloadManager::get_progress) and by the
//!      catalog through the shared [`ProgressBoard`].
//!   3. On completion or failure the task hands its terminal state to
//!      [`complete`](DownloadManager::complete), which runs the Install
//!      (success only), releases the slot, launches the next queued
//!      request, and clears the terminal progress after a grace window.
//!      On user cancel, [`cancel_download`](DownloadManager::cancel_download)
//!      owns the teardown. Every release path frees the slot exactly once
//!      and never forgets the queued-promotion launch.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::info;

use crate::error::AsrError;
use crate::model::catalog_data::find_model;
use crate::model::download::{DownloadConfig, download_model, start_queued_download};
use crate::model::install::ModelInstaller;
use crate::model::progress::ProgressBoard;
use crate::model::types::{DownloadPhase, DownloadProgress};

/// Maximum number of concurrent model downloads.
const MAX_CONCURRENT_DOWNLOADS: usize = 3;

/// How long a terminal (Completed/Failed) progress entry lingers so SSE
/// clients can observe it before it is cleared.
const PROGRESS_CLEAR_DELAY: Duration = Duration::from_secs(2);

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
    progress: Arc<ProgressBoard>,
    /// Install hook, fired on Completed by the completion tail (see
    /// [`ModelInstaller::install`]). Injected at construction; `None`
    /// only in tests that don't exercise Installs.
    installer: Option<Arc<ModelInstaller>>,
    /// SSE grace window before a terminal progress entry is cleared.
    progress_clear_delay: Duration,
}

impl DownloadManager {
    pub fn new(
        model_dir: PathBuf,
        progress: Arc<ProgressBoard>,
        installer: Option<Arc<ModelInstaller>>,
    ) -> Arc<Self> {
        Self::with_clear_delay(model_dir, progress, installer, PROGRESS_CLEAR_DELAY)
    }

    /// [`new`](Self::new) with an explicit SSE grace window — lets tests
    /// exercise the completion tail without real-time sleeps.
    fn with_clear_delay(
        model_dir: PathBuf,
        progress: Arc<ProgressBoard>,
        installer: Option<Arc<ModelInstaller>>,
        progress_clear_delay: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            model_dir,
            queue: Mutex::new(DownloadQueue {
                pending: VecDeque::new(),
                active: HashMap::new(),
            }),
            progress,
            installer,
            progress_clear_delay,
        })
    }

    /// Directory under which model files (and their `.part` siblings)
    /// live. Exposed for callers that need to build a destination path
    /// from a builtin registry filename.
    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    /// Start (or queue) a download for `model_id` at `quant` — the single
    /// public entry for the whole download flow. Resolves the catalog
    /// entry and destination path, claims a slot (rejecting a duplicate),
    /// and launches the byte pipeline whose completion tail runs the
    /// Install and releases the slot. `Ok(())` means started or queued.
    pub async fn start(
        self: &Arc<Self>,
        model_id: &str,
        quant: Option<&str>,
    ) -> Result<(), AsrError> {
        let model = find_model(model_id).ok_or_else(|| AsrError::ModelNotFound {
            model_id: model_id.to_string(),
        })?;
        let quant_name = quant.unwrap_or(&model.default_quant);
        let quant = model
            .quant(quant_name)
            .ok_or_else(|| AsrError::ProtocolError {
                detail: format!("model {model_id} has no quant {quant_name}"),
            })?;

        let request = QueuedDownloadRequest::new(
            model_id.to_string(),
            quant.url.clone(),
            self.model_dir.join(&quant.filename),
            quant.sha256.clone(),
        );

        // Atomically claim a slot, queue, or reject a duplicate. The
        // duplicate check lives inside try_claim_slot (under its lock) so
        // two concurrent submissions for the same model can't both start.
        let request = match self.try_claim_slot(request).await {
            ClaimOutcome::AlreadyActive => {
                return Err(AsrError::DownloadInProgress {
                    model_id: model_id.to_string(),
                });
            }
            ClaimOutcome::Queued => return Ok(()),
            ClaimOutcome::Claimed(request) => request,
        };

        if let Err(e) = download_model(request, self.clone(), DownloadConfig::default()).await {
            // Slot was claimed but the task never started; release it
            // (and launch anything queued) on a detached task so the
            // cap recovers without blocking the error response.
            let downloads = self.clone();
            let model_id = model_id.to_string();
            tokio::spawn(async move {
                downloads.finish_and_launch_next(&model_id).await;
            });
            return Err(e);
        }
        Ok(())
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
    async fn finish(&self, model_id: &str) -> Option<QueuedDownloadRequest> {
        let mut q = self.queue.lock().await;
        q.active.remove(model_id)?;
        q.claim_next()
    }

    /// Release the slot AND launch whatever queued request got promoted —
    /// the pairing every release path needs. Keeping it in one method means
    /// no caller can free a slot and forget the promotion (which would
    /// stall the queue).
    pub(crate) async fn finish_and_launch_next(self: &Arc<Self>, model_id: &str) {
        let next = self.finish(model_id).await;
        self.launch(next);
    }

    /// Spawn the download task for a promoted request, if any.
    fn launch(self: &Arc<Self>, next: Option<QueuedDownloadRequest>) {
        if let Some(request) = next {
            tokio::spawn(start_queued_download(request, self.clone()));
        }
    }

    /// The completion tail of a download task — success and failure paths
    /// only (a cancel's teardown is owned entirely by
    /// [`cancel_download`](Self::cancel_download)):
    ///
    /// 1. Publish the terminal progress snapshot.
    /// 2. On `Completed`, run the Install BEFORE the slot is released:
    ///    the active entry still blocks a same-model re-download, so the
    ///    quant switch can't interleave with a new download writing the
    ///    same files; `list_models` reports Downloaded throughout
    ///    (Completed progress + file on disk), so the HA reconcile
    ///    triggered by the Install's announce sees the model.
    /// 3. Release the slot and launch the next queued download.
    /// 4. After an SSE grace window, clear the terminal progress — unless
    ///    a same-model re-download admitted in that window owns the
    ///    model_id-keyed entry now (it shows a non-terminal status).
    pub(crate) async fn complete(self: &Arc<Self>, dest_path: &Path, terminal: DownloadProgress) {
        let model_id = terminal.model_id.clone();
        let completed = matches!(terminal.status, DownloadPhase::Completed);
        self.set_progress(terminal).await;

        if completed && let Some(installer) = &self.installer {
            let filename = dest_path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            installer.install(&model_id, &filename).await;
        }

        self.finish_and_launch_next(&model_id).await;

        tokio::time::sleep(self.progress_clear_delay).await;
        if self
            .get_progress(&model_id)
            .await
            .is_some_and(|p| p.status.is_terminal())
        {
            self.remove_progress(&model_id).await;
        }
    }

    // -----------------------------------------------------------------
    // Progress tracking (delegates to the shared ProgressBoard)
    // -----------------------------------------------------------------

    pub async fn set_progress(&self, progress: DownloadProgress) {
        self.progress.set(progress).await;
    }

    pub async fn get_progress(&self, model_id: &str) -> Option<DownloadProgress> {
        self.progress.get(model_id).await
    }

    pub async fn remove_progress(&self, model_id: &str) {
        self.progress.remove(model_id).await;
    }

    /// Whether a model is currently being downloaded or queued.
    pub async fn is_downloading(&self, model_id: &str) -> bool {
        self.progress.contains(model_id).await
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
    ///
    /// Public callers use [`cancel_download`](Self::cancel_download),
    /// which also launches the promoted request.
    async fn cancel(&self, model_id: &str) -> Result<Option<QueuedDownloadRequest>, AsrError> {
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

    /// Cancel a download and launch whatever queued request its freed
    /// slot promoted. The public face of [`cancel`](Self::cancel).
    pub async fn cancel_download(self: &Arc<Self>, model_id: &str) -> Result<(), AsrError> {
        let next = self.cancel(model_id).await?;
        self.launch(next);
        Ok(())
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

    /// Manager with its own board and no installer — slot/cancel tests
    /// don't exercise Installs.
    fn manager(tmp: &std::path::Path) -> Arc<DownloadManager> {
        DownloadManager::new(tmp.to_path_buf(), ProgressBoard::new(), None)
    }

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
        let downloads = manager(tmp.path());

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
        let downloads = manager(tmp.path());

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
        let downloads = manager(tmp.path());

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
        let downloads = manager(tmp.path());

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
        let downloads = manager(tmp.path());

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
        let downloads = manager(tmp.path());

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

    fn terminal(id: &str, status: DownloadPhase) -> DownloadProgress {
        DownloadProgress {
            model_id: id.to_string(),
            status,
            downloaded_bytes: 0,
            total_bytes: 0,
            speed_bps: 0.0,
            eta_secs: None,
            error: None,
        }
    }

    /// Cross-module invariant (AGENTS.md "Completed must surface as
    /// Downloaded"): during the whole completion tail — Completed progress
    /// published, Install fired, grace window not yet elapsed — a catalog
    /// sharing the ProgressBoard must report the model as Downloaded,
    /// never Downloading. The HA reconcile triggered by the Install's
    /// announce reads exactly this state; guarding only the catalog half
    /// let a clear-timing change here silently break it.
    #[tokio::test]
    async fn completion_tail_reports_downloaded_to_catalog_before_progress_clears() {
        use crate::engine::manager::{EngineManager, EngineManagerConfig};
        use crate::model::catalog::ModelCatalog;
        use crate::model::types::ModelStatus;

        let tmp = tempfile::tempdir().unwrap();
        // The verified quant file is already on disk, as after a real download.
        let model = find_model("whisper-tiny").unwrap();
        let filename = model.default_quant_file().filename.clone();
        std::fs::write(tmp.path().join(&filename), b"fake").unwrap();

        let board = ProgressBoard::new();
        let engines = EngineManager::new(EngineManagerConfig::default());
        let catalog = ModelCatalog::new(tmp.path().to_path_buf(), board.clone(), engines);
        let downloads = DownloadManager::with_clear_delay(
            tmp.path().to_path_buf(),
            board,
            None,
            Duration::from_millis(500),
        );

        assert!(claimed(
            downloads
                .try_claim_slot(request(tmp.path(), "whisper-tiny"))
                .await
        ));

        let tail = {
            let downloads = downloads.clone();
            let dest = tmp.path().join(&filename);
            tokio::spawn(async move {
                downloads
                    .complete(&dest, terminal("whisper-tiny", DownloadPhase::Completed))
                    .await;
            })
        };

        // Wait until the Completed entry is published (inside the grace window).
        let mut lingering = None;
        for _ in 0..100 {
            lingering = downloads.get_progress("whisper-tiny").await;
            if lingering
                .as_ref()
                .is_some_and(|p| p.status == DownloadPhase::Completed)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            lingering.is_some_and(|p| p.status == DownloadPhase::Completed),
            "Completed progress entry must linger through the grace window"
        );

        // The catalog must already say Downloaded, not Downloading.
        let m = catalog.get_model("whisper-tiny").await.unwrap();
        assert_eq!(m.status, ModelStatus::Downloaded);

        tail.await.unwrap();
        // After the clear, the file on disk keeps it Downloaded.
        assert_eq!(
            catalog.get_model("whisper-tiny").await.unwrap().status,
            ModelStatus::Downloaded
        );
    }

    /// The completion tail, exercised directly (previously reachable only
    /// through a real HTTP download): a Failed terminal must release the
    /// slot and clear its progress entry after the grace window.
    #[tokio::test]
    async fn complete_releases_slot_and_clears_terminal_progress() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = DownloadManager::with_clear_delay(
            tmp.path().to_path_buf(),
            ProgressBoard::new(),
            None,
            Duration::from_millis(10),
        );

        fill_slots(&downloads, tmp.path()).await;

        downloads
            .complete(
                &tmp.path().join("active-0.bin"),
                terminal("active-0", DownloadPhase::Failed),
            )
            .await;

        // Slot freed: a fresh claim succeeds.
        assert!(claimed(
            downloads.try_claim_slot(request(tmp.path(), "new-1")).await
        ));
        // Terminal progress cleared after the grace window (complete()
        // awaited it).
        assert!(downloads.get_progress("active-0").await.is_none());
    }

    /// A same-model re-download admitted during the grace window owns the
    /// model_id-keyed progress: the delayed clear must NOT delete its
    /// live (non-terminal) entry.
    #[tokio::test]
    async fn complete_does_not_clear_a_redownloads_live_progress() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = DownloadManager::with_clear_delay(
            tmp.path().to_path_buf(),
            ProgressBoard::new(),
            None,
            Duration::from_millis(50),
        );

        assert!(claimed(
            downloads.try_claim_slot(request(tmp.path(), "m")).await
        ));

        let tail = {
            let downloads = downloads.clone();
            let dest = tmp.path().join("m.bin");
            tokio::spawn(async move {
                downloads
                    .complete(&dest, terminal("m", DownloadPhase::Completed))
                    .await;
            })
        };

        // During the grace window, a re-download takes over the entry.
        tokio::time::sleep(Duration::from_millis(10)).await;
        downloads
            .set_progress(terminal("m", DownloadPhase::Downloading))
            .await;

        tail.await.unwrap();
        assert!(
            matches!(
                downloads.get_progress("m").await.map(|p| p.status),
                Some(DownloadPhase::Downloading)
            ),
            "the re-download's live progress must survive the delayed clear"
        );
    }

    #[tokio::test]
    async fn cancel_unknown_model_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = manager(tmp.path());
        assert!(downloads.cancel("nope").await.is_err());
    }

    #[tokio::test]
    async fn cancel_removes_the_part_file() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = manager(tmp.path());

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
        let downloads = manager(tmp.path());

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

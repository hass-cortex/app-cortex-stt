use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tracing::{error, info, warn};
use url::Url;

use crate::error::AsrError;
use crate::model::download_manager::{DownloadManager, QueuedDownloadRequest};
use crate::model::types::{DownloadPhase, DownloadProgress};

/// Hosts allowed for model downloads. The vendored catalog only points
/// at Hugging Face; the initial URL is validated and redirects (e.g. to
/// the HF CDN) are followed by reqwest.
pub const ALLOWED_HOSTS: &[&str] = &["huggingface.co"];

/// Configuration for the download pipeline.
pub struct DownloadConfig {
    /// Size of each read chunk in bytes (default: 64 KB).
    pub chunk_size: usize,
    /// Whether to verify the SHA-256 hash after download (default: true).
    pub verify_sha256: bool,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            chunk_size: 64 * 1024,
            verify_sha256: true,
        }
    }
}

/// Validate that a download URL uses HTTPS and points to an allowed host.
pub fn validate_download_url(url: &str) -> bool {
    let parsed = match Url::parse(url) {
        Ok(u) => u,
        Err(_) => return false,
    };

    if parsed.scheme() != "https" {
        return false;
    }

    let host = match parsed.host_str() {
        Some(h) => h,
        None => return false,
    };

    ALLOWED_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

/// Shared HTTP client for all model downloads. Built once: a fresh
/// `Client` per download would re-pay DNS/TLS setup and never reuse a
/// keep-alive connection to the same host. The timeouts bound a stalled
/// connection — a cancel frees the slot immediately, but the orphaned
/// task only ends when its network await resolves, so cap that to a
/// hung-connect / no-data window rather than forever (generous enough not
/// to trip a slow-but-live download).
pub(crate) fn http_client() -> &'static Client {
    static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .read_timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to build shared HTTP download client")
    })
}

/// The partial-download sibling of `dest_path` (e.g. `model.bin` ->
/// `model.bin.part`). The single source of truth for this path: the
/// download task writes it and [`DownloadManager::cancel`] removes it, so
/// they must agree.
pub(crate) fn part_path(dest_path: &Path) -> PathBuf {
    dest_path.with_extension(
        dest_path
            .extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    )
}

/// Whether the download was cancelled. [`DownloadManager::cancel`] sets
/// the flag, frees the slot, removes progress, and deletes the `.part`
/// file — so a task that observes this only needs to stop; it touches no
/// shared state itself (which avoids racing a same-model re-download).
/// Checked at the chunk boundary and before the verify / install phases
/// so a cancel issued between phases is honoured, not just mid-stream.
fn is_cancelled(cancel_flag: &AtomicBool, model_id: &str) -> bool {
    let cancelled = cancel_flag.load(Ordering::Relaxed);
    if cancelled {
        info!(model_id = %model_id, "download cancelled, stopping");
    }
    cancelled
}

/// Compute the SHA-256 hash of a file, reading in 64 KB chunks.
///
/// Returns the lowercase hex-encoded digest.
pub async fn compute_sha256(path: &Path) -> Result<String, AsrError> {
    let data = fs::read(path).await?;
    let mut hasher = Sha256::new();

    for chunk in data.chunks(64 * 1024) {
        hasher.update(chunk);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Result of starting a model download. The background task is detached;
/// cancellation is cooperative via the [`QueuedDownloadRequest`] cancel
/// flag held by [`DownloadManager`], not by aborting a task handle.
pub struct DownloadHandle {
    /// Watch receiver for polling progress updates.
    pub progress_rx: watch::Receiver<DownloadProgress>,
}

/// How a download task ended (distinct from a genuine error).
enum DownloadOutcome {
    /// Finished and verified; the model is installed.
    Completed,
    /// The cancel flag was observed mid-download; the partial file has
    /// been removed and the manager has already freed the slot + progress.
    Cancelled,
}

/// Start downloading a model file in a background task.
///
/// Returns a [`DownloadHandle`] containing a progress watch receiver and
/// the spawned task handle. The background task:
/// 1. Resumes from a partial `.part` file if one exists (HTTP Range header).
/// 2. Streams the response body in chunks, updating progress via the watch channel.
/// 3. Verifies SHA-256 on completion (if `expected_sha256` is non-empty and config allows).
/// 4. Deletes corrupted files on hash mismatch.
pub async fn download_model(
    url: &str,
    dest_path: PathBuf,
    expected_sha256: &str,
    model_id: &str,
    cancel_flag: Arc<AtomicBool>,
    downloads: Arc<DownloadManager>,
    config: DownloadConfig,
) -> Result<DownloadHandle, AsrError> {
    if !validate_download_url(url) {
        return Err(AsrError::DownloadFailed {
            model_id: model_id.to_string(),
            detail: format!("URL rejected: must be HTTPS with allowed host ({ALLOWED_HOSTS:?})"),
        });
    }

    let initial_progress = DownloadProgress {
        model_id: model_id.to_string(),
        status: DownloadPhase::Downloading,
        downloaded_bytes: 0,
        total_bytes: 0,
        speed_bps: 0.0,
        eta_secs: None,
        error: None,
    };

    // Register initial progress immediately so list_models() sees "downloading"
    // status before the first HTTP chunk arrives — unless this download was
    // already cancelled (a queued request cancelled in the window before its
    // promoted task ran), in which case cancel() owns the state and we must
    // not resurrect a "Downloading" entry it just cleared.
    if !cancel_flag.load(Ordering::Relaxed) {
        downloads.set_progress(initial_progress.clone()).await;
    }

    let (tx, rx) = watch::channel(initial_progress.clone());

    let url = url.to_string();
    let expected_sha256 = expected_sha256.to_string();
    let model_id = model_id.to_string();

    tokio::spawn(async move {
        let result = download_task(
            &url,
            &dest_path,
            &expected_sha256,
            &model_id,
            &cancel_flag,
            &downloads,
            &config,
            &tx,
        )
        .await;

        // The cancel flag is the single authority that cancel() owns this
        // download's teardown (slot, progress, and .part). If it is set —
        // whether the task ended Cancelled, errored because cancel deleted
        // the .part mid-verify, or even completed in the instant
        // before observing it — this task must touch NO shared state: the
        // model_id-keyed entry/progress may already belong to a same-model
        // re-download, and finish()/set_progress/remove_progress here would
        // free its slot or clobber its progress. cancel() does it all.
        if cancel_flag.load(Ordering::Relaxed) {
            return;
        }

        match &result {
            Ok(DownloadOutcome::Completed) => {
                let progress = DownloadProgress {
                    model_id: model_id.clone(),
                    status: DownloadPhase::Completed,
                    downloaded_bytes: 0,
                    total_bytes: 0,
                    speed_bps: 0.0,
                    eta_secs: None,
                    error: None,
                };
                downloads.set_progress(progress.clone()).await;
                let _ = tx.send(progress);

                // Install BEFORE finish(): the active entry still blocks a
                // same-model re-download, so the quant switch can't interleave
                // with a new download writing the same files. list_models
                // reports Downloaded throughout (Completed progress + file on
                // disk), so the HA reconcile triggered by the Install's
                // announce sees the model.
                if let Some(installer) = downloads.installer() {
                    let filename = dest_path
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    installer.install(&model_id, &filename).await;
                }
            }
            Ok(DownloadOutcome::Cancelled) => unreachable!("handled above"),
            Err(e) => {
                error!(model_id = %model_id, error = %e, "download failed");
                let progress = DownloadProgress {
                    model_id: model_id.clone(),
                    status: DownloadPhase::Failed,
                    downloaded_bytes: 0,
                    total_bytes: 0,
                    speed_bps: 0.0,
                    eta_secs: None,
                    error: Some(e.to_string()),
                };
                downloads.set_progress(progress.clone()).await;
                let _ = tx.send(progress);
            }
        }

        // Release the active slot and start the next queued download.
        launch_next(&downloads, downloads.finish(&model_id).await);

        // Brief delay so SSE clients can pick up the terminal status, then
        // clear it — but only if it is still OUR terminal entry. finish()
        // already freed the slot, so a same-model re-download admitted in
        // this window owns the model_id-keyed progress now (it shows a
        // non-terminal status); we must not delete its live progress.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if matches!(
            downloads.get_progress(&model_id).await.map(|p| p.status),
            Some(DownloadPhase::Completed | DownloadPhase::Failed)
        ) {
            downloads.remove_progress(&model_id).await;
        }
    });

    Ok(DownloadHandle { progress_rx: rx })
}

/// Launch the next queued download that a [`DownloadManager::finish`] or
/// [`DownloadManager::cancel`] handed back (its slot is already claimed),
/// if any. The single place that turns a promoted request into a running
/// task, so the finish/cancel call sites stay one-liners.
pub fn launch_next(downloads: &Arc<DownloadManager>, next: Option<QueuedDownloadRequest>) {
    if let Some(request) = next {
        tokio::spawn(start_queued_download(request, downloads.clone()));
    }
}

/// Start a queued download request. Returns a boxed future to break the
/// recursive async type cycle (download_model → on_download_finished →
/// start_queued_download → download_model).
pub fn start_queued_download(
    request: QueuedDownloadRequest,
    downloads: Arc<DownloadManager>,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(async move {
        let model_id = request.model_id.clone();
        match download_model(
            &request.url,
            request.dest_path,
            &request.sha256,
            &request.model_id,
            Arc::clone(&request.cancel_flag),
            downloads.clone(),
            DownloadConfig::default(),
        )
        .await
        {
            // Task launched; it is detached and stops via the cancel flag.
            Ok(_handle) => {}
            Err(e) => {
                error!(
                    model_id = %model_id, error = %e,
                    "failed to start queued download"
                );
                // Slot was claimed when this request was promoted; release
                // it (and launch anything else queued) so the cap recovers.
                launch_next(&downloads, downloads.finish(&model_id).await);
            }
        }
    })
}

/// The actual download logic, separated for readability.
#[allow(clippy::too_many_arguments)] // all params distinct and required; a struct adds only ceremony
async fn download_task(
    url: &str,
    dest_path: &Path,
    expected_sha256: &str,
    model_id: &str,
    cancel_flag: &AtomicBool,
    downloads: &Arc<DownloadManager>,
    config: &DownloadConfig,
    tx: &watch::Sender<DownloadProgress>,
) -> Result<DownloadOutcome, AsrError> {
    let part_path = part_path(dest_path);

    // Check for existing partial download to resume.
    let existing_bytes = if part_path.exists() {
        fs::metadata(&part_path).await?.len()
    } else {
        0
    };

    let client = http_client();
    let mut request = client.get(url);

    if existing_bytes > 0 {
        info!(
            model_id = %model_id,
            resume_from = existing_bytes,
            "resuming partial download"
        );
        request = request.header("Range", format!("bytes={existing_bytes}-"));
    }

    let response = request.send().await.map_err(|e| AsrError::DownloadFailed {
        model_id: model_id.to_string(),
        detail: format!("HTTP request failed: {e}"),
    })?;

    // Handle 416 Range Not Satisfiable — .part file is already complete.
    // Delete it and retry without Range header.
    if response.status().as_u16() == 416 && existing_bytes > 0 {
        warn!(
            model_id = %model_id,
            "416 Range Not Satisfiable — .part file likely complete, retrying fresh"
        );
        drop(response);
        let _ = fs::remove_file(&part_path).await;
        // Retry without Range
        let response2 = client
            .get(url)
            .send()
            .await
            .map_err(|e| AsrError::DownloadFailed {
                model_id: model_id.to_string(),
                detail: format!("HTTP retry failed: {e}"),
            })?;
        if !response2.status().is_success() {
            return Err(AsrError::DownloadFailed {
                model_id: model_id.to_string(),
                detail: format!("HTTP {}", response2.status()),
            });
        }
        // Re-assign and continue with fresh download
        return Box::pin(download_task(
            url,
            dest_path,
            expected_sha256,
            model_id,
            cancel_flag,
            downloads,
            config,
            tx,
        ))
        .await;
    }

    if !response.status().is_success() && response.status().as_u16() != 206 {
        return Err(AsrError::DownloadFailed {
            model_id: model_id.to_string(),
            detail: format!("HTTP {}", response.status()),
        });
    }

    // Determine total size from Content-Length (or Content-Range for resumed downloads).
    let total_bytes = if response.status().as_u16() == 206 {
        // Partial content: try Content-Range header for full size.
        response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit('/').next())
            .and_then(|s| s.parse::<u64>().ok())
    } else {
        response.content_length()
    };

    let total = total_bytes.unwrap_or(0);
    let mut downloaded_bytes = existing_bytes;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part_path)
        .await?;

    let mut stream = response.bytes_stream();
    let start_time = std::time::Instant::now();

    while let Some(chunk_result) = stream.next().await {
        if is_cancelled(cancel_flag, model_id) {
            return Ok(DownloadOutcome::Cancelled);
        }

        let chunk = chunk_result.map_err(|e| AsrError::DownloadFailed {
            model_id: model_id.to_string(),
            detail: format!("stream error: {e}"),
        })?;

        file.write_all(&chunk).await?;
        downloaded_bytes += chunk.len() as u64;

        let elapsed = start_time.elapsed().as_secs_f64();
        let net_downloaded = downloaded_bytes.saturating_sub(existing_bytes) as f64;
        let speed_bps = if elapsed > 0.0 {
            net_downloaded / elapsed
        } else {
            0.0
        };
        let eta_secs = if speed_bps > 0.0 && total > downloaded_bytes {
            Some((total - downloaded_bytes) as f64 / speed_bps)
        } else {
            None
        };

        let progress = DownloadProgress {
            model_id: model_id.to_string(),
            status: DownloadPhase::Downloading,
            downloaded_bytes,
            total_bytes: total,
            speed_bps,
            eta_secs,
            error: None,
        };

        // Re-check right before publishing: cancel() may have landed during
        // write_all and already cleared progress, and this model_id-keyed
        // entry may now belong to a same-model re-download — don't resurrect
        // a stale "Downloading" snapshot over it.
        if is_cancelled(cancel_flag, model_id) {
            return Ok(DownloadOutcome::Cancelled);
        }
        downloads.set_progress(progress.clone()).await;
        let _ = tx.send(progress);
    }

    file.flush().await?;
    drop(file);

    // Cancel may have arrived during the final chunk; don't spend time
    // verifying a download the user abandoned.
    if is_cancelled(cancel_flag, model_id) {
        return Ok(DownloadOutcome::Cancelled);
    }

    info!(
        model_id = %model_id,
        bytes = downloaded_bytes,
        "download complete, verifying"
    );

    // SHA-256 verification.
    if config.verify_sha256 && !expected_sha256.is_empty() {
        // Honour a cancel before publishing a Verifying snapshot (which
        // cancel can't clear) and before burning time hashing a (possibly
        // multi-GB) file the user abandoned.
        if is_cancelled(cancel_flag, model_id) {
            return Ok(DownloadOutcome::Cancelled);
        }
        let verifying = DownloadProgress {
            model_id: model_id.to_string(),
            status: DownloadPhase::Verifying,
            downloaded_bytes,
            total_bytes: total,
            speed_bps: 0.0,
            eta_secs: None,
            error: None,
        };
        downloads.set_progress(verifying.clone()).await;
        let _ = tx.send(verifying);

        let actual = compute_sha256(&part_path).await?;

        if actual != expected_sha256 {
            warn!(
                model_id = %model_id,
                expected = %expected_sha256,
                actual = %actual,
                "SHA-256 mismatch, deleting corrupted file"
            );
            let _ = fs::remove_file(&part_path).await;
            return Err(AsrError::DownloadFailed {
                model_id: model_id.to_string(),
                detail: format!("SHA-256 mismatch: expected {expected_sha256}, got {actual}"),
            });
        }

        info!(model_id = %model_id, "SHA-256 verified");
    }

    // Last chance to honour a cancel before we install the model
    // (rename to the final destination).
    if is_cancelled(cancel_flag, model_id) {
        return Ok(DownloadOutcome::Cancelled);
    }

    // GGUF models are single files — rename .part to final destination.
    fs::rename(&part_path, dest_path).await?;

    info!(model_id = %model_id, path = %dest_path.display(), "model download complete");
    Ok(DownloadOutcome::Completed)
}

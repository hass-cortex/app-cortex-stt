use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tracing::{error, info, warn};
use url::Url;

use crate::error::AsrError;
use crate::model::downloads::{Downloads, QueuedDownloadRequest};
use crate::model::types::{DownloadPhase, DownloadProgress};

/// Hosts allowed for model downloads.
pub const ALLOWED_HOSTS: &[&str] = &["huggingface.co", "github.com", "blob.handy.computer"];

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

/// Result of starting a model download.
pub struct DownloadHandle {
    /// Watch receiver for polling progress updates.
    pub progress_rx: watch::Receiver<DownloadProgress>,
    /// Handle to the background download task (can be used for cancellation).
    pub task_handle: tokio::task::JoinHandle<()>,
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
    downloads: Arc<Downloads>,
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

    // Register initial progress immediately so list_models() sees "downloading" status
    // before the first HTTP chunk arrives.
    downloads.set_progress(initial_progress.clone()).await;

    let (tx, rx) = watch::channel(initial_progress.clone());

    let url = url.to_string();
    let expected_sha256 = expected_sha256.to_string();
    let model_id = model_id.to_string();

    let task_handle = tokio::spawn(async move {
        let result = download_task(
            &url,
            &dest_path,
            &expected_sha256,
            &model_id,
            &downloads,
            &config,
            &tx,
        )
        .await;

        match &result {
            Ok(()) => {
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
            }
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
        if let Some(next) = downloads.on_finished().await {
            tokio::spawn(start_queued_download(next, downloads.clone()));
        }

        // Brief delay so SSE clients can pick up the terminal status.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        downloads.remove_progress(&model_id).await;
    });

    Ok(DownloadHandle {
        progress_rx: rx,
        task_handle,
    })
}

/// Start a queued download request. Returns a boxed future to break the
/// recursive async type cycle (download_model → on_download_finished →
/// start_queued_download → download_model).
pub fn start_queued_download(
    request: QueuedDownloadRequest,
    downloads: Arc<Downloads>,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(async move {
        let model_id = request.model_id.clone();
        let dest_path = request.dest_path.clone();
        match download_model(
            &request.url,
            request.dest_path,
            &request.sha256,
            &request.model_id,
            downloads.clone(),
            DownloadConfig::default(),
        )
        .await
        {
            Ok(handle) => {
                downloads
                    .register_active(model_id, handle.task_handle, dest_path)
                    .await;
            }
            Err(e) => {
                error!(
                    model_id = %model_id, error = %e,
                    "failed to start queued download"
                );
                downloads.release_slot().await;
            }
        }
    })
}

/// The actual download logic, separated for readability.
async fn download_task(
    url: &str,
    dest_path: &Path,
    expected_sha256: &str,
    model_id: &str,
    downloads: &Arc<Downloads>,
    config: &DownloadConfig,
    tx: &watch::Sender<DownloadProgress>,
) -> Result<(), AsrError> {
    let part_path = dest_path.with_extension(
        dest_path
            .extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    );

    // Check for existing partial download to resume.
    let existing_bytes = if part_path.exists() {
        fs::metadata(&part_path).await?.len()
    } else {
        0
    };

    let client = Client::new();
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

        downloads.set_progress(progress.clone()).await;
        let _ = tx.send(progress);
    }

    file.flush().await?;
    drop(file);

    info!(
        model_id = %model_id,
        bytes = downloaded_bytes,
        "download complete, verifying"
    );

    // SHA-256 verification.
    if config.verify_sha256 && !expected_sha256.is_empty() {
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

    // Handle archive extraction or simple rename.
    let url_lower = url.to_lowercase();
    let is_tar_gz = url_lower.ends_with(".tar.gz") || url_lower.ends_with(".tgz");
    let is_tar_bz2 = url_lower.ends_with(".tar.bz2") || url_lower.ends_with(".tbz2");

    if is_tar_gz || is_tar_bz2 {
        let extracting = DownloadProgress {
            model_id: model_id.to_string(),
            status: DownloadPhase::Extracting,
            downloaded_bytes,
            total_bytes: total,
            speed_bps: 0.0,
            eta_secs: None,
            error: None,
        };
        downloads.set_progress(extracting.clone()).await;
        let _ = tx.send(extracting);

        info!(model_id = %model_id, "extracting archive");
        let parent = dest_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let part_path_owned = part_path.clone();
        let dest_path_owned = dest_path.to_path_buf();
        let model_id_owned = model_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), AsrError> {
            use std::io::BufReader;
            let file = std::fs::File::open(&part_path_owned)?;

            let tmp_dir = parent.join(format!(".tmp-extract-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&tmp_dir)?;

            // Extract based on compression format
            if is_tar_gz {
                let decoder = flate2::read::GzDecoder::new(BufReader::new(file));
                let mut archive = tar::Archive::new(decoder);
                archive
                    .unpack(&tmp_dir)
                    .map_err(|e| AsrError::DownloadFailed {
                        model_id: model_id_owned.clone(),
                        detail: format!("tar.gz extraction failed: {e}"),
                    })?;
            } else {
                let decoder = bzip2::read::BzDecoder::new(BufReader::new(file));
                let mut archive = tar::Archive::new(decoder);
                archive
                    .unpack(&tmp_dir)
                    .map_err(|e| AsrError::DownloadFailed {
                        model_id: model_id_owned.clone(),
                        detail: format!("tar.bz2 extraction failed: {e}"),
                    })?;
            }

            // Unwrap single-directory nesting (e.g. archive contains dir/dir/files).
            // Only unwrap when the level contains exactly one entry AND it is a directory.
            // This prevents drilling past a top-level dir that contains both files and subdirs.
            let mut source_dir = tmp_dir.clone();
            loop {
                let entries: Vec<_> = std::fs::read_dir(&source_dir)
                    .map_err(AsrError::Io)?
                    .filter_map(|e| e.ok())
                    .collect();
                if entries.len() == 1
                    && entries[0]
                        .file_type()
                        .map(|ft| ft.is_dir())
                        .unwrap_or(false)
                {
                    source_dir = entries[0].path();
                } else {
                    break;
                }
            }

            // Rename to final destination (atomic swap)
            if dest_path_owned.exists() {
                std::fs::remove_dir_all(&dest_path_owned)?;
            }
            std::fs::rename(&source_dir, &dest_path_owned)?;

            // Clean up
            let _ = std::fs::remove_dir_all(&tmp_dir);
            let _ = std::fs::remove_file(&part_path_owned);

            Ok(())
        })
        .await
        .map_err(|e| AsrError::DownloadFailed {
            model_id: model_id.to_string(),
            detail: format!("extraction task failed: {e}"),
        })??;
    } else {
        // Simple file — just rename .part to final destination.
        fs::rename(&part_path, dest_path).await?;
    }

    info!(model_id = %model_id, path = %dest_path.display(), "model download complete");
    Ok(())
}

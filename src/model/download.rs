use std::path::{Path, PathBuf};
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
use crate::model::manager::ModelManager;
use crate::model::types::DownloadProgress;

/// Hosts allowed for model downloads.
pub const ALLOWED_HOSTS: &[&str] = &["huggingface.co", "github.com"];

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

/// Start downloading a model file in a background task.
///
/// Returns a `watch::Receiver` that the caller can poll for progress updates.
/// The background task:
/// 1. Resumes from a partial `.part` file if one exists (HTTP Range header).
/// 2. Streams the response body in chunks, updating progress via the watch channel.
/// 3. Verifies SHA-256 on completion (if `expected_sha256` is non-empty and config allows).
/// 4. Deletes corrupted files on hash mismatch.
pub fn download_model(
    url: &str,
    dest_path: PathBuf,
    expected_sha256: &str,
    model_id: &str,
    model_manager: Arc<ModelManager>,
    config: DownloadConfig,
) -> Result<watch::Receiver<DownloadProgress>, AsrError> {
    if !validate_download_url(url) {
        return Err(AsrError::DownloadFailed {
            model_id: model_id.to_string(),
            detail: format!("URL rejected: must be HTTPS with allowed host ({ALLOWED_HOSTS:?})"),
        });
    }

    let initial_progress = DownloadProgress {
        model_id: model_id.to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        percent: None,
    };

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
            &model_manager,
            &config,
            &tx,
        )
        .await;

        if let Err(e) = result {
            error!(model_id = %model_id, error = %e, "download failed");
        }

        // Clean up progress tracking regardless of outcome.
        model_manager.remove_download_progress(&model_id).await;
    });

    Ok(rx)
}

/// The actual download logic, separated for readability.
async fn download_task(
    url: &str,
    dest_path: &Path,
    expected_sha256: &str,
    model_id: &str,
    model_manager: &Arc<ModelManager>,
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

    let mut downloaded_bytes = existing_bytes;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part_path)
        .await?;

    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| AsrError::DownloadFailed {
            model_id: model_id.to_string(),
            detail: format!("stream error: {e}"),
        })?;

        file.write_all(&chunk).await?;
        downloaded_bytes += chunk.len() as u64;

        let percent = total_bytes.map(|total| {
            if total > 0 {
                (downloaded_bytes as f32 / total as f32) * 100.0
            } else {
                0.0
            }
        });

        let progress = DownloadProgress {
            model_id: model_id.to_string(),
            downloaded_bytes,
            total_bytes,
            percent,
        };

        model_manager.set_download_progress(progress.clone()).await;
        // Ignore send errors (receiver may have been dropped).
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

    // Rename .part file to final destination.
    fs::rename(&part_path, dest_path).await?;

    info!(model_id = %model_id, path = %dest_path.display(), "model download complete");
    Ok(())
}

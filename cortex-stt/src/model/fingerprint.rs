//! Installed-file fingerprints: the SHA-256 a catalog GGUF was verified
//! against, kept in a `<file>.sha256` sidecar next to it.
//!
//! Upstream can republish a file under the same name, so "the file exists"
//! does not say whether it is the one the catalog now pins. Comparing the
//! sidecar with the catalog does, without hashing gigabytes per request.

use std::path::{Path, PathBuf};

use tracing::{info, warn};

use crate::model::catalog::downloaded_quant;
use crate::model::catalog_data::catalog_models;
use crate::model::download::compute_sha256;

const SUFFIX: &str = ".sha256";

/// Sidecar path for a model file.
pub fn sidecar_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(SUFFIX);
    PathBuf::from(name)
}

/// The recorded SHA-256 of `file`, if a sidecar exists.
pub fn read(file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(sidecar_path(file)).ok()?;
    let sha = text.trim();
    (!sha.is_empty()).then(|| sha.to_ascii_lowercase())
}

/// Record `sha256` as the fingerprint of `file`. Best-effort: without a
/// sidecar the model simply reports no update.
pub async fn record(file: &Path, sha256: &str) {
    let path = sidecar_path(file);
    if let Err(e) = tokio::fs::write(&path, format!("{sha256}\n")).await {
        warn!(path = %path.display(), error = %e, "failed to record model fingerprint");
    }
}

/// Remove the sidecar of `file`, if any.
pub async fn remove(file: &Path) {
    let path = sidecar_path(file);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warn!(path = %path.display(), error = %e, "failed to remove model fingerprint"),
    }
}

/// Whether the installed file differs from what the catalog pins.
/// Unknown (no sidecar yet) counts as up to date.
pub fn update_available(file: &Path, catalog_sha256: &str) -> bool {
    !catalog_sha256.is_empty()
        && read(file).is_some_and(|sha| !sha.eq_ignore_ascii_case(catalog_sha256))
}

/// Hash every installed catalog file that has no sidecar yet — files
/// installed before fingerprints existed. Runs once at startup, in the
/// background; each file is hashed once and never again.
pub async fn backfill(model_dir: &Path) {
    for model in catalog_models() {
        let Some(quant) = downloaded_quant(model_dir, model) else {
            continue;
        };
        let file = model_dir.join(&quant.filename);
        if sidecar_path(&file).is_file() {
            continue;
        }
        match compute_sha256(&file).await {
            Ok(sha) => {
                record(&file, &sha).await;
                info!(model_id = %model.id, update_available = !sha.eq_ignore_ascii_case(&quant.sha256),
                    "fingerprinted installed model");
            }
            Err(e) => {
                warn!(model_id = %model.id, error = %e, "failed to fingerprint installed model")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_sidecar_means_no_update() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("m.gguf");
        std::fs::write(&file, b"x").unwrap();
        assert!(!update_available(&file, "abc"));
    }

    #[tokio::test]
    async fn a_different_catalog_hash_is_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("m.gguf");
        std::fs::write(&file, b"x").unwrap();
        record(&file, "abc").await;
        assert!(!update_available(&file, "ABC"));
        assert!(update_available(&file, "def"));
        remove(&file).await;
        assert!(read(&file).is_none());
    }

    #[tokio::test]
    async fn backfill_records_the_real_hash() {
        let dir = tempfile::tempdir().unwrap();
        let model = catalog_models().first().expect("catalog not empty");
        let quant = model.default_quant_file();
        let file = dir.path().join(&quant.filename);
        std::fs::write(&file, b"not the real model").unwrap();

        backfill(dir.path()).await;

        assert_eq!(read(&file), Some(compute_sha256(&file).await.unwrap()));
        assert!(update_available(&file, &quant.sha256));
    }
}

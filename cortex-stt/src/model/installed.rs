//! Installed Variants (ADR 0007): what is on disk, and what each file is.
//!
//! A catalog file lives at `{model_dir}/{model_id}/{filename}` beside a
//! `<file>.json` sidecar naming its model and quant, the sha256 it was
//! verified against, when it was installed, and a snapshot of the catalog
//! entry's display data. Identity comes from the sidecar, never from
//! looking the filename up in today's catalog — which is what lets a file
//! outlive its catalog entry as a Retired variant. A Custom model is a
//! `*.gguf` at the top level, with no sidecar.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::model::catalog_data::{CatalogCapabilities, CatalogModel, find_by_filename, find_model};
use crate::model::download::compute_sha256;

const SIDECAR_SUFFIX: &str = ".json";
/// Pre-ADR-0007 fingerprint sidecar: a bare sha256 beside a flat file.
const LEGACY_SUFFIX: &str = ".sha256";

/// One Variant's identity: a model, and its quant (`None` for a Custom model).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VariantKey {
    pub model: String,
    pub quant: Option<String>,
}

impl VariantKey {
    pub fn new(model: impl Into<String>, quant: Option<String>) -> Self {
        Self {
            model: model.into(),
            quant,
        }
    }

    /// Read a **Variant key** as a request names it: `model:quant`, or a
    /// bare model id (`quant` then `None`, to be resolved by the catalog).
    pub fn parse(key: &str) -> Self {
        match key.split_once(':') {
            Some((model, quant)) => Self::new(model, Some(quant.to_string())),
            None => Self::new(key, None),
        }
    }

    /// The flat string form, used wherever one key is needed (engine
    /// pools, download slots): `model:quant`, or `model` without a quant.
    pub fn as_key(&self) -> String {
        match &self.quant {
            Some(q) => format!("{}:{q}", self.model),
            None => self.model.clone(),
        }
    }
}

impl std::fmt::Display for VariantKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_key())
    }
}

/// The catalog entry's display data at install time, so a Retired
/// variant still shows what it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub name: String,
    pub description: String,
    pub family: String,
    pub languages: Vec<String>,
    pub capabilities: CatalogCapabilities,
}

impl Snapshot {
    pub fn of(model: &CatalogModel) -> Self {
        Self {
            name: model.name.clone(),
            description: model.description.clone(),
            family: model.family.clone(),
            languages: model.languages.clone(),
            capabilities: model.capabilities.clone(),
        }
    }
}

/// The sidecar's contents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub model: String,
    pub quant: String,
    /// The sha256 the file was verified against; `None` until known
    /// (an unverified install, or a file awaiting its startup backfill).
    #[serde(default)]
    pub sha256: Option<String>,
    /// Unix seconds.
    pub installed_at: i64,
    pub snapshot: Snapshot,
}

impl Sidecar {
    pub fn for_install(model: &CatalogModel, quant: &str, sha256: Option<String>) -> Self {
        Self {
            model: model.id.clone(),
            quant: quant.to_string(),
            sha256,
            installed_at: chrono::Utc::now().timestamp(),
            snapshot: Snapshot::of(model),
        }
    }
}

/// One installed Variant as found on disk.
#[derive(Debug, Clone)]
pub struct InstalledVariant {
    pub key: VariantKey,
    pub path: PathBuf,
    /// The verified sha256 (catalog variants only).
    pub sha256: Option<String>,
    /// Unix seconds.
    pub installed_at: i64,
    /// `None` for a Custom model.
    pub snapshot: Option<Snapshot>,
}

impl InstalledVariant {
    /// Whether the catalog still offers this Variant. A Custom model is
    /// never Retired: it was never in the catalog.
    pub fn is_retired(&self) -> bool {
        match &self.key.quant {
            Some(q) => find_model(&self.key.model)
                .and_then(|m| m.quant(q))
                .is_none(),
            None => false,
        }
    }

    /// Whether upstream republished the file under the same name. Unknown
    /// (no recorded sha256) counts as up to date; a Retired variant has
    /// nothing to update to.
    pub fn update_available(&self) -> bool {
        let Some(q) = &self.key.quant else {
            return false;
        };
        let Some(pinned) = find_model(&self.key.model).and_then(|m| m.quant(q)) else {
            return false;
        };
        self.sha256
            .as_deref()
            .is_some_and(|sha| !sha.eq_ignore_ascii_case(&pinned.sha256))
    }
}

/// The directory a catalog model's files live in.
pub fn model_subdir(model_dir: &Path, model_id: &str) -> PathBuf {
    model_dir.join(model_id)
}

/// Where a catalog Variant's file lives.
pub fn variant_path(model_dir: &Path, model_id: &str, filename: &str) -> PathBuf {
    model_subdir(model_dir, model_id).join(filename)
}

/// Sidecar path for a model file.
pub fn sidecar_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(SIDECAR_SUFFIX);
    PathBuf::from(name)
}

fn legacy_sidecar_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(LEGACY_SUFFIX);
    PathBuf::from(name)
}

pub fn read_sidecar(file: &Path) -> Option<Sidecar> {
    let text = std::fs::read_to_string(sidecar_path(file)).ok()?;
    match serde_json::from_str(&text) {
        Ok(s) => Some(s),
        Err(e) => {
            warn!(path = %file.display(), error = %e, "unreadable variant sidecar");
            None
        }
    }
}

/// Write `sidecar` for `file`. Best-effort: without one the file is still
/// recognised from the catalog, only its update state is unknown.
pub async fn write_sidecar(file: &Path, sidecar: &Sidecar) {
    let path = sidecar_path(file);
    let body = match serde_json::to_string_pretty(sidecar) {
        Ok(b) => b,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "failed to encode variant sidecar");
            return;
        }
    };
    if let Err(e) = tokio::fs::write(&path, body).await {
        warn!(path = %path.display(), error = %e, "failed to write variant sidecar");
    }
}

/// Remove the sidecar of `file`, if any.
pub async fn remove_sidecar(file: &Path) {
    let path = sidecar_path(file);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warn!(path = %path.display(), error = %e, "failed to remove variant sidecar"),
    }
}

fn is_gguf(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|e| e == "gguf")
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn mtime_secs(path: &Path) -> i64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

/// What a sidecar-less file in `{model_id}/` is, read from the catalog.
/// Covers files installed before sidecars existed and an install that
/// stopped between moving the file in and writing its sidecar.
fn inferred(model_id: &str, file: &Path) -> Option<Sidecar> {
    let model = find_model(model_id)?;
    let name = file_name(file);
    let quant = model.quants.iter().find(|q| q.filename == name)?;
    Some(Sidecar {
        model: model.id.clone(),
        quant: quant.quant.clone(),
        sha256: None,
        installed_at: mtime_secs(file),
        snapshot: Snapshot::of(model),
    })
}

/// Every installed Variant: catalog files in their model directories,
/// then Custom models at the top level. A file in a model directory that
/// neither has a sidecar nor matches the catalog is not a Variant.
pub fn scan(model_dir: &Path) -> Vec<InstalledVariant> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(model_dir) else {
        return out;
    };
    let mut dirs = Vec::new();
    let mut top = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        } else if is_gguf(&path) {
            top.push(path);
        }
    }
    dirs.sort();
    top.sort();

    for dir in dirs {
        let dir_name = file_name(&dir);
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = files.flatten().map(|e| e.path()).collect();
        files.sort();
        for file in files.into_iter().filter(|f| is_gguf(f)) {
            let Some(sidecar) = read_sidecar(&file).or_else(|| inferred(&dir_name, &file)) else {
                continue;
            };
            out.push(InstalledVariant {
                key: VariantKey::new(sidecar.model, Some(sidecar.quant)),
                path: file,
                sha256: sidecar.sha256.map(|s| s.to_ascii_lowercase()),
                installed_at: sidecar.installed_at,
                snapshot: Some(sidecar.snapshot),
            });
        }
    }

    for file in top {
        let name = file_name(&file);
        // A catalog filename at the top level is a pre-ADR-0007 install
        // awaiting its move into a model directory, not a Custom model.
        if find_by_filename(&name).is_some() {
            continue;
        }
        // A `:` would make its id read as `model:quant`.
        if name.contains(':') {
            warn!(file = %name, "skipping a Custom model whose name contains ':'");
            continue;
        }
        out.push(InstalledVariant {
            key: VariantKey::new(name.trim_end_matches(".gguf"), None),
            installed_at: mtime_secs(&file),
            path: file,
            sha256: None,
            snapshot: None,
        });
    }
    out
}

/// The installed Variants of one model.
pub fn of_model<'a>(
    installed: &'a [InstalledVariant],
    model_id: &str,
) -> Vec<&'a InstalledVariant> {
    installed
        .iter()
        .filter(|v| v.key.model == model_id)
        .collect()
}

/// Startup-once move to the ADR 0007 layout: each top-level catalog file
/// goes into its model's directory with a sidecar (its legacy `.sha256`
/// fingerprint carried over, its mtime as the install time), and each
/// sidecar-less file already in a model directory gets one. Runs before
/// engine registration. Best-effort per file: a failure leaves that file
/// where it was, still recognised.
pub async fn migrate_layout(model_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(model_dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            let dir_name = file_name(&path);
            let Ok(files) = std::fs::read_dir(&path) else {
                continue;
            };
            for file in files.flatten().map(|e| e.path()).filter(|f| is_gguf(f)) {
                if sidecar_path(&file).is_file() {
                    continue;
                }
                if let Some(sidecar) = inferred(&dir_name, &file) {
                    write_sidecar(&file, &sidecar).await;
                }
            }
            continue;
        }
        if !is_gguf(&path) {
            continue;
        }
        let name = file_name(&path);
        let Some((model, quant)) = find_by_filename(&name) else {
            continue; // a Custom model
        };
        let dest = variant_path(model_dir, &model.id, &name);
        let legacy = legacy_sidecar_path(&path);
        let sha256 = std::fs::read_to_string(&legacy)
            .ok()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());
        let sidecar = Sidecar {
            model: model.id.clone(),
            quant: quant.quant.clone(),
            sha256,
            installed_at: mtime_secs(&path),
            snapshot: Snapshot::of(model),
        };
        if let Err(e) = tokio::fs::create_dir_all(dest.parent().unwrap_or(model_dir)).await {
            warn!(path = %dest.display(), error = %e, "failed to create model directory");
            continue;
        }
        if let Err(e) = tokio::fs::rename(&path, &dest).await {
            warn!(from = %path.display(), to = %dest.display(), error = %e,
                "failed to move model file into its directory");
            continue;
        }
        write_sidecar(&dest, &sidecar).await;
        let _ = tokio::fs::remove_file(&legacy).await;
        info!(model_id = %model.id, quant = %quant.quant, path = %dest.display(),
            "moved model file into its directory");
    }
}

/// Hash every installed catalog file whose sidecar has no sha256 yet —
/// files that predate fingerprints. Runs once at startup, in the
/// background; each file is hashed once.
pub async fn backfill(model_dir: &Path) {
    for variant in scan(model_dir) {
        if variant.key.quant.is_none() || variant.sha256.is_some() {
            continue;
        }
        let Some(mut sidecar) = read_sidecar(&variant.path) else {
            continue;
        };
        match compute_sha256(&variant.path).await {
            Ok(sha) => {
                sidecar.sha256 = Some(sha);
                write_sidecar(&variant.path, &sidecar).await;
                info!(variant = %variant.key, "fingerprinted installed model");
            }
            Err(e) => {
                warn!(variant = %variant.key, error = %e, "failed to fingerprint installed model")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::catalog_data::catalog_models;

    fn tiny() -> (&'static CatalogModel, String, String) {
        let m = find_model("whisper-tiny").expect("whisper-tiny in catalog");
        let q = m.default_quant_file();
        (m, q.quant.clone(), q.filename.clone())
    }

    #[tokio::test]
    async fn a_flat_install_moves_into_its_model_directory_keeping_its_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let (model, quant, filename) = tiny();
        std::fs::write(dir.path().join(&filename), b"weights").unwrap();
        std::fs::write(dir.path().join(format!("{filename}.sha256")), "ABC\n").unwrap();

        migrate_layout(dir.path()).await;

        let moved = variant_path(dir.path(), &model.id, &filename);
        assert!(moved.is_file(), "file moved into {}", moved.display());
        assert!(!dir.path().join(&filename).exists());
        let sidecar = read_sidecar(&moved).expect("sidecar written");
        assert_eq!(sidecar.model, model.id);
        assert_eq!(sidecar.quant, quant);
        assert_eq!(sidecar.sha256.as_deref(), Some("abc"));
    }

    #[tokio::test]
    async fn identity_comes_from_the_sidecar_not_the_catalog() {
        // A file whose catalog entry is gone (a rename upstream) keeps the
        // model it was installed as, and is Retired.
        let dir = tempfile::tempdir().unwrap();
        let (model, _, filename) = tiny();
        let file = variant_path(dir.path(), "renamed-away", &filename);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"weights").unwrap();
        let mut sidecar = Sidecar::for_install(model, "Q8_0", Some("abc".into()));
        sidecar.model = "renamed-away".into();
        write_sidecar(&file, &sidecar).await;

        let found = scan(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].key,
            VariantKey::new("renamed-away", Some("Q8_0".into()))
        );
        assert!(found[0].is_retired());
        assert!(
            !found[0].update_available(),
            "a Retired variant has nothing to update to"
        );
    }

    #[tokio::test]
    async fn a_republished_file_is_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let (model, quant, filename) = tiny();
        let file = variant_path(dir.path(), &model.id, &filename);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"weights").unwrap();

        write_sidecar(
            &file,
            &Sidecar::for_install(model, &quant, Some("not-the-pin".into())),
        )
        .await;
        assert!(scan(dir.path())[0].update_available());

        let pinned = model.quant(&quant).unwrap().sha256.to_ascii_uppercase();
        write_sidecar(&file, &Sidecar::for_install(model, &quant, Some(pinned))).await;
        assert!(!scan(dir.path())[0].update_available());

        write_sidecar(&file, &Sidecar::for_install(model, &quant, None)).await;
        assert!(
            !scan(dir.path())[0].update_available(),
            "unknown counts as up to date"
        );
    }

    #[tokio::test]
    async fn a_top_level_gguf_outside_the_catalog_is_a_custom_model() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("my-finetune.gguf"), b"weights").unwrap();

        let found = scan(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, VariantKey::new("my-finetune", None));
        assert!(!found[0].is_retired());
    }

    #[tokio::test]
    async fn a_custom_file_named_like_a_variant_key_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("whisper-small:Q8_0.gguf"), b"weights").unwrap();

        assert!(scan(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn two_quants_of_one_model_are_two_variants() {
        let dir = tempfile::tempdir().unwrap();
        let model = catalog_models()
            .iter()
            .find(|m| m.quants.len() >= 2)
            .unwrap();
        for q in &model.quants[..2] {
            let file = variant_path(dir.path(), &model.id, &q.filename);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, b"weights").unwrap();
        }
        let found = scan(dir.path());
        let quants: Vec<_> = found.iter().filter_map(|v| v.key.quant.clone()).collect();
        assert!(quants.contains(&model.quants[0].quant));
        assert!(quants.contains(&model.quants[1].quant));
    }
}

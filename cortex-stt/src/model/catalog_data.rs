//! Vendored model catalog — a converted snapshot of Handy's
//! `catalog.json` (handy-computer GGUF releases on Hugging Face).
//!
//! Refresh with `scripts/sync-catalog.py`; never edit `catalog.json` by
//! hand. See ADR 0003 for why the catalog is vendored rather than
//! fetched at runtime.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
struct CatalogRoot {
    models: Vec<CatalogModel>,
}

/// One downloadable model (a **Catalog model** in CONTEXT.md terms).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogModel {
    /// Upstream slug; the model's stable identity everywhere.
    pub id: String,
    pub name: String,
    pub description: String,
    pub family: String,
    #[serde(default)]
    pub parameters: Option<String>,
    #[serde(default)]
    pub base_model: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    pub languages: Vec<String>,
    pub capabilities: CatalogCapabilities,
    pub quants: Vec<QuantFile>,
    pub default_quant: String,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub recommended_rank: Option<u32>,
    #[serde(default)]
    pub speed_score: Option<u32>,
    #[serde(default)]
    pub accuracy_score: Option<u32>,
}

/// Model capabilities as advertised by the catalog (display/UX only —
/// the runtime source of truth is the loaded engine's capabilities).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogCapabilities {
    pub streaming: bool,
    pub translate: bool,
    pub lang_detect: bool,
    pub timestamps: String,
}

/// One precision variant of a catalog model (a **Quant**).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantFile {
    pub quant: String,
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

impl CatalogModel {
    pub fn quant(&self, name: &str) -> Option<&QuantFile> {
        self.quants.iter().find(|q| q.quant == name)
    }

    pub fn default_quant_file(&self) -> &QuantFile {
        self.quant(&self.default_quant).unwrap_or(&self.quants[0])
    }
}

static CATALOG: LazyLock<CatalogRoot> = LazyLock::new(|| {
    serde_json::from_str(include_str!("catalog.json")).expect("vendored catalog.json is valid")
});

/// All catalog models, in upstream order.
pub fn catalog_models() -> &'static [CatalogModel] {
    &CATALOG.models
}

/// Look up a catalog model by its id (slug).
pub fn find_model(id: &str) -> Option<&'static CatalogModel> {
    CATALOG.models.iter().find(|m| m.id == id)
}

/// Map an on-disk filename back to its catalog model and quant.
pub fn find_by_filename(filename: &str) -> Option<(&'static CatalogModel, &'static QuantFile)> {
    CATALOG.models.iter().find_map(|m| {
        m.quants
            .iter()
            .find(|q| q.filename == filename)
            .map(|q| (m, q))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_and_is_consistent() {
        let models = catalog_models();
        assert!(!models.is_empty());
        for m in models {
            assert!(!m.quants.is_empty(), "{} has no quants", m.id);
            assert!(
                m.quant(&m.default_quant).is_some(),
                "{} default_quant {} not in quants",
                m.id,
                m.default_quant
            );
            for q in &m.quants {
                assert_eq!(q.sha256.len(), 64, "{} {} bad sha256", m.id, q.quant);
                assert!(q.url.starts_with("https://huggingface.co/"));
            }
        }
    }

    #[test]
    fn filenames_are_globally_unique() {
        let mut seen = std::collections::HashSet::new();
        for m in catalog_models() {
            for q in &m.quants {
                assert!(
                    seen.insert(&q.filename),
                    "duplicate filename {}",
                    q.filename
                );
            }
        }
    }

    #[test]
    fn known_models_present() {
        for id in ["Breeze-ASR-25", "SenseVoiceSmall", "whisper-large-v3-turbo"] {
            assert!(find_model(id).is_some(), "missing {id}");
        }
    }
}

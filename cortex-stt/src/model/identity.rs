//! Catalog variant identities and persistent aliases for pre-variant IDs.

use std::collections::HashMap;
use std::path::Path;

use super::catalog_data::{CatalogModel, QuantFile, catalog_models, find_model};
use crate::db::database::Database;
use crate::error::AsrError;

pub fn variant_id(model_id: &str, quant: &str) -> String {
    format!("{model_id}@{quant}")
}

pub fn find_variant(id: &str) -> Option<(&'static CatalogModel, &'static QuantFile)> {
    let (base, quant) = id.split_once('@')?;
    let model = find_model(base)?;
    Some((model, model.quant(quant)?))
}

/// Pin each legacy catalog ID to its installed quant, or its catalog
/// default on first migration. Existing pins survive file deletion and
/// catalog changes; custom IDs never enter the alias map.
pub async fn migrate(db: &Database, model_dir: &Path) -> Result<HashMap<String, String>, AsrError> {
    let mut candidates = HashMap::new();
    for model in catalog_models() {
        let mut installed = None;
        for quant in &model.quants {
            if tokio::fs::metadata(model_dir.join(&quant.filename))
                .await
                .is_ok_and(|metadata| metadata.is_file())
            {
                installed = Some(quant);
                break;
            }
        }
        let quant = installed.unwrap_or_else(|| model.default_quant_file());
        candidates.insert(model.id.clone(), variant_id(&model.id, &quant.quant));
    }
    db.migrate_model_aliases(candidates).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_are_catalog_validated() {
        let model = &catalog_models()[0];
        let id = variant_id(&model.id, &model.default_quant);
        assert!(find_variant(&id).is_some());
        assert!(find_variant(&model.id).is_none());
        assert!(find_variant(&variant_id(&model.id, "not-a-quant")).is_none());
        assert!(find_variant("custom@f16").is_none());
    }

    #[tokio::test]
    async fn aliases_survive_deletion_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite");
        let model = catalog_models()
            .iter()
            .find(|m| m.quants.len() > 1)
            .unwrap();
        let quant = model
            .quants
            .iter()
            .find(|q| q.quant != model.default_quant)
            .unwrap();
        let file = dir.path().join(&quant.filename);
        tokio::fs::write(&file, b"model").await.unwrap();
        let db = Database::open(&path).await.unwrap();
        let pinned = migrate(&db, dir.path()).await.unwrap();
        assert_eq!(pinned[&model.id], variant_id(&model.id, &quant.quant));
        tokio::fs::remove_file(file).await.unwrap();
        drop(db);
        let db = Database::open(&path).await.unwrap();
        assert_eq!(migrate(&db, dir.path()).await.unwrap(), pinned);
    }

    #[tokio::test]
    async fn missing_files_use_default_and_multiple_files_use_catalog_order() {
        let dir = tempfile::tempdir().unwrap();
        let model = catalog_models()
            .iter()
            .find(|m| m.quants.len() > 1)
            .unwrap();
        let db = Database::open_in_memory().await.unwrap();
        let aliases = migrate(&db, dir.path()).await.unwrap();
        assert_eq!(
            aliases[&model.id],
            variant_id(&model.id, &model.default_quant)
        );
        for quant in &model.quants {
            tokio::fs::write(dir.path().join(&quant.filename), b"model")
                .await
                .unwrap();
        }
        let db = Database::open_in_memory().await.unwrap();
        let aliases = migrate(&db, dir.path()).await.unwrap();
        assert_eq!(
            aliases[&model.id],
            variant_id(&model.id, &model.quants[0].quant)
        );
    }
}

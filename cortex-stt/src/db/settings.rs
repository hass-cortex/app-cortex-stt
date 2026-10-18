use rusqlite::params;

use super::database::{Database, map_db_err};
use crate::error::AsrError;
use crate::settings::Settings;

impl Database {
    /// Pin legacy catalog identities once and migrate only model references.
    /// Kept separate from app settings so saving settings cannot erase pins.
    pub(crate) async fn migrate_model_aliases(
        &self,
        candidates: std::collections::HashMap<String, String>,
    ) -> Result<std::collections::HashMap<String, String>, AsrError> {
        let _guard = self.settings_write_lock().lock().await;
        self.connection()
            .call(move |conn| {
                use rusqlite::OptionalExtension;
                let tx = conn.transaction()?;
                let read = |key: &str| -> rusqlite::Result<Option<String>> {
                    tx.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
                        .optional()
                };
                let json_error = |e| rusqlite::Error::ToSqlConversionFailure(Box::new(e));
                let mut aliases: std::collections::HashMap<String, String> = read("model_variant_aliases")?
                    .map(|json| serde_json::from_str(&json))
                    .transpose().map_err(json_error)?.unwrap_or_default();
                for (base, variant) in candidates {
                    aliases.entry(base).or_insert(variant);
                }
                let stored = read("app_settings")?;
                let legacy = read("default_model")?;
                if stored.is_some() || legacy.is_some() {
                    // Edit the JSON rather than round-tripping Settings: retain
                    // every unrelated field, including fields from newer builds.
                    let mut settings: serde_json::Value = match stored {
                        Some(json) => serde_json::from_str(&json).map_err(json_error)?,
                        None => serde_json::to_value(Settings::default()).map_err(json_error)?,
                    };
                    if !settings.is_object() {
                        return Err(rusqlite::Error::InvalidQuery);
                    }
                    if let Some(old) = legacy {
                        // An explicit blob value (especially a canonical ID)
                        // takes precedence over the obsolete standalone key.
                        if settings.get("default_model").is_none_or(|v| v.is_null()) {
                            settings["default_model"] = old.into();
                        }
                    }
                    if let Some(id) = settings.get("default_model").and_then(|v| v.as_str())
                        && let Some(variant) = aliases.get(id)
                    {
                        settings["default_model"] = variant.clone().into();
                    }
                    if let Some(overrides) = settings.get_mut("backend_overrides").and_then(|v| v.as_object_mut()) {
                        for (base, variant) in &aliases {
                            if let Some(value) = overrides.remove(base) {
                                overrides.entry(variant.clone()).or_insert(value);
                            }
                        }
                    }
                    tx.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('app_settings', ?1)",
                        [serde_json::to_string(&settings).map_err(json_error)?])?;
                    tx.execute("DELETE FROM settings WHERE key = 'default_model'", [])?;
                }
                tx.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('model_variant_aliases', ?1)",
                    [serde_json::to_string(&aliases).map_err(json_error)?])?;
                tx.commit()?;
                Ok(aliases)
            })
            .await
            .map_err(map_db_err)
    }

    /// Load settings, returning defaults on a fresh install.
    pub async fn load_settings(&self) -> Result<Settings, AsrError> {
        Ok(self.load_stored_settings().await?.unwrap_or_default())
    }

    /// Load settings as stored, or `None` on a fresh install (lets the
    /// caller distinguish "user configured" from built-in defaults).
    ///
    /// Also folds the legacy separately-stored `default_model` key into
    /// the blob — a one-time self-healing migration so `default_model`
    /// has a single home. The key is deleted only AFTER the blob write
    /// succeeds, so a failure mid-migration retries on the next load
    /// instead of losing the value.
    pub async fn load_stored_settings(&self) -> Result<Option<Settings>, AsrError> {
        let stored = self.read_settings_blob().await?;

        match self.read_legacy_default_model().await? {
            None => Ok(stored),
            Some(model) => {
                let _guard = self.settings_write_lock().lock().await;
                let mut settings = stored.unwrap_or_default();
                settings.default_model = Some(model);
                self.write_settings_blob(&settings).await?;
                self.delete_legacy_default_model().await?;
                Ok(Some(settings))
            }
        }
    }

    /// Save settings from `PUT /api/settings`. The incoming blob's
    /// `default_model` is IGNORED — the stored value is preserved, so a
    /// stale settings-form snapshot can never clobber a default set via
    /// `PUT /api/engine/default` (the single writer of that field).
    pub async fn save_settings(&self, settings: &Settings) -> Result<(), AsrError> {
        let _guard = self.settings_write_lock().lock().await;
        let mut merged = settings.clone();
        merged.default_model = self
            .read_settings_blob()
            .await?
            .and_then(|stored| stored.default_model);
        self.write_settings_blob(&merged).await
    }

    /// Persist the default model ID — the only writer of the field.
    /// Read-modify-write under the settings write lock so it can't race
    /// a concurrent whole-blob save.
    pub async fn set_default_model(&self, model_id: &str) -> Result<(), AsrError> {
        let _guard = self.settings_write_lock().lock().await;
        let mut settings = self.read_settings_blob().await?.unwrap_or_default();
        settings.default_model = Some(model_id.to_string());
        self.write_settings_blob(&settings).await
    }

    async fn read_settings_blob(&self) -> Result<Option<Settings>, AsrError> {
        self.connection()
            .call(|conn| {
                let mut stmt =
                    conn.prepare("SELECT value FROM settings WHERE key = 'app_settings'")?;

                let result: Option<String> = stmt.query_row([], |row| row.get(0)).ok();

                let settings = match result {
                    Some(json) => Some(serde_json::from_str(&json).map_err(|e| {
                        rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("invalid settings JSON: {e}"),
                        )))
                    })?),
                    None => None,
                };

                Ok(settings)
            })
            .await
            .map_err(map_db_err)
    }

    async fn write_settings_blob(&self, settings: &Settings) -> Result<(), AsrError> {
        let json = serde_json::to_string(settings).map_err(|e| AsrError::DatabaseError {
            detail: format!("failed to serialize settings: {e}"),
        })?;

        self.connection()
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO settings (key, value) VALUES ('app_settings', ?1)",
                    params![json],
                )?;
                Ok(())
            })
            .await
            .map_err(map_db_err)
    }

    /// Read the pre-0.4 standalone `default_model` key, if present.
    async fn read_legacy_default_model(&self) -> Result<Option<String>, AsrError> {
        self.connection()
            .call(|conn| {
                let mut stmt =
                    conn.prepare("SELECT value FROM settings WHERE key = 'default_model'")?;
                let result: Option<String> = stmt.query_row([], |row| row.get(0)).ok();
                Ok(result)
            })
            .await
            .map_err(map_db_err)
    }

    async fn delete_legacy_default_model(&self) -> Result<(), AsrError> {
        self.connection()
            .call(|conn| {
                conn.execute("DELETE FROM settings WHERE key = 'default_model'", [])?;
                Ok(())
            })
            .await
            .map_err(map_db_err)
    }
}

#[cfg(test)]
mod variant_tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn model_reference_migration_preserves_explicit_values_and_other_fields() {
        let db = Database::open_in_memory().await.unwrap();
        let original = serde_json::json!({
            "default_model": "base",
            "backend_overrides": {"base": "old", "base@q": "explicit", "custom": "custom-value"},
            "future_setting": {"nested": 123}
        });
        db.connection()
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO settings VALUES ('app_settings', ?1)",
                    [original.to_string()],
                )?;
                Ok::<_, rusqlite::Error>(())
            })
            .await
            .unwrap();
        let aliases = HashMap::from([("base".into(), "base@q".into())]);
        db.migrate_model_aliases(aliases.clone()).await.unwrap();
        db.migrate_model_aliases(HashMap::from([("base".into(), "base@other".into())]))
            .await
            .unwrap();
        let stored: String = db
            .connection()
            .call(|conn| {
                conn.query_row(
                    "SELECT value FROM settings WHERE key = 'app_settings'",
                    [],
                    |r| r.get(0),
                )
            })
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&stored).unwrap(),
            serde_json::json!({
                "default_model": "base@q",
                "backend_overrides": {"base@q": "explicit", "custom": "custom-value"},
                "future_setting": {"nested": 123}
            })
        );
        assert_eq!(
            db.migrate_model_aliases(HashMap::new()).await.unwrap(),
            aliases
        );
    }

    #[tokio::test]
    async fn standalone_default_migrates_but_does_not_replace_explicit_default() {
        for explicit in [None, Some("base@explicit"), Some("custom")] {
            let db = Database::open_in_memory().await.unwrap();
            let value = serde_json::json!({"default_model": explicit}).to_string();
            db.connection()
                .call(move |conn| {
                    conn.execute("INSERT INTO settings VALUES ('app_settings', ?1)", [value])?;
                    conn.execute("INSERT INTO settings VALUES ('default_model', 'base')", [])?;
                    Ok::<_, rusqlite::Error>(())
                })
                .await
                .unwrap();
            db.migrate_model_aliases(HashMap::from([("base".into(), "base@q".into())]))
                .await
                .unwrap();
            let stored: String = db
                .connection()
                .call(|conn| {
                    conn.query_row(
                        "SELECT value FROM settings WHERE key = 'app_settings'",
                        [],
                        |r| r.get(0),
                    )
                })
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&stored).unwrap()["default_model"],
                explicit.unwrap_or("base@q")
            );
        }
    }
}

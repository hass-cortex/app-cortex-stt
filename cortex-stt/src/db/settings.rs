use rusqlite::params;

use super::database::{Database, map_db_err};
use crate::error::AsrError;
use crate::settings::Settings;

impl Database {
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
    /// `default_model` and `preferred_quants` are IGNORED — the stored
    /// values are preserved, so a stale settings-form snapshot can never
    /// clobber a choice made through its own endpoint (the single writer
    /// of each field).
    pub async fn save_settings(&self, settings: &Settings) -> Result<(), AsrError> {
        let _guard = self.settings_write_lock().lock().await;
        let mut merged = settings.clone();
        let stored = self.read_settings_blob().await?;
        merged.default_model = stored.as_ref().and_then(|s| s.default_model.clone());
        merged.preferred_quants = stored.map(|s| s.preferred_quants).unwrap_or_default();
        self.write_settings_blob(&merged).await
    }

    /// Persist a model's Preferred variant quant — the only writer of
    /// `preferred_quants`.
    pub async fn set_preferred_quant(&self, model_id: &str, quant: &str) -> Result<(), AsrError> {
        let _guard = self.settings_write_lock().lock().await;
        let mut settings = self.read_settings_blob().await?.unwrap_or_default();
        settings
            .preferred_quants
            .insert(model_id.to_string(), quant.to_string());
        self.write_settings_blob(&settings).await
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

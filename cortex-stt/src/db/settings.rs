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
    pub async fn load_stored_settings(&self) -> Result<Option<Settings>, AsrError> {
        self.read_settings_blob().await
    }

    /// Save settings (`PUT /api/settings`, and the startup migration).
    pub async fn save_settings(&self, settings: &Settings) -> Result<(), AsrError> {
        let _guard = self.settings_write_lock().lock().await;
        self.write_settings_blob(settings).await
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
}

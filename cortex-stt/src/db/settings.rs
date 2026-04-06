use rusqlite::params;

use super::database::{Database, map_db_err};
use crate::api::settings::Settings;
use crate::error::AsrError;

impl Database {
    /// Load settings from the database, returning defaults if not stored.
    /// Merges the separately-stored `default_model` key into the result.
    pub async fn load_settings(&self) -> Result<Settings, AsrError> {
        let mut settings: Settings = self
            .connection()
            .call(|conn| {
                let mut stmt =
                    conn.prepare("SELECT value FROM settings WHERE key = 'app_settings'")?;

                let result: Option<String> = stmt.query_row([], |row| row.get(0)).ok();

                let settings = match result {
                    Some(json) => serde_json::from_str(&json).map_err(|e| {
                        rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("invalid settings JSON: {e}"),
                        )))
                    })?,
                    None => Settings::default(),
                };

                Ok(settings)
            })
            .await
            .map_err(map_db_err)?;

        // Merge the separately-stored default_model key
        if let Ok(Some(model)) = self.get_default_model().await {
            settings.default_model = model;
        }

        Ok(settings)
    }

    /// Save settings to the database.
    pub async fn save_settings(&self, settings: &Settings) -> Result<(), AsrError> {
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

    /// Get the persisted default model ID, if any.
    pub async fn get_default_model(&self) -> Result<Option<String>, AsrError> {
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

    /// Persist the default model ID.
    pub async fn set_default_model(&self, model_id: &str) -> Result<(), AsrError> {
        let model_id_owned = model_id.to_string();

        self.connection()
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO settings (key, value) VALUES ('default_model', ?1)",
                    params![model_id_owned],
                )?;
                Ok(())
            })
            .await
            .map_err(map_db_err)
    }
}

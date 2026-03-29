use rusqlite::params;

use super::database::Database;
use crate::api::settings::Settings;
use crate::error::AsrError;

impl Database {
    /// Load settings from the database, returning defaults if not stored.
    pub fn load_settings(&self) -> Result<Settings, AsrError> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key = 'app_settings'")
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;

        let result: Option<String> = stmt.query_row([], |row| row.get(0)).ok();

        match result {
            Some(json) => serde_json::from_str(&json).map_err(|e| AsrError::DatabaseError {
                detail: format!("invalid settings JSON: {e}"),
            }),
            None => Ok(Settings::default()),
        }
    }

    /// Save settings to the database.
    pub fn save_settings(&self, settings: &Settings) -> Result<(), AsrError> {
        let json = serde_json::to_string(settings).map_err(|e| AsrError::DatabaseError {
            detail: format!("failed to serialize settings: {e}"),
        })?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('app_settings', ?1)",
            params![json],
        )
        .map_err(|e| AsrError::DatabaseError {
            detail: e.to_string(),
        })?;
        Ok(())
    }

    /// Get the persisted default model ID, if any.
    pub fn get_default_model(&self) -> Result<Option<String>, AsrError> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key = 'default_model'")
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;

        let result: Option<String> = stmt.query_row([], |row| row.get(0)).ok();
        Ok(result)
    }

    /// Persist the default model ID.
    pub fn set_default_model(&self, model_id: &str) -> Result<(), AsrError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('default_model', ?1)",
            params![model_id],
        )
        .map_err(|e| AsrError::DatabaseError {
            detail: e.to_string(),
        })?;
        Ok(())
    }
}

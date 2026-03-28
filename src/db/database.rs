use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::AsrError;

/// Thread-safe SQLite database wrapper.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open a database at the given file path, creating it if needed.
    pub fn open(path: &Path) -> Result<Self, AsrError> {
        let conn = Connection::open(path).map_err(|e| AsrError::DatabaseError {
            detail: e.to_string(),
        })?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.run_migrations()?;
        Ok(db)
    }

    /// Open an in-memory database (useful for tests).
    pub fn open_in_memory() -> Result<Self, AsrError> {
        let conn = Connection::open_in_memory().map_err(|e| AsrError::DatabaseError {
            detail: e.to_string(),
        })?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.run_migrations()?;
        Ok(db)
    }

    /// Run all schema migrations.
    fn run_migrations(&self) -> Result<(), AsrError> {
        let conn = self.conn.lock().map_err(|e| AsrError::DatabaseError {
            detail: format!("lock poisoned: {e}"),
        })?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS records (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL DEFAULT (datetime('now')),
                source TEXT NOT NULL,
                language TEXT,
                model_id TEXT NOT NULL,
                audio_duration_ms INTEGER NOT NULL,
                inference_ms INTEGER NOT NULL,
                text TEXT NOT NULL,
                segments_json TEXT NOT NULL DEFAULT '[]',
                audio_path TEXT,
                has_error INTEGER NOT NULL DEFAULT 0,
                error_message TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_records_timestamp ON records(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_records_source ON records(source);
            CREATE INDEX IF NOT EXISTS idx_records_model_id ON records(model_id);

            CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                last4 TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                last_used_at TEXT
            );
            ",
        )
        .map_err(|e| AsrError::DatabaseError {
            detail: e.to_string(),
        })?;

        Ok(())
    }

    /// Acquire the inner connection lock.
    pub fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AsrError> {
        self.conn.lock().map_err(|e| AsrError::DatabaseError {
            detail: format!("lock poisoned: {e}"),
        })
    }
}

use std::path::Path;

use tokio_rusqlite::Connection;

use crate::error::AsrError;

/// The concrete error type returned by `conn.call()` when closures use `rusqlite::Error`.
pub(crate) type CallError = tokio_rusqlite::Error<rusqlite::Error>;

/// Map a [`CallError`] to [`AsrError::DatabaseError`].
pub(crate) fn map_db_err(e: CallError) -> AsrError {
    AsrError::DatabaseError {
        detail: e.to_string(),
    }
}

/// Async SQLite database wrapper backed by a dedicated background thread.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open a database at the given file path, creating it if needed.
    pub async fn open(path: &Path) -> Result<Self, AsrError> {
        let conn = Connection::open(path)
            .await
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;
        let db = Self { conn };
        db.run_migrations().await?;
        Ok(db)
    }

    /// Open an in-memory database (useful for tests).
    pub async fn open_in_memory() -> Result<Self, AsrError> {
        let conn = Connection::open_in_memory()
            .await
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;
        let db = Self { conn };
        db.run_migrations().await?;
        Ok(db)
    }

    /// Run all schema migrations.
    async fn run_migrations(&self) -> Result<(), AsrError> {
        self.conn
            .call(|conn| {
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
                error_message TEXT,
                api_key_id TEXT
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

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
                )?;
                Ok(())
            })
            .await
            .map_err(map_db_err)?;

        Ok(())
    }

    /// Access the inner `tokio_rusqlite::Connection` for running queries.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

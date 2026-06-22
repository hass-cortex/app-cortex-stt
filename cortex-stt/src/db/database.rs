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

    /// Add `column` to `table` if it is not already present. `definition`
    /// is the DDL fragment after the column name (e.g.
    /// `"INTEGER NOT NULL DEFAULT 0"`). Idempotent — a no-op once the
    /// column exists, so it is safe to run on every startup.
    ///
    /// All three arguments are compile-time constants from migration code
    /// (never user input), so interpolating them into the DDL is safe.
    async fn add_column_if_missing(
        &self,
        table: &'static str,
        column: &'static str,
        definition: &'static str,
    ) -> Result<(), AsrError> {
        self.conn
            .call(move |conn| {
                let exists: bool = conn
                    .prepare(&format!(
                        "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'"
                    ))?
                    .query_row([], |row| row.get::<_, i64>(0))
                    .map(|c| c > 0)?;
                if !exists {
                    conn.execute_batch(&format!(
                        "ALTER TABLE {table} ADD COLUMN {column} {definition};"
                    ))?;
                }
                Ok(())
            })
            .await
            .map_err(map_db_err)
    }

    /// Run all schema migrations.
    async fn run_migrations(&self) -> Result<(), AsrError> {
        // Step 1: ensure tables exist. Indexes are created later so that
        // ALTER TABLE column-add migrations below can populate columns the
        // indexes reference before the index is created.
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
                model_load_ms INTEGER NOT NULL DEFAULT 0,
                pool_wait_ms INTEGER NOT NULL DEFAULT 0,
                cold_load_ms INTEGER NOT NULL DEFAULT 0,
                text TEXT NOT NULL,
                segments_json TEXT NOT NULL DEFAULT '[]',
                audio_path TEXT,
                has_error INTEGER NOT NULL DEFAULT 0,
                error_message TEXT,
                api_key_id TEXT
            );

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

        // Migration: backfill `source` on records tables predating its
        // introduction. Without this, CREATE INDEX idx_records_source below
        // would fail with "no such column: source" on upgraded installs.
        self.add_column_if_missing("records", "source", "TEXT NOT NULL DEFAULT 'unknown'")
            .await?;

        // Step 2: create indexes once all referenced columns are guaranteed
        // to exist.
        self.conn
            .call(|conn| {
                conn.execute_batch(
                    "
            CREATE INDEX IF NOT EXISTS idx_records_timestamp ON records(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_records_source ON records(source);
            CREATE INDEX IF NOT EXISTS idx_records_model_id ON records(model_id);
            ",
                )?;
                Ok(())
            })
            .await
            .map_err(map_db_err)?;

        // Migration: add raw_key column to api_keys.
        self.add_column_if_missing("api_keys", "raw_key", "TEXT NOT NULL DEFAULT ''")
            .await?;

        // Migration: add device column to records.
        self.add_column_if_missing("records", "device", "TEXT NOT NULL DEFAULT 'cpu'")
            .await?;

        // Migration: add acquire timing breakdown columns to records.
        self.add_column_if_missing("records", "pool_wait_ms", "INTEGER NOT NULL DEFAULT 0")
            .await?;
        self.add_column_if_missing("records", "cold_load_ms", "INTEGER NOT NULL DEFAULT 0")
            .await?;

        // Migration: add system column to api_keys.
        //   system=1 marks keys managed by the addon (e.g. Supervisor discovery
        //   bootstrap key). Clients cannot delete system keys via the admin UI.
        self.add_column_if_missing("api_keys", "system", "INTEGER NOT NULL DEFAULT 0")
            .await?;

        Ok(())
    }

    /// Access the inner `tokio_rusqlite::Connection` for running queries.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

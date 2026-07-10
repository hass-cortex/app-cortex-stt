use std::path::{Path, PathBuf};

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
    /// Backing file path (`None` for in-memory databases). The module
    /// owns its storage location — callers ask [`disk_usage_bytes`]
    /// instead of re-deriving the filename.
    ///
    /// [`disk_usage_bytes`]: Self::disk_usage_bytes
    file_path: Option<PathBuf>,
    /// Serializes read-modify-write cycles on the settings blob (see
    /// `db::settings`) so concurrent writers can't lose updates.
    settings_write_lock: tokio::sync::Mutex<()>,
}

impl Database {
    /// Open a database at the given file path, creating it if needed.
    pub async fn open(path: &Path) -> Result<Self, AsrError> {
        let conn = Connection::open(path)
            .await
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;
        let db = Self {
            conn,
            file_path: Some(path.to_path_buf()),
            settings_write_lock: tokio::sync::Mutex::new(()),
        };
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
        let db = Self {
            conn,
            file_path: None,
            settings_write_lock: tokio::sync::Mutex::new(()),
        };
        db.run_migrations().await?;
        Ok(db)
    }

    /// Size of the backing database file in bytes (0 for in-memory or
    /// when the file cannot be inspected).
    pub async fn disk_usage_bytes(&self) -> u64 {
        match &self.file_path {
            Some(path) => tokio::fs::metadata(path)
                .await
                .map(|m| m.len())
                .unwrap_or(0),
            None => 0,
        }
    }

    /// Add `column` to `table` if it is not already present. `definition`
    /// is the DDL fragment after the column name (e.g.
    /// `"INTEGER NOT NULL DEFAULT 0"`). Idempotent — a no-op once the
    /// column exists, so it is safe to run on every startup.
    ///
    /// All three arguments are compile-time constants from migration code
    /// (never user input), so interpolating them into the DDL is safe.
    pub(crate) async fn add_column_if_missing(
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

    /// Run schema migrations for the tables this module owns (`api_keys`,
    /// `settings`). The `records` table belongs to `history::store`, which
    /// migrates it in `History::new`.
    async fn run_migrations(&self) -> Result<(), AsrError> {
        self.conn
            .call(|conn| {
                conn.execute_batch(
                    "
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

        // Migration: add raw_key column to api_keys.
        self.add_column_if_missing("api_keys", "raw_key", "TEXT NOT NULL DEFAULT ''")
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

    /// The settings-blob write lock (held across load→save cycles).
    pub(crate) fn settings_write_lock(&self) -> &tokio::sync::Mutex<()> {
        &self.settings_write_lock
    }
}

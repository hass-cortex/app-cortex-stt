//! SQLite-backed storage for transcription history records.
//!
//! Private to the `history` module — all callers go through `History`.

use std::str::FromStr;
use std::sync::Arc;

use rusqlite::{params, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::db::database::{Database, map_db_err};
use crate::error::AsrError;
use crate::retention::RetentionCandidate;

/// Where the transcription request originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionSource {
    HttpApi,
    /// WebSocket stream session.
    WsApi,
}

impl TranscriptionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HttpApi => "http_api",
            Self::WsApi => "ws_api",
        }
    }
}

impl FromStr for TranscriptionSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "http_api" => Ok(Self::HttpApi),
            "ws_api" => Ok(Self::WsApi),
            other => Err(format!("unknown transcription source: {other}")),
        }
    }
}

/// Input for creating a transcription record.
///
/// `audio_path` is intentionally absent — `History::create` derives it
/// from the optional samples slice, so the pairing of row and WAV file
/// can never disagree.
pub struct CreateRecord {
    pub source: TranscriptionSource,
    pub language: Option<String>,
    pub model_id: String,
    pub audio_duration_ms: i64,
    pub inference_ms: i64,
    pub model_load_ms: i64,
    pub pool_wait_ms: i64,
    pub cold_load_ms: i64,
    pub text: String,
    pub segments_json: String,
    pub has_error: bool,
    pub error_message: Option<String>,
    pub api_key_id: Option<String>,
    pub device: String,
}

/// A stored transcription record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionRecord {
    pub id: String,
    pub timestamp: String,
    pub source: String,
    pub language: Option<String>,
    pub model_id: String,
    pub audio_duration_ms: i64,
    pub inference_ms: i64,
    pub model_load_ms: i64,
    pub pool_wait_ms: i64,
    pub cold_load_ms: i64,
    pub text: String,
    pub segments_json: String,
    pub audio_path: Option<String>,
    pub has_error: bool,
    pub error_message: Option<String>,
    pub api_key_id: Option<String>,
    pub device: String,
}

/// Optional filters for listing records.
#[derive(Debug, Default)]
pub struct ListRecordsFilter {
    pub source: Option<TranscriptionSource>,
    pub model_id: Option<String>,
    pub text: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub has_error: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// The full ordered column list for a `records` row, shared by every
/// `SELECT … FROM records` that hydrates a [`TranscriptionRecord`]. The
/// order MUST match the positional `row.get(N)` indices in
/// [`row_to_record`]; keeping the list in one place stops the SELECT
/// sites and the mapper from drifting apart when a column is added.
const RECORD_COLUMNS: &str = "id, timestamp, source, language, model_id, audio_duration_ms, inference_ms, model_load_ms, pool_wait_ms, cold_load_ms, text, segments_json, audio_path, has_error, error_message, api_key_id, device";

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

/// Insert a row with a caller-generated id. The id is returned so the
/// surrounding `History::create` can pair it with the WAV filename
/// without a second roundtrip.
pub(super) async fn insert(
    db: &Arc<Database>,
    id: &str,
    rec: &CreateRecord,
    audio_path: Option<&str>,
) -> Result<(), AsrError> {
    let id = id.to_string();
    let source = rec.source.as_str().to_string();
    let language = rec.language.clone();
    let model_id = rec.model_id.clone();
    let audio_duration_ms = rec.audio_duration_ms;
    let inference_ms = rec.inference_ms;
    let model_load_ms = rec.model_load_ms;
    let pool_wait_ms = rec.pool_wait_ms;
    let cold_load_ms = rec.cold_load_ms;
    let text = rec.text.clone();
    let segments_json = rec.segments_json.clone();
    let audio_path = audio_path.map(|s| s.to_string());
    let has_error = rec.has_error as i32;
    let error_message = rec.error_message.clone();
    let api_key_id = rec.api_key_id.clone();
    let device = rec.device.clone();

    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO records (id, source, language, model_id, audio_duration_ms, inference_ms, model_load_ms, pool_wait_ms, cold_load_ms, text, segments_json, audio_path, has_error, error_message, api_key_id, device)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    id,
                    source,
                    language,
                    model_id,
                    audio_duration_ms,
                    inference_ms,
                    model_load_ms,
                    pool_wait_ms,
                    cold_load_ms,
                    text,
                    segments_json,
                    audio_path,
                    has_error,
                    error_message,
                    api_key_id,
                    device,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn get(
    db: &Arc<Database>,
    id: &str,
) -> Result<Option<TranscriptionRecord>, AsrError> {
    let id_owned = id.to_string();
    db.connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {RECORD_COLUMNS} FROM records WHERE id = ?1"
            ))?;
            let mut rows = stmt.query_map(params![id_owned], row_to_record)?;
            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(map_db_err)
}

/// Look up the `audio_path` for a single row. `Ok(None)` means the row
/// doesn't exist; `Ok(Some(None))` means the row exists but has no
/// audio file; `Ok(Some(Some(path)))` means the row exists and has one.
pub(super) async fn lookup_audio_path(
    db: &Arc<Database>,
    id: &str,
) -> Result<Option<Option<String>>, AsrError> {
    let id_owned = id.to_string();
    db.connection()
        .call(move |conn| {
            conn.query_row(
                "SELECT audio_path FROM records WHERE id = ?1",
                params![&id_owned],
                |row| row.get::<_, Option<String>>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .await
        .map_err(map_db_err)
}

/// Delete a single row by id (no audio handling — caller is expected to
/// remove the file first).
pub(super) async fn delete_row(db: &Arc<Database>, id: &str) -> Result<bool, AsrError> {
    let id_owned = id.to_string();
    db.connection()
        .call(move |conn| {
            let deleted = conn.execute("DELETE FROM records WHERE id = ?1", params![id_owned])?;
            Ok(deleted > 0)
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn list(
    db: &Arc<Database>,
    filter: &ListRecordsFilter,
) -> Result<Vec<TranscriptionRecord>, AsrError> {
    let source = filter.source;
    let model_id = filter.model_id.clone();
    let text = filter.text.clone();
    let from = filter.from.clone();
    let to = filter.to.clone();
    let has_error = filter.has_error;
    let limit = filter.limit;
    let offset = filter.offset;

    db.connection()
        .call(move |conn| {
            let mut sql = format!("SELECT {RECORD_COLUMNS} FROM records WHERE 1=1");
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut idx = 1;

            if let Some(source) = &source {
                sql.push_str(&format!(" AND source = ?{idx}"));
                param_values.push(Box::new(source.as_str().to_owned()));
                idx += 1;
            }
            if let Some(model_id) = &model_id {
                sql.push_str(&format!(" AND model_id LIKE ?{idx}"));
                param_values.push(Box::new(format!("%{model_id}%")));
                idx += 1;
            }
            if let Some(text) = &text {
                sql.push_str(&format!(" AND text LIKE ?{idx}"));
                param_values.push(Box::new(format!("%{text}%")));
                idx += 1;
            }
            if let Some(from) = &from {
                sql.push_str(&format!(" AND timestamp >= ?{idx}"));
                param_values.push(Box::new(from.clone()));
                idx += 1;
            }
            if let Some(to) = &to {
                sql.push_str(&format!(" AND timestamp <= ?{idx}"));
                param_values.push(Box::new(to.clone()));
                idx += 1;
            }
            if let Some(has_error) = has_error {
                sql.push_str(&format!(" AND has_error = ?{idx}"));
                param_values.push(Box::new(has_error as i32));
                idx += 1;
            }

            sql.push_str(" ORDER BY timestamp DESC");

            if let Some(limit) = limit {
                sql.push_str(&format!(" LIMIT ?{idx}"));
                param_values.push(Box::new(limit));
                idx += 1;
            }
            if let Some(offset) = offset {
                sql.push_str(&format!(" OFFSET ?{idx}"));
                param_values.push(Box::new(offset));
            }

            let mut stmt = conn.prepare(&sql)?;
            let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|b| b.as_ref()).collect();
            stmt.query_map(params_ref.as_slice(), row_to_record)?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .await
        .map_err(map_db_err)
}

/// Look up `(id, audio_path)` pairs for the given ids. Rows that don't
/// exist are omitted from the result.
pub(super) async fn lookup_audio_paths(
    db: &Arc<Database>,
    ids: &[String],
) -> Result<Vec<(String, Option<String>)>, AsrError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids = ids.to_vec();
    db.connection()
        .call(move |conn| {
            let placeholders = repeat_placeholders(ids.len());
            let sql = format!("SELECT id, audio_path FROM records WHERE id IN ({placeholders})");
            let mut stmt = conn.prepare(&sql)?;
            stmt.query_map(params_from_iter(ids.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
        })
        .await
        .map_err(map_db_err)
}

/// Look up `(id, audio_path)` for every row in the table.
pub(super) async fn all_audio_paths(
    db: &Arc<Database>,
) -> Result<Vec<(String, Option<String>)>, AsrError> {
    db.connection()
        .call(|conn| {
            let mut stmt = conn.prepare("SELECT id, audio_path FROM records")?;
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
        })
        .await
        .map_err(map_db_err)
}

/// Batch DELETE rows by id. No audio handling — caller is expected to
/// have removed the files first.
pub(super) async fn delete_rows(db: &Arc<Database>, ids: &[String]) -> Result<usize, AsrError> {
    if ids.is_empty() {
        return Ok(0);
    }
    let ids = ids.to_vec();
    db.connection()
        .call(move |conn| {
            let placeholders = repeat_placeholders(ids.len());
            let sql = format!("DELETE FROM records WHERE id IN ({placeholders})");
            let deleted = conn.execute(&sql, params_from_iter(ids.iter()))?;
            Ok(deleted)
        })
        .await
        .map_err(map_db_err)
}

/// Batch NULL the `audio_path` column for the given ids. Caller is
/// expected to have removed the files first.
pub(super) async fn null_audio_paths(
    db: &Arc<Database>,
    ids: &[String],
) -> Result<usize, AsrError> {
    if ids.is_empty() {
        return Ok(0);
    }
    let ids = ids.to_vec();
    db.connection()
        .call(move |conn| {
            let placeholders = repeat_placeholders(ids.len());
            let sql = format!(
                "UPDATE records SET audio_path = NULL WHERE id IN ({placeholders}) AND audio_path IS NOT NULL"
            );
            let updated = conn.execute(&sql, params_from_iter(ids.iter()))?;
            Ok(updated)
        })
        .await
        .map_err(map_db_err)
}

// ---------------------------------------------------------------------------
// Retention candidate enumeration
// ---------------------------------------------------------------------------

/// All records as retention candidates without size info. Used by
/// `record_retention` (which only needs id + created_at).
pub(super) async fn list_record_candidates(
    db: &Arc<Database>,
) -> Result<Vec<RetentionCandidate>, AsrError> {
    db.connection()
        .call(|conn| {
            let mut stmt =
                conn.prepare("SELECT id, timestamp FROM records ORDER BY timestamp ASC")?;
            stmt.query_map([], |row| {
                Ok(RetentionCandidate {
                    id: row.get::<_, String>(0)?,
                    created_at: row.get::<_, String>(1)?,
                    size_bytes: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
        })
        .await
        .map_err(map_db_err)
}

/// Records with an audio file, returned as `(id, timestamp, filename)`.
/// The caller (History::list_audio_candidates) stat()s each file to fill
/// in `size_bytes` — that I/O lives in the `audio` submodule.
pub(super) async fn list_audio_rows(
    db: &Arc<Database>,
) -> Result<Vec<(String, String, String)>, AsrError> {
    db.connection()
        .call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, timestamp, audio_path FROM records
                 WHERE audio_path IS NOT NULL
                 ORDER BY timestamp ASC",
            )?;
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
        })
        .await
        .map_err(map_db_err)
}

// ---------------------------------------------------------------------------
// Analytics aggregates (consumed by api/metrics.rs via History)
// ---------------------------------------------------------------------------

pub(super) async fn count_records(
    db: &Arc<Database>,
    source: Option<TranscriptionSource>,
) -> Result<usize, AsrError> {
    db.connection()
        .call(move |conn| {
            let count: i64 = if let Some(src) = source {
                conn.query_row(
                    "SELECT COUNT(*) FROM records WHERE source = ?1 AND has_error = 0",
                    params![src.as_str()],
                    |row| row.get(0),
                )
            } else {
                conn.query_row(
                    "SELECT COUNT(*) FROM records WHERE has_error = 0",
                    [],
                    |row| row.get(0),
                )
            }?;
            Ok(count as usize)
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn count_records_today(
    db: &Arc<Database>,
    source: Option<TranscriptionSource>,
) -> Result<usize, AsrError> {
    db.connection()
        .call(move |conn| {
            let count: i64 = if let Some(src) = source {
                conn.query_row(
                    "SELECT COUNT(*) FROM records WHERE source = ?1 AND has_error = 0 AND timestamp >= datetime('now', 'start of day')",
                    params![src.as_str()],
                    |row| row.get(0),
                )
            } else {
                conn.query_row(
                    "SELECT COUNT(*) FROM records WHERE has_error = 0 AND timestamp >= datetime('now', 'start of day')",
                    [],
                    |row| row.get(0),
                )
            }?;
            Ok(count as usize)
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn total_audio_duration_ms(db: &Arc<Database>) -> Result<i64, AsrError> {
    db.connection()
        .call(|conn| {
            conn.query_row(
                "SELECT COALESCE(SUM(audio_duration_ms), 0) FROM records WHERE has_error = 0",
                [],
                |row| row.get(0),
            )
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn today_audio_duration_ms(db: &Arc<Database>) -> Result<i64, AsrError> {
    db.connection()
        .call(|conn| {
            conn.query_row(
                "SELECT COALESCE(SUM(audio_duration_ms), 0) FROM records WHERE has_error = 0 AND timestamp >= datetime('now', 'start of day')",
                [],
                |row| row.get(0),
            )
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn avg_inference_ms(db: &Arc<Database>) -> Result<f64, AsrError> {
    db.connection()
        .call(|conn| {
            conn.query_row(
                "SELECT COALESCE(AVG(inference_ms), 0.0) FROM records WHERE has_error = 0",
                [],
                |row| row.get(0),
            )
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn count_errors(db: &Arc<Database>, today_only: bool) -> Result<usize, AsrError> {
    db.connection()
        .call(move |conn| {
            let count: i64 = if today_only {
                conn.query_row(
                    "SELECT COUNT(*) FROM records WHERE has_error = 1 AND timestamp >= datetime('now', 'start of day')",
                    [],
                    |row| row.get(0),
                )
            } else {
                conn.query_row(
                    "SELECT COUNT(*) FROM records WHERE has_error = 1",
                    [],
                    |row| row.get(0),
                )
            }?;
            Ok(count as usize)
        })
        .await
        .map_err(map_db_err)
}

// ---------------------------------------------------------------------------
// Row mapper
// ---------------------------------------------------------------------------

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TranscriptionRecord> {
    Ok(TranscriptionRecord {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        source: row.get(2)?,
        language: row.get(3)?,
        model_id: row.get(4)?,
        audio_duration_ms: row.get(5)?,
        inference_ms: row.get(6)?,
        model_load_ms: row.get(7)?,
        pool_wait_ms: row.get(8)?,
        cold_load_ms: row.get(9)?,
        text: row.get(10)?,
        segments_json: row.get(11)?,
        audio_path: row.get(12)?,
        has_error: row.get::<_, i32>(13)? != 0,
        error_message: row.get(14)?,
        api_key_id: row.get(15)?,
        device: row.get(16)?,
    })
}

fn repeat_placeholders(n: usize) -> String {
    (1..=n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",")
}

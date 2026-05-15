use std::str::FromStr;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::database::{Database, map_db_err};
use crate::error::AsrError;

/// Where the transcription request originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionSource {
    HttpApi,
}

impl TranscriptionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HttpApi => "http_api",
        }
    }
}

impl FromStr for TranscriptionSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "http_api" => Ok(Self::HttpApi),
            other => Err(format!("unknown transcription source: {other}")),
        }
    }
}

/// Input for creating a transcription record.
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
    pub audio_path: Option<String>,
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

impl Database {
    /// Insert a new transcription record. Returns the generated UUID.
    pub async fn insert_record(&self, rec: &CreateRecord) -> Result<String, AsrError> {
        let id = uuid::Uuid::new_v4().to_string();
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
        let audio_path = rec.audio_path.clone();
        let has_error = rec.has_error as i32;
        let error_message = rec.error_message.clone();
        let api_key_id = rec.api_key_id.clone();
        let device = rec.device.clone();
        let id_clone = id.clone();

        self.connection()
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO records (id, source, language, model_id, audio_duration_ms, inference_ms, model_load_ms, pool_wait_ms, cold_load_ms, text, segments_json, audio_path, has_error, error_message, api_key_id, device)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                    params![
                        id_clone,
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
            .map_err(map_db_err)?;

        Ok(id)
    }

    /// Get a single record by id.
    pub async fn get_record(&self, id: &str) -> Result<Option<TranscriptionRecord>, AsrError> {
        let id_owned = id.to_string();

        self.connection()
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, timestamp, source, language, model_id, audio_duration_ms, inference_ms, model_load_ms, pool_wait_ms, cold_load_ms, text, segments_json, audio_path, has_error, error_message, api_key_id, device
                     FROM records WHERE id = ?1",
                )?;

                let mut rows = stmt.query_map(params![id_owned], row_to_record)?;

                match rows.next() {
                    Some(row) => Ok(Some(row?)),
                    None => Ok(None),
                }
            })
            .await
            .map_err(map_db_err)
    }

    /// Delete a record by id. Returns true if a row was deleted.
    pub async fn delete_record(&self, id: &str) -> Result<bool, AsrError> {
        let id_owned = id.to_string();

        self.connection()
            .call(move |conn| {
                let deleted =
                    conn.execute("DELETE FROM records WHERE id = ?1", params![id_owned])?;
                Ok(deleted > 0)
            })
            .await
            .map_err(map_db_err)
    }

    /// List records with optional filters, ordered by timestamp descending.
    pub async fn list_records(
        &self,
        filter: &ListRecordsFilter,
    ) -> Result<Vec<TranscriptionRecord>, AsrError> {
        // Clone all filter values into owned types for the move closure.
        let source = filter.source;
        let model_id = filter.model_id.clone();
        let text = filter.text.clone();
        let from = filter.from.clone();
        let to = filter.to.clone();
        let has_error = filter.has_error;
        let limit = filter.limit;
        let offset = filter.offset;

        self.connection()
            .call(move |conn| {
                let mut sql = String::from(
                    "SELECT id, timestamp, source, language, model_id, audio_duration_ms, inference_ms, model_load_ms, pool_wait_ms, cold_load_ms, text, segments_json, audio_path, has_error, error_message, api_key_id, device
                     FROM records WHERE 1=1",
                );
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

                let rows = stmt.query_map(params_ref.as_slice(), row_to_record)?;

                let mut records = Vec::new();
                for row in rows {
                    records.push(row?);
                }
                Ok(records)
            })
            .await
            .map_err(map_db_err)
    }

    /// Delete all records. Returns the count deleted.
    pub async fn delete_all_records(&self) -> Result<usize, AsrError> {
        self.connection()
            .call(move |conn| {
                let deleted = conn.execute("DELETE FROM records", [])?;
                Ok(deleted)
            })
            .await
            .map_err(map_db_err)
    }

    /// Get all audio file paths.
    pub async fn get_all_audio_paths(&self) -> Result<Vec<String>, AsrError> {
        self.connection()
            .call(move |conn| {
                let mut stmt =
                    conn.prepare("SELECT audio_path FROM records WHERE audio_path IS NOT NULL")?;
                let paths: Vec<String> = stmt
                    .query_map([], |row| row.get(0))?
                    .filter_map(|r| r.ok())
                    .collect();
                Ok(paths)
            })
            .await
            .map_err(map_db_err)
    }

    /// Delete records older than the given number of days. Returns the count deleted.
    pub async fn cleanup_records_older_than_days(&self, days: i64) -> Result<usize, AsrError> {
        self.connection()
            .call(move |conn| {
                let deleted = conn.execute(
                    "DELETE FROM records WHERE timestamp < datetime('now', ?1)",
                    params![format!("-{days} days")],
                )?;
                Ok(deleted)
            })
            .await
            .map_err(map_db_err)
    }

    /// Get audio file paths for records older than the given number of days.
    pub async fn get_audio_paths_older_than_days(
        &self,
        days: i64,
    ) -> Result<Vec<String>, AsrError> {
        self.connection()
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT audio_path FROM records
                     WHERE audio_path IS NOT NULL AND timestamp < datetime('now', ?1)",
                )?;

                let rows = stmt.query_map(params![format!("-{days} days")], |row| row.get(0))?;

                let mut paths = Vec::new();
                for row in rows {
                    paths.push(row?);
                }
                Ok(paths)
            })
            .await
            .map_err(map_db_err)
    }

    /// Count records from today, optionally filtered by source.
    pub async fn count_records_today(
        &self,
        source: Option<TranscriptionSource>,
    ) -> Result<usize, AsrError> {
        self.connection()
            .call(move |conn| {
                let count: i64 = if let Some(src) = source {
                    conn.query_row(
                        "SELECT COUNT(*) FROM records WHERE source = ?1 AND timestamp >= datetime('now', 'start of day')",
                        params![src.as_str()],
                        |row| row.get(0),
                    )
                } else {
                    conn.query_row(
                        "SELECT COUNT(*) FROM records WHERE timestamp >= datetime('now', 'start of day')",
                        [],
                        |row| row.get(0),
                    )
                }?;
                Ok(count as usize)
            })
            .await
            .map_err(map_db_err)
    }

    /// Sum `audio_duration_ms` for all records.
    pub async fn total_audio_duration_ms(&self) -> Result<i64, AsrError> {
        self.connection()
            .call(|conn| {
                let total: i64 = conn.query_row(
                    "SELECT COALESCE(SUM(audio_duration_ms), 0) FROM records",
                    [],
                    |row| row.get(0),
                )?;
                Ok(total)
            })
            .await
            .map_err(map_db_err)
    }

    /// Sum `audio_duration_ms` for today's records.
    pub async fn today_audio_duration_ms(&self) -> Result<i64, AsrError> {
        self.connection()
            .call(|conn| {
                let total: i64 = conn.query_row(
                    "SELECT COALESCE(SUM(audio_duration_ms), 0) FROM records WHERE timestamp >= datetime('now', 'start of day')",
                    [],
                    |row| row.get(0),
                )?;
                Ok(total)
            })
            .await
            .map_err(map_db_err)
    }

    /// Average `inference_ms` across all records.
    pub async fn avg_inference_ms(&self) -> Result<f64, AsrError> {
        self.connection()
            .call(|conn| {
                let avg: f64 = conn.query_row(
                    "SELECT COALESCE(AVG(inference_ms), 0.0) FROM records",
                    [],
                    |row| row.get(0),
                )?;
                Ok(avg)
            })
            .await
            .map_err(map_db_err)
    }

    /// Count records with `has_error = 1`. If `today_only` is true, restrict to today.
    pub async fn count_errors(&self, today_only: bool) -> Result<usize, AsrError> {
        self.connection()
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

    /// Count total records, optionally filtered by source.
    pub async fn count_records(
        &self,
        source: Option<TranscriptionSource>,
    ) -> Result<usize, AsrError> {
        self.connection()
            .call(move |conn| {
                let count: i64 = if let Some(src) = source {
                    conn.query_row(
                        "SELECT COUNT(*) FROM records WHERE source = ?1",
                        params![src.as_str()],
                        |row| row.get(0),
                    )
                } else {
                    conn.query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
                }?;
                Ok(count as usize)
            })
            .await
            .map_err(map_db_err)
    }

    /// Delete the oldest records to keep at most `max_count` records.
    /// Returns the number of records deleted.
    pub async fn cleanup_records_by_count(&self, max_count: usize) -> Result<usize, AsrError> {
        self.connection()
            .call(move |conn| {
                let total: i64 =
                    conn.query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))?;

                let excess = total - max_count as i64;
                if excess <= 0 {
                    return Ok(0);
                }

                let deleted = conn.execute(
                    "DELETE FROM records WHERE id IN (SELECT id FROM records ORDER BY timestamp ASC LIMIT ?1)",
                    params![excess],
                )?;
                Ok(deleted)
            })
            .await
            .map_err(map_db_err)
    }

    /// Get audio file paths for the oldest records, ordered by timestamp ASC,
    /// keeping at most `max_count` records. Returns paths of records that
    /// would be deleted.
    pub async fn get_audio_paths_exceeding_count(
        &self,
        max_count: usize,
    ) -> Result<Vec<String>, AsrError> {
        self.connection()
            .call(move |conn| {
                let total: i64 =
                    conn.query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))?;

                let excess = total - max_count as i64;
                if excess <= 0 {
                    return Ok(Vec::new());
                }

                let mut stmt = conn.prepare(
                    "SELECT audio_path FROM records
                     WHERE audio_path IS NOT NULL
                     ORDER BY timestamp ASC LIMIT ?1",
                )?;

                let rows = stmt.query_map(params![excess], |row| row.get::<_, String>(0))?;

                let mut paths = Vec::new();
                for row in rows {
                    paths.push(row?);
                }
                Ok(paths)
            })
            .await
            .map_err(map_db_err)
    }

    /// Get audio file paths ordered by timestamp ASC (oldest first).
    /// Used by disk-limit cleanup to iterate and delete until under limit.
    pub async fn get_audio_paths_oldest_first(&self) -> Result<Vec<(String, String)>, AsrError> {
        self.connection()
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, audio_path FROM records
                     WHERE audio_path IS NOT NULL
                     ORDER BY timestamp ASC",
                )?;

                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;

                let mut result = Vec::new();
                for row in rows {
                    result.push(row?);
                }
                Ok(result)
            })
            .await
            .map_err(map_db_err)
    }
}

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

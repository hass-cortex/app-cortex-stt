use std::str::FromStr;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::database::Database;
use crate::error::AsrError;

/// Where the transcription request originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionSource {
    Wyoming,
    HttpApi,
}

impl TranscriptionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wyoming => "wyoming",
            Self::HttpApi => "http_api",
        }
    }
}

impl FromStr for TranscriptionSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "wyoming" => Ok(Self::Wyoming),
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
    pub text: String,
    pub segments_json: String,
    pub audio_path: Option<String>,
    pub has_error: bool,
    pub error_message: Option<String>,
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
    pub text: String,
    pub segments_json: String,
    pub audio_path: Option<String>,
    pub has_error: bool,
    pub error_message: Option<String>,
}

/// Optional filters for listing records.
#[derive(Debug, Default)]
pub struct ListRecordsFilter {
    pub source: Option<TranscriptionSource>,
    pub model_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl Database {
    /// Insert a new transcription record. Returns the generated UUID.
    pub fn insert_record(&self, rec: &CreateRecord) -> Result<String, AsrError> {
        let id = uuid::Uuid::new_v4().to_string();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO records (id, source, language, model_id, audio_duration_ms, inference_ms, text, segments_json, audio_path, has_error, error_message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                rec.source.as_str(),
                rec.language,
                rec.model_id,
                rec.audio_duration_ms,
                rec.inference_ms,
                rec.text,
                rec.segments_json,
                rec.audio_path,
                rec.has_error as i32,
                rec.error_message,
            ],
        )
        .map_err(|e| AsrError::DatabaseError {
            detail: e.to_string(),
        })?;
        Ok(id)
    }

    /// Get a single record by id.
    pub fn get_record(&self, id: &str) -> Result<Option<TranscriptionRecord>, AsrError> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, timestamp, source, language, model_id, audio_duration_ms, inference_ms, text, segments_json, audio_path, has_error, error_message
                 FROM records WHERE id = ?1",
            )
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;

        let mut rows =
            stmt.query_map(params![id], row_to_record)
                .map_err(|e| AsrError::DatabaseError {
                    detail: e.to_string(),
                })?;

        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?)),
            None => Ok(None),
        }
    }

    /// Delete a record by id. Returns true if a row was deleted.
    pub fn delete_record(&self, id: &str) -> Result<bool, AsrError> {
        let conn = self.conn()?;
        let deleted = conn
            .execute("DELETE FROM records WHERE id = ?1", params![id])
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;
        Ok(deleted > 0)
    }

    /// List records with optional filters, ordered by timestamp descending.
    pub fn list_records(
        &self,
        filter: &ListRecordsFilter,
    ) -> Result<Vec<TranscriptionRecord>, AsrError> {
        let mut sql = String::from(
            "SELECT id, timestamp, source, language, model_id, audio_duration_ms, inference_ms, text, segments_json, audio_path, has_error, error_message
             FROM records WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(source) = &filter.source {
            sql.push_str(&format!(" AND source = ?{idx}"));
            param_values.push(Box::new(source.as_str().to_owned()));
            idx += 1;
        }
        if let Some(model_id) = &filter.model_id {
            sql.push_str(&format!(" AND model_id = ?{idx}"));
            param_values.push(Box::new(model_id.clone()));
            idx += 1;
        }
        if let Some(from) = &filter.from {
            sql.push_str(&format!(" AND timestamp >= ?{idx}"));
            param_values.push(Box::new(from.clone()));
            idx += 1;
        }
        if let Some(to) = &filter.to {
            sql.push_str(&format!(" AND timestamp <= ?{idx}"));
            param_values.push(Box::new(to.clone()));
            idx += 1;
        }

        sql.push_str(" ORDER BY timestamp DESC");

        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT ?{idx}"));
            param_values.push(Box::new(limit));
            idx += 1;
        }
        if let Some(offset) = filter.offset {
            sql.push_str(&format!(" OFFSET ?{idx}"));
            param_values.push(Box::new(offset));
        }

        let conn = self.conn()?;
        let mut stmt = conn.prepare(&sql).map_err(|e| AsrError::DatabaseError {
            detail: e.to_string(),
        })?;

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();

        let rows = stmt
            .query_map(params_ref.as_slice(), row_to_record)
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?);
        }
        Ok(records)
    }

    /// Delete records older than the given number of days. Returns the count deleted.
    pub fn cleanup_records_older_than_days(&self, days: i64) -> Result<usize, AsrError> {
        let conn = self.conn()?;
        let deleted = conn
            .execute(
                "DELETE FROM records WHERE timestamp < datetime('now', ?1)",
                params![format!("-{days} days")],
            )
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;
        Ok(deleted)
    }

    /// Get audio file paths for records older than the given number of days.
    pub fn get_audio_paths_older_than_days(&self, days: i64) -> Result<Vec<String>, AsrError> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT audio_path FROM records
                 WHERE audio_path IS NOT NULL AND timestamp < datetime('now', ?1)",
            )
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;

        let rows = stmt
            .query_map(params![format!("-{days} days")], |row| row.get(0))
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;

        let mut paths = Vec::new();
        for row in rows {
            paths.push(row.map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?);
        }
        Ok(paths)
    }

    /// Count records from today, optionally filtered by source.
    pub fn count_records_today(
        &self,
        source: Option<TranscriptionSource>,
    ) -> Result<usize, AsrError> {
        let conn = self.conn()?;
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
        }
        .map_err(|e| AsrError::DatabaseError {
            detail: e.to_string(),
        })?;
        Ok(count as usize)
    }

    /// Sum `audio_duration_ms` for all records.
    pub fn total_audio_duration_ms(&self) -> Result<i64, AsrError> {
        let conn = self.conn()?;
        let total: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(audio_duration_ms), 0) FROM records",
                [],
                |row| row.get(0),
            )
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;
        Ok(total)
    }

    /// Sum `audio_duration_ms` for today's records.
    pub fn today_audio_duration_ms(&self) -> Result<i64, AsrError> {
        let conn = self.conn()?;
        let total: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(audio_duration_ms), 0) FROM records WHERE timestamp >= datetime('now', 'start of day')",
                [],
                |row| row.get(0),
            )
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;
        Ok(total)
    }

    /// Average `inference_ms` across all records.
    pub fn avg_inference_ms(&self) -> Result<f64, AsrError> {
        let conn = self.conn()?;
        let avg: f64 = conn
            .query_row(
                "SELECT COALESCE(AVG(inference_ms), 0.0) FROM records",
                [],
                |row| row.get(0),
            )
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;
        Ok(avg)
    }

    /// Count records with `has_error = 1`. If `today_only` is true, restrict to today.
    pub fn count_errors(&self, today_only: bool) -> Result<usize, AsrError> {
        let conn = self.conn()?;
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
        }
        .map_err(|e| AsrError::DatabaseError {
            detail: e.to_string(),
        })?;
        Ok(count as usize)
    }

    /// Count total records, optionally filtered by source.
    pub fn count_records(&self, source: Option<TranscriptionSource>) -> Result<usize, AsrError> {
        let conn = self.conn()?;
        let count: i64 = if let Some(src) = source {
            conn.query_row(
                "SELECT COUNT(*) FROM records WHERE source = ?1",
                params![src.as_str()],
                |row| row.get(0),
            )
        } else {
            conn.query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
        }
        .map_err(|e| AsrError::DatabaseError {
            detail: e.to_string(),
        })?;
        Ok(count as usize)
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
        text: row.get(7)?,
        segments_json: row.get(8)?,
        audio_path: row.get(9)?,
        has_error: row.get::<_, i32>(10)? != 0,
        error_message: row.get(11)?,
    })
}

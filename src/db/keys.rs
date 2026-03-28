use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::database::Database;
use crate::error::AsrError;

/// A stored API key record (never exposes the raw key).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub id: String,
    pub name: String,
    pub last4: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

/// Base64url alphabet (RFC 4648 section 5, no padding).
const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode bytes to base64url without padding.
fn base64url_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() * 4).div_ceil(3));
    let chunks = data.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(B64URL[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64URL[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64URL[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(B64URL[(triple & 0x3F) as usize] as char);
        }
    }
    out
}

/// Hash a raw API key to its hex-encoded SHA-256 digest.
fn hash_key(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    hex::encode(digest)
}

impl Database {
    /// Create a new API key. Returns the record and the raw key (shown once).
    pub fn create_api_key(&self, name: &str) -> Result<(ApiKeyRecord, String), AsrError> {
        use rand::Rng;

        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        let raw_key = base64url_encode(&bytes);
        let key_hash = hash_key(&raw_key);
        let last4 = &raw_key[raw_key.len() - 4..];
        let id = uuid::Uuid::new_v4().to_string();

        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO api_keys (id, name, key_hash, last4) VALUES (?1, ?2, ?3, ?4)",
            params![id, name, key_hash, last4],
        )
        .map_err(|e| AsrError::DatabaseError {
            detail: e.to_string(),
        })?;

        // Read back to get server-generated created_at
        let record = conn
            .query_row(
                "SELECT id, name, last4, created_at, last_used_at FROM api_keys WHERE id = ?1",
                params![id],
                |row| {
                    Ok(ApiKeyRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        last4: row.get(2)?,
                        created_at: row.get(3)?,
                        last_used_at: row.get(4)?,
                    })
                },
            )
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;

        Ok((record, raw_key))
    }

    /// Verify a raw API key. Returns the matching record if valid, updating `last_used_at`.
    pub fn verify_api_key(&self, raw_key: &str) -> Result<Option<ApiKeyRecord>, AsrError> {
        let key_hash = hash_key(raw_key);
        let conn = self.conn()?;

        // Update last_used_at and return the row in one step
        let updated = conn
            .execute(
                "UPDATE api_keys SET last_used_at = datetime('now') WHERE key_hash = ?1",
                params![key_hash],
            )
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;

        if updated == 0 {
            return Ok(None);
        }

        let record = conn
            .query_row(
                "SELECT id, name, last4, created_at, last_used_at FROM api_keys WHERE key_hash = ?1",
                params![key_hash],
                |row| {
                    Ok(ApiKeyRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        last4: row.get(2)?,
                        created_at: row.get(3)?,
                        last_used_at: row.get(4)?,
                    })
                },
            )
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;

        Ok(Some(record))
    }

    /// List all API keys (without exposing hashes).
    pub fn list_api_keys(&self) -> Result<Vec<ApiKeyRecord>, AsrError> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT id, name, last4, created_at, last_used_at FROM api_keys ORDER BY created_at DESC")
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;

        let rows = stmt
            .query_map([], |row| {
                Ok(ApiKeyRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    last4: row.get(2)?,
                    created_at: row.get(3)?,
                    last_used_at: row.get(4)?,
                })
            })
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;

        let mut keys = Vec::new();
        for row in rows {
            keys.push(row.map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?);
        }
        Ok(keys)
    }

    /// Delete an API key by id. Returns true if a row was deleted.
    /// Ensure a specific raw API key exists in the database.
    /// Used for pre-configured keys (--api-key / API_KEY env).
    pub fn ensure_api_key(&self, name: &str, raw_key: &str) -> Result<(), AsrError> {
        let key_hash = hash_key(raw_key);
        let conn = self.conn()?;

        // Check if this hash already exists
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM api_keys WHERE key_hash = ?1",
                params![key_hash],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if exists {
            return Ok(());
        }

        let id = uuid::Uuid::new_v4().to_string();
        let last4 = if raw_key.len() >= 4 {
            raw_key[raw_key.len() - 4..].to_string()
        } else {
            raw_key.to_string()
        };

        conn.execute(
            "INSERT INTO api_keys (id, name, key_hash, last4) VALUES (?1, ?2, ?3, ?4)",
            params![id, name, key_hash, last4],
        )
        .map_err(|e| AsrError::DatabaseError {
            detail: format!("insert api key failed: {e}"),
        })?;

        Ok(())
    }

    pub fn delete_api_key(&self, id: &str) -> Result<bool, AsrError> {
        let conn = self.conn()?;
        let deleted = conn
            .execute("DELETE FROM api_keys WHERE id = ?1", params![id])
            .map_err(|e| AsrError::DatabaseError {
                detail: e.to_string(),
            })?;
        Ok(deleted > 0)
    }
}

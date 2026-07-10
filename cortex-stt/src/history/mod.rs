//! Transcription history — the unified concept of a DB row paired with
//! an optional audio file on disk. See `CONTEXT.md` for the domain
//! vocabulary (Delete record vs Drop audio, Retention candidate, …).
//!
//! New audio files are written as Ogg Opus (`.opus`) for storage
//! efficiency. Legacy `.wav` rows created before this change continue
//! to be served as-is; the read path picks Content-Type from the file
//! extension stored on the record.
//!
//! Two invariants this module exists to protect:
//!
//! 1. When a record is deleted, its audio file is removed too.
//! 2. When a record's audio is dropped, the `audio_path` column is
//!    NULL'd; no row may claim to have an audio file that has been
//!    deleted from disk.

mod analytics;
mod store;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::warn;

use crate::audio::opus_writer::write_opus;
use crate::db::database::Database;
use crate::error::AsrError;
use crate::retention::{RetentionCandidate, RetentionPolicy, select_to_delete};

pub use analytics::MetricsSnapshot;
pub use store::{
    CreateRecord, HistoryFacets, ListRecordsFilter, RecordSegment, TranscriptionRecord,
    TranscriptionSource,
};

/// Outcome of a bulk "delete everything" call. Surfaced verbatim in the
/// `DELETE /api/history` response.
#[derive(Debug, Clone, Copy)]
pub struct DeleteAllOutcome {
    pub records_deleted: usize,
    pub audio_files_deleted: usize,
}

/// Outcome of one retention sweep: how many records were removed
/// (Delete record) and how many rows had their audio detached (Drop
/// audio). Surfaced verbatim in the `POST /api/history/cleanup` response.
#[derive(Debug, Clone, Copy, Default)]
pub struct SweepOutcome {
    pub deleted_records: usize,
    pub dropped_audios: usize,
}

/// Transcription history store: DB rows + their paired WAV files +
/// a broadcast channel for live update notifications.
pub struct History {
    db: Arc<Database>,
    audio_dir: PathBuf,
    tx: broadcast::Sender<()>,
}

impl History {
    /// Build a new `History` rooted at `audio_dir`. The directory is
    /// created if missing and the `records` schema migrated — the module
    /// owns both the audio storage layout and its table.
    pub async fn new(db: Arc<Database>, audio_dir: PathBuf) -> Result<Arc<Self>, AsrError> {
        tokio::fs::create_dir_all(&audio_dir).await?;
        store::migrate(&db).await?;
        let (tx, _) = broadcast::channel(100);
        Ok(Arc::new(Self { db, audio_dir, tx }))
    }

    // -----------------------------------------------------------------
    // Create
    // -----------------------------------------------------------------

    /// Create a history record. If `samples` is `Some`, a WAV file is
    /// written under `audio_dir` and the row's `audio_path` is set;
    /// otherwise the row is stored with `audio_path = NULL`.
    ///
    /// If the WAV write fails the row is still inserted with
    /// `audio_path = NULL` (and a warning logged). If the row insert
    /// fails after a WAV was written, the WAV is cleaned up so a failed
    /// create never leaves an orphan file on disk.
    pub async fn create(
        &self,
        record: CreateRecord,
        samples: Option<&[f32]>,
    ) -> Result<String, AsrError> {
        let id = uuid::Uuid::new_v4().to_string();

        let audio_filename: Option<String> = match samples {
            Some(samples) => {
                let filename = format!("{id}.opus");
                let path = self.audio_dir.join(&filename);
                match write_opus(&path, samples).await {
                    Ok(()) => Some(filename),
                    Err(e) => {
                        warn!(error = %e, record_id = %id, "Failed to save audio file; recording row without audio");
                        None
                    }
                }
            }
            None => None,
        };

        if let Err(e) = store::insert(&self.db, &id, &record, audio_filename.as_deref()).await {
            // Compensate: the WAV (if any) is now orphaned — there's no
            // row pointing at it, so retention will never reclaim it.
            if let Some(filename) = audio_filename.as_deref() {
                let _ = try_remove_audio(&self.audio_dir, filename).await;
            }
            return Err(e);
        }

        // Notify live subscribers — best-effort, ignore "no listeners".
        let _ = self.tx.send(());

        Ok(id)
    }

    // -----------------------------------------------------------------
    // Read
    // -----------------------------------------------------------------

    pub async fn get(&self, id: &str) -> Result<Option<TranscriptionRecord>, AsrError> {
        store::get(&self.db, id).await
    }

    pub async fn list(
        &self,
        filter: &ListRecordsFilter,
    ) -> Result<Vec<TranscriptionRecord>, AsrError> {
        store::list(&self.db, filter).await
    }

    /// Distinct filterable values (models, capture devices) for the UI.
    pub async fn facets(&self) -> Result<store::HistoryFacets, AsrError> {
        store::facets(&self.db).await
    }

    /// Read the audio bytes for the given record plus the matching
    /// MIME type derived from the file extension (`.opus` →
    /// `audio/ogg`, `.wav` → `audio/wav`). Returns
    /// [`AsrError::RecordNotFound`] if the row is absent, or
    /// [`AsrError::NoAudio`] if the row has no audio_path.
    pub async fn read_audio(&self, id: &str) -> Result<(Vec<u8>, &'static str), AsrError> {
        let record = self
            .get(id)
            .await?
            .ok_or_else(|| AsrError::RecordNotFound {
                record_id: id.to_string(),
            })?;
        let filename = record.audio_path.ok_or_else(|| AsrError::NoAudio {
            record_id: id.to_string(),
        })?;
        let mime = mime_for(&filename);
        let path = self.audio_dir.join(filename);
        let bytes = tokio::fs::read(&path).await.map_err(AsrError::Io)?;
        Ok((bytes, mime))
    }

    // -----------------------------------------------------------------
    // Delete record (row + audio)
    // -----------------------------------------------------------------
    //
    // All delete operations remove the WAV files *before* the DB rows,
    // so a partial failure can never orphan a file (file gone but no
    // row referencing it). The opposite ordering risks orphans if the
    // process dies between the row delete and the fs unlink — those
    // files would then be invisible to retention and never reclaimed.

    /// Delete a single record. Removes the audio file (if any) first;
    /// if that fails, the row is left intact so the caller can retry.
    /// Returns `true` if a row was actually deleted.
    pub async fn delete(&self, id: &str) -> Result<bool, AsrError> {
        let audio = match store::lookup_audio_path(&self.db, id).await? {
            None => return Ok(false),
            Some(audio) => audio,
        };
        if let Some(filename) = audio.as_deref() {
            try_remove_audio(&self.audio_dir, filename).await?;
        }
        store::delete_row(&self.db, id).await
    }

    /// Delete a batch of records. Removes WAV files first; rows whose
    /// file removal failed are left alone so the next retention pass
    /// can retry. Returns the count of rows actually deleted.
    pub async fn delete_many(&self, ids: &[String]) -> Result<usize, AsrError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let pairs = store::lookup_audio_paths(&self.db, ids).await?;
        let (safe_ids, _audio_removed) = self.remove_files(pairs).await;
        store::delete_rows(&self.db, &safe_ids).await
    }

    /// Drop every record and audio file. Files that fail to remove
    /// keep their row in place (retried on the next sweep). The
    /// returned counts reflect what was actually completed.
    pub async fn delete_all(&self) -> Result<DeleteAllOutcome, AsrError> {
        let pairs = store::all_audio_paths(&self.db).await?;
        let (safe_ids, audio_files_deleted) = self.remove_files(pairs).await;
        let records_deleted = store::delete_rows(&self.db, &safe_ids).await?;
        Ok(DeleteAllOutcome {
            records_deleted,
            audio_files_deleted,
        })
    }

    // -----------------------------------------------------------------
    // Drop audio (audio file + NULL audio_path, row stays)
    // -----------------------------------------------------------------

    /// Drop the audio portion of the given records: remove each WAV
    /// first, then NULL `audio_path` only for the rows whose file we
    /// actually removed. Rows whose file removal failed retain their
    /// `audio_path` for the next retry. Returns the count of rows
    /// whose `audio_path` was set to NULL.
    pub async fn drop_audios(&self, ids: &[String]) -> Result<usize, AsrError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let pairs = store::lookup_audio_paths(&self.db, ids).await?;
        let mut updated_ids = Vec::new();
        for (id, audio_path) in pairs {
            let Some(filename) = audio_path else { continue };
            if try_remove_audio(&self.audio_dir, &filename).await.is_ok() {
                updated_ids.push(id);
            }
        }
        store::null_audio_paths(&self.db, &updated_ids).await
    }

    // -----------------------------------------------------------------
    // Retention candidate sources
    // -----------------------------------------------------------------

    /// Candidates for `record_retention`: id + created_at, no size.
    pub async fn list_record_candidates(&self) -> Result<Vec<RetentionCandidate>, AsrError> {
        store::list_record_candidates(&self.db).await
    }

    /// Candidates for `audio_retention`: id + created_at + size.
    /// Reads the size of each WAV file from disk. Files that no longer
    /// exist are reported with `size_bytes = Some(0)` so they still
    /// surface to the retention algorithm and their orphaned rows can
    /// be cleaned up by the disk-limit branch (other branches don't
    /// consult size).
    pub async fn list_audio_candidates(&self) -> Result<Vec<RetentionCandidate>, AsrError> {
        let rows = store::list_audio_rows(&self.db).await?;
        let mut out = Vec::with_capacity(rows.len());
        for (id, created_at, filename) in rows {
            let path = self.audio_dir.join(&filename);
            let size = tokio::fs::metadata(&path)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            out.push(RetentionCandidate {
                id,
                created_at,
                size_bytes: Some(size),
            });
        }
        Ok(out)
    }

    // -----------------------------------------------------------------
    // Retention sweep (the single composer of gather → select → apply)
    // -----------------------------------------------------------------

    /// Apply both retention policies now and report what was removed.
    ///
    /// The single home for the gather → `select_to_delete` → apply flow:
    /// the hourly sweep (`cleanup.rs`) and `POST /api/history/cleanup`
    /// both call this instead of re-wiring the ingredients. The two
    /// policies are independent (see CONTEXT.md) and both branches run
    /// even if one fails — a record-retention error never suppresses
    /// audio retention. Best-effort: a failed branch logs a warning and
    /// contributes 0 to the outcome.
    pub async fn run_retention_sweep(
        &self,
        record_policy: &RetentionPolicy,
        audio_policy: &RetentionPolicy,
    ) -> SweepOutcome {
        let deleted_records = self.sweep_records(record_policy).await;
        let dropped_audios = self.sweep_audios(audio_policy).await;
        SweepOutcome {
            deleted_records,
            dropped_audios,
        }
    }

    /// Record-retention branch: enumerate candidates, select by policy,
    /// Delete record. Errors log and yield 0 so the audio branch still runs.
    async fn sweep_records(&self, policy: &RetentionPolicy) -> usize {
        let candidates = match self.list_record_candidates().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "failed to enumerate record retention candidates");
                return 0;
            }
        };
        let ids = select_to_delete(&candidates, policy);
        match self.delete_many(&ids).await {
            Ok(deleted) => deleted,
            Err(e) => {
                warn!(error = %e, "failed to delete history records during sweep");
                0
            }
        }
    }

    /// Audio-retention branch: enumerate candidates, select by policy,
    /// Drop audio. Errors log and yield 0.
    async fn sweep_audios(&self, policy: &RetentionPolicy) -> usize {
        let candidates = match self.list_audio_candidates().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "failed to enumerate audio retention candidates");
                return 0;
            }
        };
        let ids = select_to_delete(&candidates, policy);
        match self.drop_audios(&ids).await {
            Ok(dropped) => dropped,
            Err(e) => {
                warn!(error = %e, "failed to drop audio during sweep");
                0
            }
        }
    }

    // -----------------------------------------------------------------
    // Storage
    // -----------------------------------------------------------------

    /// Total size of the audio files this store owns, in bytes. The
    /// audio directory layout is private to `History` — callers ask
    /// this instead of re-deriving the path.
    pub async fn audio_disk_usage_bytes(&self) -> u64 {
        let dir = self.audio_dir.clone();
        tokio::task::spawn_blocking(move || crate::model::storage::dir_size(&dir))
            .await
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------
    // Live updates
    // -----------------------------------------------------------------

    /// Subscribe to "history changed" notifications. Each fired event
    /// is a unit `()` — subscribers refetch on receipt. Fired only on
    /// `create`; bulk operations don't broadcast.
    pub fn subscribe_live(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }

    // -----------------------------------------------------------------
    // Internal: walk `(id, audio_path)` pairs, removing files and
    // collecting the ids that are now safe to delete from the DB.
    // -----------------------------------------------------------------

    /// For each pair, attempt to remove the audio file (if any). The
    /// returned `safe_ids` are ids whose row is now free of any on-disk
    /// artifact — either because there was no audio_path, or because
    /// we successfully removed (or already-missing) the file. Ids whose
    /// file removal failed are *omitted* so callers can retry later
    /// rather than orphan the file.
    ///
    /// The second tuple element is the count of audio files actually
    /// removed (used by `delete_all` to populate its response).
    async fn remove_files(&self, pairs: Vec<(String, Option<String>)>) -> (Vec<String>, usize) {
        let mut safe_ids = Vec::with_capacity(pairs.len());
        let mut audio_removed = 0usize;
        for (id, audio_path) in pairs {
            match audio_path {
                None => safe_ids.push(id),
                Some(filename) => match try_remove_audio(&self.audio_dir, &filename).await {
                    Ok(()) => {
                        safe_ids.push(id);
                        audio_removed += 1;
                    }
                    Err(_) => {
                        // File removal failed — leave the row alone.
                        // The error is already logged by try_remove_audio.
                    }
                },
            }
        }
        (safe_ids, audio_removed)
    }
}

/// MIME type for serving a history audio file based on its extension.
/// New rows are `.opus` (Ogg Opus); pre-migration rows remain `.wav`.
fn mime_for(filename: &str) -> &'static str {
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".opus") || lower.ends_with(".ogg") {
        "audio/ogg"
    } else {
        "audio/wav"
    }
}

/// Remove a single audio file. Missing files are treated as success
/// (idempotent cleanup — retention may try repeatedly). Other I/O
/// errors are logged and returned so the caller can leave the
/// referencing row in place.
async fn try_remove_audio(audio_dir: &Path, filename: &str) -> Result<(), AsrError> {
    let path = audio_dir.join(filename);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            warn!(error = %e, path = %path.display(), "failed to remove audio file");
            Err(AsrError::Io(e))
        }
    }
}

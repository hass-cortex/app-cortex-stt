//! Application settings — the single owned shape, used by both the
//! persistence layer (`db::settings`, stored as one JSON blob under the
//! `app_settings` key) and the HTTP API (`api::settings`). Living here
//! keeps the dependency arrows pointing inward: neither storage nor the
//! API layer defines the other's format.
//!
//! Field evolution: new fields must carry `#[serde(default)]` (or a
//! default fn) so blobs written by older versions still deserialize.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::engine::traits::BackendOverride;
use crate::retention::RetentionPolicy;

/// Application settings persisted in the DB and exposed via the REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// The user's explicit default-model choice, or `None` if never set —
    /// startup then falls back to the CLI/env/addon-option value. Written
    /// ONLY by `PUT /api/engine/default` (validated); `PUT /api/settings`
    /// preserves the stored value, so a whole-blob save can't clobber it.
    #[serde(default)]
    pub default_model: Option<String>,
    pub pool_size: usize,
    pub max_loaded_models: usize,
    /// None = keep models loaded forever; Some(n) = unload after n seconds idle.
    pub idle_timeout_secs: Option<u64>,
    /// None = no timeout; Some(n) = abort transcription after n seconds.
    pub transcription_timeout_secs: Option<u64>,
    pub save_audio: bool,
    pub audio_retention: RetentionPolicy,
    pub record_retention: RetentionPolicy,
    #[serde(default)]
    pub preload_default_model: bool,
    /// Timezone for display. "auto" = browser detection, or IANA timezone (e.g., "Asia/Taipei")
    #[serde(default = "default_timezone")]
    pub timezone: String,
    /// Per-model compute backend override. Key = model_id.
    #[serde(default)]
    pub backend_overrides: HashMap<String, BackendOverride>,
}

impl Settings {
    /// The engine idle-timeout these settings ask for — the single home
    /// for the "0 or null means keep loaded forever" rule. Both the
    /// startup precedence matrix (`EffectiveConfig::resolve`) and the
    /// runtime sync (`apply_engine_settings`) project through this.
    pub fn engine_idle_timeout(&self) -> Option<std::time::Duration> {
        match self.idle_timeout_secs {
            Some(0) | None => None,
            Some(secs) => Some(std::time::Duration::from_secs(secs)),
        }
    }
}

fn default_timezone() -> String {
    "auto".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_model: None,
            pool_size: 1,
            max_loaded_models: 1,
            idle_timeout_secs: None,
            transcription_timeout_secs: Some(300),
            save_audio: true,
            preload_default_model: false,
            audio_retention: RetentionPolicy::Days(7),
            record_retention: RetentionPolicy::Days(30),
            timezone: default_timezone(),
            backend_overrides: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_uses_days_policies() {
        let settings = Settings::default();
        assert_eq!(settings.audio_retention, RetentionPolicy::Days(7));
        assert_eq!(settings.record_retention, RetentionPolicy::Days(30));
    }

    #[test]
    fn settings_full_roundtrip() {
        let settings = Settings {
            audio_retention: RetentionPolicy::DiskLimitMb(2048),
            record_retention: RetentionPolicy::Count(500),
            ..Default::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.audio_retention, RetentionPolicy::DiskLimitMb(2048));
        assert_eq!(parsed.record_retention, RetentionPolicy::Count(500));
    }
}

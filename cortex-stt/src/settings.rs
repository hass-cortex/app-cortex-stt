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
    /// The **Pre-load**: the Variant key loaded at startup, or `None`.
    #[serde(default)]
    pub preload: Option<String>,
    /// Pre-Variant settings, read once by [`Settings::migrate_legacy_preload`]
    /// and never written back.
    #[serde(default, rename = "default_model", skip_serializing)]
    pub legacy_default_model: Option<String>,
    #[serde(default, rename = "preload_default_model", skip_serializing)]
    pub legacy_preload_default_model: bool,
    pub pool_size: usize,
    pub max_loaded_models: usize,
    /// None = keep models loaded forever; Some(n) = unload after n seconds idle.
    pub idle_timeout_secs: Option<u64>,
    /// None = no timeout; Some(n) = abort transcription after n seconds.
    pub transcription_timeout_secs: Option<u64>,
    pub save_audio: bool,
    pub audio_retention: RetentionPolicy,
    pub record_retention: RetentionPolicy,
    /// Timezone for display. "auto" = browser detection, or IANA timezone (e.g., "Asia/Taipei")
    #[serde(default = "default_timezone")]
    pub timezone: String,
    /// Per-model compute backend override. Key = model_id.
    #[serde(default)]
    pub backend_overrides: HashMap<String, BackendOverride>,
}

impl Settings {
    /// Each field that differs between `self` and `new`, as
    /// `field: old -> new` (values in compact JSON) — the audit line a
    /// settings change writes.
    pub fn changes_to(&self, new: &Settings) -> Vec<String> {
        let (Ok(serde_json::Value::Object(old)), Ok(serde_json::Value::Object(new))) =
            (serde_json::to_value(self), serde_json::to_value(new))
        else {
            return Vec::new();
        };
        new.iter()
            .filter_map(|(field, value)| {
                let before = old.get(field).unwrap_or(&serde_json::Value::Null);
                (before != value).then(|| format!("{field}: {before} -> {value}"))
            })
            .collect()
    }
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

impl Settings {
    /// Fold the pre-Variant default model into the **Pre-load**: it was
    /// only ever used when `preload_default_model` was on. `resolve` turns
    /// the model id into the key of the Variant it served (`None` when
    /// nothing is installed). Returns whether the stored blob still holds
    /// legacy fields and must be rewritten.
    pub fn migrate_legacy_preload(&mut self, resolve: impl FnOnce(&str) -> Option<String>) -> bool {
        let legacy = self.legacy_default_model.take();
        let wanted = std::mem::take(&mut self.legacy_preload_default_model);
        if self.preload.is_none()
            && wanted
            && let Some(model) = &legacy
        {
            self.preload = resolve(model);
        }
        legacy.is_some() || wanted
    }
}

fn default_timezone() -> String {
    "auto".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            preload: None,
            legacy_default_model: None,
            legacy_preload_default_model: false,
            pool_size: 1,
            max_loaded_models: 1,
            idle_timeout_secs: None,
            transcription_timeout_secs: Some(300),
            save_audio: true,
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
    fn a_settings_change_names_the_field_and_both_values() {
        let old = Settings::default();
        let mut new = old.clone();
        new.preload = Some("SenseVoiceSmall:Q8_0".to_string());
        new.save_audio = !old.save_audio;
        let changes = old.changes_to(&new);
        assert!(
            changes.contains(&r#"preload: null -> "SenseVoiceSmall:Q8_0""#.to_string()),
            "{changes:?}"
        );
        assert!(
            changes.iter().any(|c| c.starts_with("save_audio: ")),
            "{changes:?}"
        );
    }

    #[test]
    fn an_unchanged_save_reports_no_changes() {
        let settings = Settings::default();
        assert!(settings.changes_to(&settings.clone()).is_empty());
    }

    #[test]
    fn settings_default_uses_days_policies() {
        let settings = Settings::default();
        assert_eq!(settings.audio_retention, RetentionPolicy::Days(7));
        assert_eq!(settings.record_retention, RetentionPolicy::Days(30));
    }

    #[test]
    fn a_preloaded_default_model_becomes_the_preload_variant() {
        // A blob as a pre-Variant release wrote it.
        let mut stored = serde_json::to_value(Settings::default()).unwrap();
        stored["default_model"] = "SenseVoiceSmall".into();
        stored["preload_default_model"] = true.into();
        let mut settings: Settings = serde_json::from_value(stored).unwrap();

        assert!(settings.migrate_legacy_preload(|m| Some(format!("{m}:Q8_0"))));
        assert_eq!(settings.preload.as_deref(), Some("SenseVoiceSmall:Q8_0"));
        let written = serde_json::to_value(&settings).unwrap();
        assert!(written.get("default_model").is_none());
        assert!(written.get("preload_default_model").is_none());
        // Once rewritten there is nothing left to migrate.
        assert!(!settings.migrate_legacy_preload(|_| unreachable!()));
    }

    #[test]
    fn a_default_model_that_was_not_preloaded_preloads_nothing() {
        let mut settings = Settings {
            legacy_default_model: Some("SenseVoiceSmall".into()),
            ..Default::default()
        };
        assert!(settings.migrate_legacy_preload(|m| Some(format!("{m}:Q8_0"))));
        assert_eq!(settings.preload, None);
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

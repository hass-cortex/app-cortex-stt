//! Registry consistency tests — always run, no models or network needed.

use cortex_stt_server::engine::registry::builtin_models;
use cortex_stt_server::model::download::validate_download_url;

#[test]
fn all_model_ids_are_unique() {
    let models = builtin_models();
    let mut ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    let original_len = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), original_len, "duplicate model IDs found");
}

#[test]
fn all_models_have_url() {
    for m in &builtin_models() {
        assert!(!m.url.is_empty(), "model {} has empty URL", m.id);
    }
}

#[test]
fn all_urls_are_https_and_whitelisted() {
    for m in &builtin_models() {
        assert!(
            m.url.starts_with("https://"),
            "model {} URL not HTTPS: {}",
            m.id,
            m.url
        );
        assert!(
            validate_download_url(&m.url),
            "model {} URL not whitelisted: {}",
            m.id,
            m.url
        );
    }
}

#[test]
fn all_directory_models_have_archive_dir_name() {
    for m in &builtin_models() {
        if m.is_directory {
            assert!(
                !m.archive_dir_name.is_empty(),
                "directory model {} missing archive_dir_name",
                m.id
            );
        }
    }
}

#[test]
fn all_models_have_supported_languages() {
    for m in &builtin_models() {
        assert!(
            !m.supported_languages.is_empty(),
            "model {} has no languages",
            m.id
        );
    }
}

#[test]
fn model_count_matches_expected() {
    assert_eq!(builtin_models().len(), 13, "expected 13 builtin models");
}

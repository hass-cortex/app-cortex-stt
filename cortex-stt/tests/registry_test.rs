//! Vendored catalog consistency tests — always run, no models or network
//! needed. Exercises the public `catalog_data` API (the old `engine::registry`
//! builtin-model list was removed in 0.3.0).

use std::collections::HashSet;

use cortex_stt::model::catalog_data::{catalog_models, find_model};
use cortex_stt::model::download::validate_download_url;

#[test]
fn catalog_is_non_empty() {
    assert!(!catalog_models().is_empty(), "vendored catalog is empty");
}

#[test]
fn known_model_ids_are_present() {
    for id in [
        "whisper-tiny",
        "whisper-small",
        "Breeze-ASR-25",
        "SenseVoiceSmall",
    ] {
        assert!(find_model(id).is_some(), "catalog is missing model {id}");
    }
}

#[test]
fn every_model_default_quant_exists() {
    for m in catalog_models() {
        assert!(!m.quants.is_empty(), "model {} has no quants", m.id);
        assert!(
            m.quant(&m.default_quant).is_some(),
            "model {} default_quant {} is not among its quants",
            m.id,
            m.default_quant
        );
    }
}

#[test]
fn every_quant_has_a_valid_sha256() {
    for m in catalog_models() {
        for q in &m.quants {
            assert_eq!(
                q.sha256.len(),
                64,
                "model {} quant {} has a non-64-char sha256",
                m.id,
                q.quant
            );
        }
    }
}

#[test]
fn all_urls_are_https_and_whitelisted() {
    for m in catalog_models() {
        for q in &m.quants {
            assert!(
                q.url.starts_with("https://"),
                "model {} quant {} URL not HTTPS: {}",
                m.id,
                q.quant,
                q.url
            );
            assert!(
                validate_download_url(&q.url),
                "model {} quant {} URL not whitelisted: {}",
                m.id,
                q.quant,
                q.url
            );
        }
    }
}

#[test]
fn every_model_declares_languages() {
    for m in catalog_models() {
        assert!(!m.languages.is_empty(), "model {} has no languages", m.id);
    }
}

#[test]
fn model_ids_are_unique() {
    let mut seen = HashSet::new();
    for m in catalog_models() {
        assert!(seen.insert(m.id.as_str()), "duplicate model id {}", m.id);
    }
}

#[test]
fn quant_filenames_are_globally_unique() {
    let mut seen = HashSet::new();
    for m in catalog_models() {
        for q in &m.quants {
            assert!(
                seen.insert(q.filename.as_str()),
                "duplicate quant filename {}",
                q.filename
            );
        }
    }
}

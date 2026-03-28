use std::io::Write;

use wyoming_asr::model::download::{compute_sha256, validate_download_url};

#[test]
fn test_validate_url_allows_huggingface() {
    assert!(validate_download_url(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin"
    ));
    // Subdomain should also be allowed.
    assert!(validate_download_url(
        "https://cdn-lfs.huggingface.co/repos/some-hash/file.bin"
    ));
}

#[test]
fn test_validate_url_allows_github() {
    assert!(validate_download_url(
        "https://github.com/rhasspy/models/releases/download/v1.0/model.onnx"
    ));
}

#[test]
fn test_validate_url_rejects_other_hosts() {
    // Unknown host.
    assert!(!validate_download_url(
        "https://evil.com/malicious-model.bin"
    ));
    // HTTP (not HTTPS).
    assert!(!validate_download_url(
        "http://huggingface.co/some/model.bin"
    ));
    // Completely invalid URL.
    assert!(!validate_download_url("not-a-url"));
    // FTP scheme.
    assert!(!validate_download_url("ftp://huggingface.co/model.bin"));
}

#[tokio::test]
async fn test_compute_sha256_correct() {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(b"hello world").unwrap();
    tmp.flush().unwrap();

    let hash = compute_sha256(tmp.path()).await.unwrap();

    // Known SHA-256 of "hello world".
    assert_eq!(
        hash,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

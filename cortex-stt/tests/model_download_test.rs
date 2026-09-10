use std::io::Write;

use cortex_stt::model::download::{compute_sha256, validate_download_url};

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
fn test_validate_url_rejects_other_hosts() {
    // GitHub is no longer whitelisted — the GGUF catalog is Hugging Face only.
    assert!(!validate_download_url(
        "https://github.com/rhasspy/models/releases/download/v1.0/model.onnx"
    ));
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

/// The hash is computed by streaming a 64 KB buffer, so a file smaller
/// than one buffer never exercises the read loop. This one spans several
/// buffers and ends mid-buffer, which is where an off-by-one in the
/// `&buf[..n]` slice would show up.
#[tokio::test]
async fn test_compute_sha256_spans_buffer_boundaries() {
    use sha2::{Digest, Sha256};

    // 2.5 buffers: two full reads plus a partial tail.
    let len = 64 * 1024 * 2 + 12_345;
    let data: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();

    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(&data).unwrap();
    tmp.flush().unwrap();

    let expected = hex::encode(Sha256::digest(&data));
    let actual = compute_sha256(tmp.path()).await.unwrap();

    assert_eq!(
        actual, expected,
        "streamed digest must match whole-file digest"
    );
}

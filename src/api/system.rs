use std::sync::Arc;
use std::thread::available_parallelism;

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
struct SystemInfoResponse {
    cpu_count: usize,
    total_memory_mb: u64,
    available_memory_mb: u64,
    has_avx: bool,
    has_avx2: bool,
    cuda_available: bool,
    os: &'static str,
    arch: &'static str,
}

/// Parse a field from /proc/meminfo and return its value in kB.
#[cfg(target_os = "linux")]
fn read_meminfo_kb(field: &str) -> Option<u64> {
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in content.lines() {
        if line.starts_with(field) {
            let value = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
            return Some(value);
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn read_meminfo_kb(_field: &str) -> Option<u64> {
    None
}

/// Detect CUDA availability at runtime.
///
/// Returns `true` only when the binary was compiled with a CUDA feature
/// (`whisper-cuda` or `ort-cuda`) **and** `nvidia-smi` succeeds at runtime.
fn detect_cuda() -> bool {
    let compiled_with_cuda = cfg!(feature = "whisper-cuda") || cfg!(feature = "ort-cuda");
    if !compiled_with_cuda {
        return false;
    }
    std::process::Command::new("nvidia-smi")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn get_system_info(State(_state): State<Arc<AppState>>) -> axum::Json<SystemInfoResponse> {
    let cpu_count = available_parallelism().map(|n| n.get()).unwrap_or(1);

    let total_memory_mb = read_meminfo_kb("MemTotal:").unwrap_or(0) / 1024;
    let available_memory_mb = read_meminfo_kb("MemAvailable:").unwrap_or(0) / 1024;

    let has_avx = cfg!(target_feature = "avx");
    let has_avx2 = cfg!(target_feature = "avx2");

    axum::Json(SystemInfoResponse {
        cpu_count,
        total_memory_mb,
        available_memory_mb,
        has_avx,
        has_avx2,
        cuda_available: detect_cuda(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    })
}

pub fn system_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/system", get(get_system_info))
}

use std::sync::Arc;
use std::thread::available_parallelism;

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use serde::Serialize;

use crate::model::storage::{dir_size, free_disk_space};
use crate::state::AppState;

#[derive(Debug, Serialize)]
struct SystemInfoResponse {
    cpu_count: usize,
    total_memory_mb: u64,
    available_memory_mb: u64,
    has_avx: bool,
    has_avx2: bool,
    cuda_available: bool,
    /// Which engine backends have GPU acceleration compiled in.
    gpu_engines: GpuEngines,
    os: &'static str,
    arch: &'static str,
}

#[derive(Debug, Serialize)]
struct GpuEngines {
    whisper: bool,
    onnx: bool,
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

/// Cached CUDA availability (static per process — CUDA doesn't appear/disappear at runtime).
static CUDA_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Detect CUDA availability at runtime (cached after first call).
///
/// Returns `true` only when the binary was compiled with a CUDA feature
/// (`whisper-cuda` or `ort-cuda`) **and** `nvidia-smi` succeeds at runtime.
fn detect_cuda() -> bool {
    *CUDA_AVAILABLE.get_or_init(|| {
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
    })
}

/// Runtime hardware capabilities used for model recommendation.
#[derive(Debug, Clone)]
pub struct HardwareCapabilities {
    pub available_memory_mb: u64,
    pub has_avx: bool,
    pub cuda_available: bool,
}

impl HardwareCapabilities {
    /// Detect current hardware capabilities.
    pub fn detect() -> Self {
        Self {
            available_memory_mb: read_meminfo_kb("MemAvailable:").unwrap_or(0) / 1024,
            has_avx: std::arch::is_x86_feature_detected!("avx"),
            cuda_available: detect_cuda(),
        }
    }
}

async fn get_system_info(State(_state): State<Arc<AppState>>) -> axum::Json<SystemInfoResponse> {
    let cpu_count = available_parallelism().map(|n| n.get()).unwrap_or(1);

    let total_memory_mb = read_meminfo_kb("MemTotal:").unwrap_or(0) / 1024;
    let hw = HardwareCapabilities::detect();

    axum::Json(SystemInfoResponse {
        cpu_count,
        total_memory_mb,
        available_memory_mb: hw.available_memory_mb,
        has_avx: hw.has_avx,
        has_avx2: std::arch::is_x86_feature_detected!("avx2"),
        cuda_available: hw.cuda_available,
        gpu_engines: GpuEngines {
            whisper: cfg!(feature = "whisper-cuda"),
            onnx: cfg!(feature = "ort-cuda"),
        },
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    })
}

#[derive(Debug, Serialize)]
struct StorageResponse {
    models_bytes: u64,
    audio_bytes: u64,
    database_bytes: u64,
    free_bytes: u64,
}

async fn get_storage_info(State(state): State<Arc<AppState>>) -> axum::Json<StorageResponse> {
    let model_dir = state.model_manager.model_dir().to_path_buf();
    let audio_dir = state.data_dir.join("audio");
    let db_path = state.data_dir.join("records.db");
    let free_path = state.data_dir.clone();

    // Compute sizes on a blocking thread to avoid blocking the async runtime.
    let (models_bytes, audio_bytes, database_bytes, free_bytes) =
        tokio::task::spawn_blocking(move || {
            (
                dir_size(&model_dir),
                dir_size(&audio_dir),
                dir_size(&db_path),
                free_disk_space(&free_path),
            )
        })
        .await
        .unwrap_or((0, 0, 0, 0));

    axum::Json(StorageResponse {
        models_bytes,
        audio_bytes,
        database_bytes,
        free_bytes,
    })
}

pub fn system_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/system", get(get_system_info))
        .route("/api/storage", get(get_storage_info))
}

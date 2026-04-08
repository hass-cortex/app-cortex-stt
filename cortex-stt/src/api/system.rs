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
    /// GPU hardware info (present only when CUDA is detected).
    gpu_info: Option<GpuInfo>,
    /// Which engine backends have GPU acceleration compiled in.
    gpu_engines: GpuEngines,
    os: &'static str,
    arch: &'static str,
}

#[derive(Debug, Serialize)]
struct GpuInfo {
    name: String,
    memory_total_mb: u64,
    memory_used_mb: u64,
    memory_free_mb: u64,
    driver_version: String,
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

/// Try PATH first, then common install locations (WSL, CUDA toolkit).
const NVIDIA_SMI_CANDIDATES: &[&str] = &["nvidia-smi", "/usr/lib/wsl/lib/nvidia-smi", "/usr/bin/nvidia-smi"];

/// Cached CUDA availability (static per process — CUDA doesn't appear/disappear at runtime).
static CUDA_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Detect whether an NVIDIA GPU is present (cached after first call).
///
/// Returns `true` when `nvidia-smi` succeeds, regardless of whether the
/// binary was compiled with CUDA features.
fn detect_cuda() -> bool {
    *CUDA_AVAILABLE.get_or_init(|| {
        for cmd in NVIDIA_SMI_CANDIDATES {
            let result = std::process::Command::new(cmd)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            match &result {
                Ok(status) if status.success() => {
                    tracing::info!(cmd, "CUDA detected via nvidia-smi");
                    return true;
                }
                Ok(status) => {
                    tracing::debug!(cmd, code = ?status.code(), "nvidia-smi exited non-zero");
                }
                Err(e) => {
                    tracing::debug!(cmd, error = %e, "nvidia-smi not found");
                }
            }
        }
        tracing::info!("No CUDA GPU detected");
        false
    })
}

/// Query GPU details via nvidia-smi.
fn query_gpu_info() -> Option<GpuInfo> {
    for cmd in NVIDIA_SMI_CANDIDATES {
        let output = match std::process::Command::new(cmd)
            .args([
                "--query-gpu=name,memory.total,memory.used,memory.free,driver_version",
                "--format=csv,noheader,nounits",
            ])
            .output()
        {
            Ok(o) => o,
            Err(_) => continue,
        };
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = text.trim().splitn(5, ", ").collect();
        if parts.len() == 5 {
            return Some(GpuInfo {
                name: parts[0].to_string(),
                memory_total_mb: parts[1].parse().unwrap_or(0),
                memory_used_mb: parts[2].parse().unwrap_or(0),
                memory_free_mb: parts[3].parse().unwrap_or(0),
                driver_version: parts[4].to_string(),
            });
        }
    }
    None
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
    ///
    /// `cuda_available` here means the binary can actually run CUDA workloads
    /// (hardware present AND compiled with CUDA features).
    pub fn detect() -> Self {
        let compiled_with_cuda = cfg!(feature = "cuda");
        Self {
            available_memory_mb: read_meminfo_kb("MemAvailable:").unwrap_or(0) / 1024,
            has_avx: std::arch::is_x86_feature_detected!("avx"),
            cuda_available: compiled_with_cuda && detect_cuda(),
        }
    }
}

async fn get_system_info(State(_state): State<Arc<AppState>>) -> axum::Json<SystemInfoResponse> {
    let cpu_count = available_parallelism().map(|n| n.get()).unwrap_or(1);

    let total_memory_mb = read_meminfo_kb("MemTotal:").unwrap_or(0) / 1024;
    let hw = HardwareCapabilities::detect();

    let cuda = detect_cuda();
    let gpu_info = if cuda { query_gpu_info() } else { None };

    axum::Json(SystemInfoResponse {
        cpu_count,
        total_memory_mb,
        available_memory_mb: hw.available_memory_mb,
        has_avx: hw.has_avx,
        has_avx2: std::arch::is_x86_feature_detected!("avx2"),
        cuda_available: cuda,
        gpu_info,
        gpu_engines: GpuEngines {
            whisper: cfg!(feature = "cuda"),
            onnx: cfg!(feature = "cuda"),
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

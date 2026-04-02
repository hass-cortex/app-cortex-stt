//! CLI tool for testing cortex-stt-server model download and transcription pipeline.
//!
//! Usage:
//!   asr-cli list                              # List available models
//!   asr-cli download <model-id>               # Download a model
//!   asr-cli transcribe <model-id> <wav-file>  # Transcribe an audio file
//!   asr-cli test <model-id> <wav-file>        # Download (if needed) + transcribe
//!   asr-cli test-all <wav-dir>                # Test all downloaded models

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

use cortex_stt_server::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt_server::engine::registry::builtin_models;
use cortex_stt_server::model::download::{DownloadConfig, download_model, validate_download_url};
use cortex_stt_server::model::manager::ModelManager;
use cortex_stt_server::model::types::ModelStatus;

#[derive(Parser)]
#[command(name = "asr-cli", about = "Test tool for cortex-stt-server models")]
struct Cli {
    /// Model storage directory
    #[arg(long, env = "MODEL_DIR", default_value = "./data/models")]
    model_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List all available models and their status
    List,

    /// Download a model by ID
    Download { model_id: String },

    /// Transcribe a WAV file with a specific model
    Transcribe {
        model_id: String,
        wav_file: PathBuf,
        /// Language hint (e.g., "zh", "en", "ja")
        #[arg(long, short)]
        language: Option<String>,
    },

    /// Download (if needed) and transcribe — full pipeline test
    Test {
        model_id: String,
        wav_file: PathBuf,
        #[arg(long, short)]
        language: Option<String>,
    },

    /// Test all downloaded models against audio files in a directory
    TestAll {
        /// Directory containing WAV files named by language (e.g., zh.wav, en.wav, ja.wav)
        wav_dir: PathBuf,
    },

    /// Verify all model download URLs are reachable (HEAD request)
    VerifyUrls,

    /// Download all registry models, verifying each stage
    DownloadAll,

    /// Verify each downloaded model: file exists → correct structure → can load → can transcribe
    Verify {
        /// Optional: only verify this model
        model_id: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    std::fs::create_dir_all(&cli.model_dir)?;

    let model_manager = ModelManager::new(cli.model_dir.clone());

    match cli.command {
        Command::List => cmd_list(&model_manager).await,
        Command::Download { model_id } => cmd_download(&model_id, &model_manager).await,
        Command::Transcribe {
            model_id,
            wav_file,
            language,
        } => cmd_transcribe(&model_id, &wav_file, language.as_deref(), &cli.model_dir).await,
        Command::Test {
            model_id,
            wav_file,
            language,
        } => {
            cmd_download_if_needed(&model_id, &model_manager).await?;
            cmd_transcribe(&model_id, &wav_file, language.as_deref(), &cli.model_dir).await
        }
        Command::TestAll { wav_dir } => {
            cmd_test_all(&wav_dir, &model_manager, &cli.model_dir).await
        }
        Command::VerifyUrls => cmd_verify_urls().await,
        Command::DownloadAll => cmd_download_all(&model_manager).await,
        Command::Verify { model_id } => cmd_verify(model_id.as_deref(), &cli.model_dir).await,
    }
}

async fn cmd_list(manager: &ModelManager) -> Result<(), Box<dyn std::error::Error>> {
    let models = manager.list_models().await;

    println!(
        "{:<25} {:<12} {:<12} {:<8} Languages",
        "ID", "Engine", "Status", "Size"
    );
    println!("{}", "-".repeat(80));

    for m in &models {
        let langs = if m.supported_languages.len() > 5 {
            format!(
                "{} (+{} more)",
                m.supported_languages[..5].join(","),
                m.supported_languages.len() - 5
            )
        } else {
            m.supported_languages.join(",")
        };

        let status = match m.status {
            ModelStatus::Downloaded => "✓ downloaded",
            ModelStatus::Available => "  available",
            ModelStatus::Custom => "✓ custom",
            ModelStatus::Downloading => "⟳ downloading",
            ModelStatus::Error => "✗ error",
        };

        println!(
            "{:<25} {:<12} {:<12} {:>5}MB  {}",
            m.id,
            format!("{:?}", m.engine_type),
            status,
            m.size_mb,
            langs
        );
    }

    let downloaded = models
        .iter()
        .filter(|m| matches!(m.status, ModelStatus::Downloaded | ModelStatus::Custom))
        .count();
    println!("\n{downloaded}/{} models downloaded", models.len());

    Ok(())
}

async fn cmd_download(
    model_id: &str,
    manager: &ModelManager,
) -> Result<(), Box<dyn std::error::Error>> {
    let models = manager.list_models().await;
    let model_info = models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("Model not found: {model_id}"))?;

    if matches!(
        model_info.status,
        ModelStatus::Downloaded | ModelStatus::Custom
    ) {
        println!(
            "✓ Model '{model_id}' already downloaded at {}",
            manager.model_dir().join(&model_info.filename).display()
        );
        return Ok(());
    }

    // Get URL and SHA from registry definition
    let registry = builtin_models();
    let def = registry
        .iter()
        .find(|d| d.id == model_id)
        .ok_or_else(|| format!("Model not in registry: {model_id}"))?;

    if def.url.is_empty() {
        return Err(format!("No download URL for model: {model_id}").into());
    }

    println!(
        "Downloading '{model_id}' ({} MB) from {}...",
        def.size_mb, def.url
    );

    let dest_path = manager.model_dir().join(&def.filename);
    let manager_arc = ModelManager::new(manager.model_dir().to_path_buf());

    let handle = download_model(
        &def.url,
        dest_path.clone(),
        &def.sha256,
        model_id,
        manager_arc,
        DownloadConfig {
            verify_sha256: !def.sha256.is_empty(),
            ..Default::default()
        },
    )
    .await?;
    let mut rx = handle.progress_rx;

    // Poll progress
    loop {
        if rx.changed().await.is_err() {
            break;
        }
        let progress = rx.borrow().clone();
        if progress.total_bytes > 0 {
            let pct = progress.downloaded_bytes as f64 / progress.total_bytes as f64 * 100.0;
            print!(
                "\r  {:.1}% ({} / {} MB)",
                pct,
                progress.downloaded_bytes / (1024 * 1024),
                progress.total_bytes / (1024 * 1024)
            );
        }
    }
    println!();

    // Verify it's now downloaded
    tokio::time::sleep(Duration::from_secs(1)).await;
    if dest_path.exists() {
        println!("✓ Downloaded to {}", dest_path.display());
    } else {
        println!("✗ Download may have failed — check logs");
    }

    Ok(())
}

async fn cmd_download_if_needed(
    model_id: &str,
    manager: &ModelManager,
) -> Result<(), Box<dyn std::error::Error>> {
    let models = manager.list_models().await;
    let model = models.iter().find(|m| m.id == model_id);

    match model {
        Some(m) if matches!(m.status, ModelStatus::Downloaded | ModelStatus::Custom) => {
            println!("✓ Model '{model_id}' already downloaded");
            Ok(())
        }
        Some(_) => cmd_download(model_id, manager).await,
        None => Err(format!("Model not found: {model_id}").into()),
    }
}

async fn cmd_transcribe(
    model_id: &str,
    wav_file: &Path,
    language: Option<&str>,
    model_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if !wav_file.exists() {
        return Err(format!("WAV file not found: {}", wav_file.display()).into());
    }

    // Read and parse WAV
    let wav_data = std::fs::read(wav_file)?;
    let samples = cortex_stt_server::audio::resample::resample_to_16khz_mono(&wav_data)?;
    let duration_secs = samples.len() as f32 / 16000.0;
    println!(
        "Audio: {:.2}s, {} samples (16kHz mono)",
        duration_secs,
        samples.len()
    );

    // Create engine manager and register the model
    let engine_config = EngineManagerConfig {
        pool_size: 1,
        max_loaded_models: 1,
        idle_timeout: None,
        acquire_timeout: Duration::from_secs(300),
        idle_check_interval: Duration::from_secs(60),
    };
    let engine_manager = EngineManager::new(engine_config);

    // Determine engine type and model path — check both registry and scanned models
    let manager = ModelManager::new(model_dir.to_path_buf());
    let all_models = manager.list_models().await;
    let model_info = all_models.iter().find(|m| m.id == model_id);

    let (model_path, engine_type) = match model_info {
        Some(info) => (model_dir.join(&info.filename), info.engine_type.clone()),
        None => {
            // Also check builtin registry
            let registry = builtin_models();
            match registry.iter().find(|m| m.id == model_id) {
                Some(def) => (model_dir.join(&def.filename), def.engine_type.clone()),
                None => return Err(format!("Model not found: {model_id}").into()),
            }
        }
    };

    if !model_path.exists() {
        return Err(format!(
            "Model not found on disk: {}. Run 'asr-cli download {model_id}' first.",
            model_path.display()
        )
        .into());
    }

    println!("Loading model '{model_id}' ({engine_type:?})...");
    let load_start = Instant::now();

    register_engine(&engine_manager, model_id, model_path, &engine_type).await?;

    // Transcribe
    let options = cortex_stt_server::engine::traits::TranscribeOptions {
        language: language.map(String::from),
        translate: false,
    };

    let mut guard = engine_manager.acquire(model_id).await?;
    let load_ms = load_start.elapsed().as_millis();

    let infer_start = Instant::now();
    let result = guard.transcribe(&samples, &options)?;
    let infer_ms = infer_start.elapsed().as_millis();
    drop(guard);

    // Output results
    println!("\n┌─────────────────────────────────────────────");
    println!("│ Model:      {model_id} ({engine_type:?})");
    println!("│ Language:   {}", language.unwrap_or("auto"));
    println!("│ Audio:      {:.2}s", duration_secs);
    println!("│ Load time:  {load_ms}ms");
    println!("│ Inference:  {infer_ms}ms");
    println!(
        "│ RTF:        {:.2}x",
        infer_ms as f32 / (duration_secs * 1000.0)
    );
    println!("├─────────────────────────────────────────────");
    println!("│ Text: {}", result.text);

    if !result.segments.is_empty() {
        println!("│ Segments:");
        for seg in &result.segments {
            println!("│   [{:.2}s - {:.2}s] {}", seg.start, seg.end, seg.text);
        }
    }
    println!("└─────────────────────────────────────────────");

    Ok(())
}

async fn cmd_test_all(
    wav_dir: &Path,
    manager: &ModelManager,
    model_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if !wav_dir.exists() {
        return Err(format!("WAV directory not found: {}", wav_dir.display()).into());
    }

    let models = manager.list_models().await;
    let downloaded: Vec<_> = models
        .iter()
        .filter(|m| matches!(m.status, ModelStatus::Downloaded | ModelStatus::Custom))
        .collect();

    if downloaded.is_empty() {
        println!("No downloaded models. Run 'asr-cli download <model-id>' first.");
        return Ok(());
    }

    println!(
        "Testing {} downloaded models against audio files in {}...\n",
        downloaded.len(),
        wav_dir.display()
    );

    for model in &downloaded {
        // Find a matching audio file
        let wav_file = find_test_audio(wav_dir, &model.supported_languages);
        let wav_file = match wav_file {
            Some(f) => f,
            None => {
                println!("⏭  {}: no matching audio file found", model.id);
                continue;
            }
        };

        let lang = wav_file
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from);

        println!("Testing {} with {}...", model.id, wav_file.display());
        match cmd_transcribe(&model.id, &wav_file, lang.as_deref(), model_dir).await {
            Ok(_) => println!(),
            Err(e) => println!("  ✗ Error: {e}\n"),
        }
    }

    Ok(())
}

fn find_test_audio(wav_dir: &Path, languages: &[String]) -> Option<PathBuf> {
    // Try language-specific files first (zh.wav, en.wav, etc.)
    for lang in languages {
        let path = wav_dir.join(format!("{lang}.wav"));
        if path.exists() {
            return Some(path);
        }
    }
    // Fallback to any .wav file
    std::fs::read_dir(wav_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .find(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "wav")
                .unwrap_or(false)
        })
        .map(|e| e.path())
}

/// Register an engine factory for the given model, dispatching on engine type.
///
/// Feature-gated: requires `whisper` for Whisper models and `onnx` for all
/// ONNX-based engines (Parakeet, SenseVoice, GigaAM, Moonshine, Canary).
#[allow(
    unused_variables,
    unused_imports,
    unreachable_code,
    unreachable_patterns
)]
async fn register_engine(
    engine_manager: &EngineManager,
    model_id: &str,
    model_path: PathBuf,
    engine_type: &cortex_stt_server::engine::registry::EngineType,
) -> Result<(), Box<dyn std::error::Error>> {
    use cortex_stt_server::engine::registry::EngineType;

    match engine_type {
        #[cfg(feature = "whisper")]
        EngineType::Whisper => {
            let factory = cortex_stt_server::engine::whisper_bridge::whisper_factory(model_path);
            engine_manager.register(model_id, factory).await;
        }
        #[cfg(feature = "onnx")]
        EngineType::SenseVoice
        | EngineType::Parakeet
        | EngineType::GigaAM
        | EngineType::Moonshine
        | EngineType::Canary => {
            let factory = cortex_stt_server::engine::onnx_bridge::onnx_factory(
                model_path,
                engine_type.clone(),
                transcribe_rs::onnx::Quantization::Int8,
                cortex_stt_server::api::settings::ComputeDevice::default(),
            );
            engine_manager.register(model_id, factory).await;
        }
        _ => {
            return Err(format!(
                "Engine type {engine_type:?} not compiled in this build. \
                 Use --features whisper or --features onnx"
            )
            .into());
        }
    }

    Ok(())
}

async fn cmd_verify_urls() -> Result<(), Box<dyn std::error::Error>> {
    let models = builtin_models();
    let client = reqwest::Client::new();

    println!("Verifying {} model download URLs...\n", models.len());

    for model in &models {
        if model.url.is_empty() {
            println!("⏭  {}: no URL", model.id);
            continue;
        }

        if !validate_download_url(&model.url) {
            println!("✗  {}: URL rejected by whitelist: {}", model.id, model.url);
            continue;
        }

        print!(
            "   {}: HEAD {}... ",
            model.id,
            &model.url[..model.url.len().min(60)]
        );

        match client.head(&model.url).send().await {
            Ok(resp) => {
                let status = resp.status();
                let size = resp
                    .headers()
                    .get("content-length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());

                if status.is_success() || status.is_redirection() {
                    let size_str = size
                        .map(|s| format!("{} MB", s / (1024 * 1024)))
                        .unwrap_or_else(|| "unknown size".to_string());
                    println!("✓ {} ({})", status, size_str);
                } else {
                    println!("✗ {}", status);
                }
            }
            Err(e) => {
                println!("✗ {e}");
            }
        }
    }

    Ok(())
}

async fn cmd_download_all(manager: &ModelManager) -> Result<(), Box<dyn std::error::Error>> {
    let models = builtin_models();
    let mut ok = 0u32;
    let mut fail = 0u32;

    for def in &models {
        println!("--- {} ---", def.id);
        match cmd_download(&def.id, manager).await {
            Ok(_) => ok += 1,
            Err(e) => {
                println!("  ✗ FAILED: {e}");
                fail += 1;
            }
        }
        println!();
    }

    println!(
        "=== Download Summary: {ok} ok, {fail} failed, {} total ===",
        models.len()
    );
    if fail > 0 {
        Err(format!("{fail} downloads failed").into())
    } else {
        Ok(())
    }
}

/// Staged verification for each model:
///   Stage 1: File/directory exists on disk
///   Stage 2: Correct structure (ONNX: has model*.onnx; Whisper: .bin > 1MB)
///   Stage 3: Engine factory loads successfully
///   Stage 4: Can transcribe 1 second of silence
async fn cmd_verify(
    model_id_filter: Option<&str>,
    model_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let models = builtin_models();
    let targets: Vec<_> = match model_id_filter {
        Some(id) => models.iter().filter(|m| m.id == id).collect(),
        None => models.iter().collect(),
    };

    if targets.is_empty() {
        return Err("No matching models found".into());
    }

    let mut pass_count = 0usize;

    for def in &targets {
        let model_path = model_dir.join(&def.filename);
        print!("{:<25} ", def.id);

        // Stage 1: File exists
        if !model_path.exists() {
            println!("⏭ not downloaded");
            continue;
        }

        // Stage 2: Structure check
        let structure_ok = if def.is_directory {
            std::fs::read_dir(&model_path)
                .map(|entries| {
                    entries.filter_map(|e| e.ok()).any(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.contains("model") && name.ends_with(".onnx")
                    })
                })
                .unwrap_or(false)
        } else {
            std::fs::metadata(&model_path)
                .map(|m| m.len() > 1_000_000)
                .unwrap_or(false)
        };

        if !structure_ok {
            println!("✗ bad structure (missing model files)");
            continue;
        }

        // Stage 3: Engine loads
        let engine_manager = EngineManager::new(EngineManagerConfig {
            pool_size: 1,
            max_loaded_models: 1,
            idle_timeout: None,
            acquire_timeout: Duration::from_secs(300),
            idle_check_interval: Duration::from_secs(60),
        });

        if let Err(e) =
            register_engine(&engine_manager, &def.id, model_path, &def.engine_type).await
        {
            println!("✗ register failed: {e}");
            continue;
        }

        let guard = match engine_manager.acquire(&def.id).await {
            Ok(g) => g,
            Err(e) => {
                println!("✗ load failed: {e}");
                continue;
            }
        };
        drop(guard);

        // Stage 4: Transcribe silence
        let silence = vec![0.0f32; 16000];
        let opts = cortex_stt_server::engine::traits::TranscribeOptions::default();
        match engine_manager.acquire(&def.id).await {
            Ok(mut g) => match g.transcribe(&silence, &opts) {
                Ok(r) => {
                    println!("✓ ok (\"{}\")", &r.text[..r.text.len().min(40)]);
                    pass_count += 1;
                }
                Err(e) => println!("✗ transcribe failed: {e}"),
            },
            Err(e) => println!("✗ acquire failed: {e}"),
        }
    }

    println!("\n{pass_count}/{} models verified", targets.len());
    let downloaded = targets
        .iter()
        .filter(|d| model_dir.join(&d.filename).exists())
        .count();
    println!("{downloaded}/{} models downloaded", targets.len());

    Ok(())
}

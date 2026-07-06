//! CLI tool for testing cortex-stt model download and transcription pipeline.
//!
//! Usage:
//!   asr-cli list                              # List available models
//!   asr-cli download <model-id> [--quant Q]   # Download a model
//!   asr-cli transcribe <model-id> <wav-file>  # Transcribe an audio file
//!   asr-cli stream <model-id> <wav-file>      # Feed chunks through the engine stream
//!   asr-cli test <model-id> <wav-file>        # Download (if needed) + transcribe
//!   asr-cli test-all <wav-dir>                # Test all downloaded models
//!   asr-cli verify [model-id]                 # Verify downloaded models load + run

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::model::catalog::ModelCatalog;
use cortex_stt::model::catalog_data::{catalog_models, find_model};
use cortex_stt::model::download::{DownloadConfig, download_model, validate_download_url};
use cortex_stt::model::download_manager::DownloadManager;
use cortex_stt::model::types::ModelStatus;

#[derive(Parser)]
#[command(name = "asr-cli", about = "Test tool for cortex-stt models")]
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
    Download {
        model_id: String,
        /// Quant to install (default: the catalog's default_quant)
        #[arg(long, short)]
        quant: Option<String>,
    },

    /// Transcribe a WAV file with a specific model
    Transcribe {
        model_id: String,
        wav_file: PathBuf,
        /// Language hint (e.g., "zh", "en", "ja")
        #[arg(long, short)]
        language: Option<String>,
    },

    /// Feed a WAV file through the engine streaming path in real-time-ish
    /// chunks, printing partial transcripts (no HTTP involved)
    Stream {
        model_id: String,
        wav_file: PathBuf,
        #[arg(long, short)]
        language: Option<String>,
        /// Chunk size in milliseconds
        #[arg(long, default_value_t = 100)]
        chunk_ms: usize,
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

    /// Verify all default-quant download URLs are reachable (HEAD request)
    VerifyUrls,

    /// Verify each downloaded model: file exists → plausible size → can load → can transcribe
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

    let downloads = DownloadManager::new(cli.model_dir.clone());
    let catalog = ModelCatalog::new(cli.model_dir.clone(), downloads.clone());

    match cli.command {
        Command::List => cmd_list(&catalog).await,
        Command::Download { model_id, quant } => {
            cmd_download(&model_id, quant.as_deref(), &catalog, &downloads).await
        }
        Command::Transcribe {
            model_id,
            wav_file,
            language,
        } => cmd_transcribe(&model_id, &wav_file, language.as_deref(), &catalog).await,
        Command::Stream {
            model_id,
            wav_file,
            language,
            chunk_ms,
        } => {
            cmd_stream(
                &model_id,
                &wav_file,
                language.as_deref(),
                chunk_ms,
                &catalog,
            )
            .await
        }
        Command::Test {
            model_id,
            wav_file,
            language,
        } => {
            cmd_download_if_needed(&model_id, &catalog, &downloads).await?;
            cmd_transcribe(&model_id, &wav_file, language.as_deref(), &catalog).await
        }
        Command::TestAll { wav_dir } => cmd_test_all(&wav_dir, &catalog).await,
        Command::VerifyUrls => cmd_verify_urls().await,
        Command::Verify { model_id } => cmd_verify(model_id.as_deref(), &catalog).await,
    }
}

async fn cmd_list(catalog: &ModelCatalog) -> Result<(), Box<dyn std::error::Error>> {
    let models = catalog.list_models().await;

    println!(
        "{:<28} {:<12} {:<14} {:<8} {:<6} Languages",
        "ID", "Family", "Status", "Size", "Stream"
    );
    println!("{}", "-".repeat(96));

    for m in &models {
        let langs = if m.languages.len() > 5 {
            format!(
                "{} (+{} more)",
                m.languages[..5].join(","),
                m.languages.len() - 5
            )
        } else {
            m.languages.join(",")
        };

        let status = match m.status {
            ModelStatus::Downloaded => {
                format!("✓ {}", m.downloaded_quant.as_deref().unwrap_or("dl"))
            }
            ModelStatus::Available => "  available".to_string(),
            ModelStatus::Queued => "⏳ queued".to_string(),
            ModelStatus::Custom => "✓ custom".to_string(),
            ModelStatus::Downloading => "⟳ downloading".to_string(),
            ModelStatus::Error => "✗ error".to_string(),
        };

        println!(
            "{:<28} {:<12} {:<14} {:>5}MB {:<6} {}",
            m.id,
            m.family,
            status,
            m.size_mb,
            if m.capabilities.streaming { "yes" } else { "" },
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
    quant: Option<&str>,
    catalog: &ModelCatalog,
    downloads: &Arc<DownloadManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    let model = find_model(model_id).ok_or_else(|| format!("Model not in catalog: {model_id}"))?;
    let quant_file = match quant {
        Some(q) => model
            .quant(q)
            .ok_or_else(|| format!("Model {model_id} has no quant {q}"))?,
        None => model.default_quant_file(),
    };

    let dest_path = catalog.model_dir().join(&quant_file.filename);
    if dest_path.exists() {
        println!(
            "✓ Model '{model_id}' ({}) already downloaded at {}",
            quant_file.quant,
            dest_path.display()
        );
        return Ok(());
    }

    println!(
        "Downloading '{model_id}' {} ({} MB) from {}...",
        quant_file.quant,
        quant_file.size_bytes / (1024 * 1024),
        quant_file.url
    );

    // One-shot CLI: never cancels, so a fresh flag that stays false.
    let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let handle = download_model(
        &quant_file.url,
        dest_path.clone(),
        &quant_file.sha256,
        model_id,
        cancel_flag,
        downloads.clone(),
        DownloadConfig::default(),
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
    catalog: &ModelCatalog,
    downloads: &Arc<DownloadManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    let models = catalog.list_models().await;
    let model = models.iter().find(|m| m.id == model_id);

    match model {
        Some(m) if matches!(m.status, ModelStatus::Downloaded | ModelStatus::Custom) => {
            println!("✓ Model '{model_id}' already downloaded");
            Ok(())
        }
        Some(_) => cmd_download(model_id, None, catalog, downloads).await,
        None => Err(format!("Model not found: {model_id}").into()),
    }
}

/// Load a WAV file as 16 kHz mono f32 samples.
fn load_wav(wav_file: &Path) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    if !wav_file.exists() {
        return Err(format!("WAV file not found: {}", wav_file.display()).into());
    }
    let wav_data = std::fs::read(wav_file)?;
    Ok(cortex_stt::audio::resample::resample_to_16khz_mono(
        &wav_data,
    )?)
}

/// Register `model_id` in a one-shot EngineManager (single pool slot).
async fn setup_engine(
    model_id: &str,
    catalog: &ModelCatalog,
) -> Result<Arc<EngineManager>, Box<dyn std::error::Error>> {
    let model_path = catalog.model_path(model_id).ok_or_else(|| {
        format!("Model not found on disk: {model_id}. Run 'asr-cli download {model_id}' first.")
    })?;

    let engine_manager = EngineManager::new(EngineManagerConfig {
        pool_size: 1,
        max_loaded_models: 1,
        idle_timeout: None,
        acquire_timeout: Duration::from_secs(300),
        idle_check_interval: Duration::from_secs(60),
    });

    let factory = cortex_stt::engine::register::create_factory(model_id, model_path, None)
        .ok_or("Engine not compiled in this build — use the default `engine` feature")?;
    engine_manager.register(model_id, factory).await;
    Ok(engine_manager)
}

async fn cmd_transcribe(
    model_id: &str,
    wav_file: &Path,
    language: Option<&str>,
    catalog: &ModelCatalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let samples = load_wav(wav_file)?;
    let duration_secs = samples.len() as f32 / cortex_stt::audio::canonical::SAMPLE_RATE_F32;
    println!(
        "Audio: {:.2}s, {} samples (16kHz mono)",
        duration_secs,
        samples.len()
    );

    println!("Loading model '{model_id}'...");
    let load_start = Instant::now();
    let engine_manager = setup_engine(model_id, catalog).await?;

    let options = cortex_stt::engine::traits::TranscribeOptions {
        language: language.map(String::from),
        ..Default::default()
    };

    let mut guard = engine_manager.acquire(model_id).await?;
    let load_ms = load_start.elapsed().as_millis();

    let infer_start = Instant::now();
    let result = guard.transcribe(&samples, &options)?;
    let infer_ms = infer_start.elapsed().as_millis();
    drop(guard);

    // Output results
    println!("\n┌─────────────────────────────────────────────");
    println!("│ Model:      {model_id}");
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
    if result.truncated {
        println!("│ ⚠ output truncated");
    }
    println!("└─────────────────────────────────────────────");

    Ok(())
}

async fn cmd_stream(
    model_id: &str,
    wav_file: &Path,
    language: Option<&str>,
    chunk_ms: usize,
    catalog: &ModelCatalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let samples = load_wav(wav_file)?;
    let duration_secs = samples.len() as f32 / cortex_stt::audio::canonical::SAMPLE_RATE_F32;
    let chunk_samples =
        (cortex_stt::audio::canonical::SAMPLE_RATE as usize) * chunk_ms.max(10) / 1000;

    let engine_manager = setup_engine(model_id, catalog).await?;
    let mut guard = engine_manager.acquire(model_id).await?;

    let caps = guard.capabilities()?;
    if !caps.supports_streaming {
        return Err(format!(
            "Model {model_id} does not support streaming (use `asr-cli transcribe`)"
        )
        .into());
    }

    let options = cortex_stt::engine::traits::TranscribeOptions {
        language: language.map(String::from),
        ..Default::default()
    };

    println!("Streaming {duration_secs:.2}s of audio in {chunk_ms}ms chunks…\n");
    let start = Instant::now();
    guard.stream_begin(&options)?;

    let mut last_revision = -1;
    for chunk in samples.chunks(chunk_samples) {
        let snapshot = guard.stream_feed(chunk)?;
        if snapshot.revision != last_revision {
            last_revision = snapshot.revision;
            println!(
                "[{:>6.2}s r{:>3}] {}",
                start.elapsed().as_secs_f32(),
                snapshot.revision,
                snapshot.display
            );
        }
    }

    let result = guard.stream_finalize()?;
    println!("\n┌─────────────────────────────────────────────");
    println!(
        "│ Final ({:.2}s wall): {}",
        start.elapsed().as_secs_f32(),
        result.text
    );
    if result.truncated {
        println!("│ ⚠ output truncated");
    }
    println!("└─────────────────────────────────────────────");

    Ok(())
}

async fn cmd_test_all(
    wav_dir: &Path,
    catalog: &ModelCatalog,
) -> Result<(), Box<dyn std::error::Error>> {
    if !wav_dir.exists() {
        return Err(format!("WAV directory not found: {}", wav_dir.display()).into());
    }

    let models = catalog.list_models().await;
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
        let wav_file = find_test_audio(wav_dir, &model.languages);
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
        match cmd_transcribe(&model.id, &wav_file, lang.as_deref(), catalog).await {
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

async fn cmd_verify_urls() -> Result<(), Box<dyn std::error::Error>> {
    let models = catalog_models();
    let client = reqwest::Client::new();

    println!(
        "Verifying {} default-quant download URLs...\n",
        models.len()
    );

    for model in models {
        let quant = model.default_quant_file();
        if !validate_download_url(&quant.url) {
            println!("✗  {}: URL rejected by whitelist: {}", model.id, quant.url);
            continue;
        }

        print!(
            "   {}: HEAD {}... ",
            model.id,
            &quant.url[..quant.url.len().min(60)]
        );

        match client.head(&quant.url).send().await {
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

/// Staged verification for each downloaded model:
///   Stage 1: GGUF file exists on disk
///   Stage 2: Plausible size (> 1 MB)
///   Stage 3: Engine factory loads successfully
///   Stage 4: Can transcribe 1 second of silence
async fn cmd_verify(
    model_id_filter: Option<&str>,
    catalog: &ModelCatalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let models = catalog.list_models().await;
    let targets: Vec<_> = models
        .iter()
        .filter(|m| model_id_filter.is_none_or(|id| m.id == id))
        .collect();

    if targets.is_empty() {
        return Err("No matching models found".into());
    }

    let mut pass_count = 0usize;
    let mut downloaded = 0usize;

    for info in &targets {
        print!("{:<28} ", info.id);

        // Stage 1: File exists
        let Some(model_path) = catalog.model_path(&info.id) else {
            println!("⏭ not downloaded");
            continue;
        };
        downloaded += 1;

        // Stage 2: Plausible size
        let size_ok = std::fs::metadata(&model_path)
            .map(|m| m.len() > 1_000_000)
            .unwrap_or(false);
        if !size_ok {
            println!("✗ implausibly small file");
            continue;
        }

        // Stage 3 + 4: Engine loads and transcribes silence
        let engine_manager = match setup_engine(&info.id, catalog).await {
            Ok(m) => m,
            Err(e) => {
                println!("✗ register failed: {e}");
                continue;
            }
        };

        let silence = vec![0.0f32; cortex_stt::audio::canonical::SAMPLE_RATE as usize];
        let opts = cortex_stt::engine::traits::TranscribeOptions::default();
        match engine_manager.acquire(&info.id).await {
            Ok(mut g) => match g.transcribe(&silence, &opts) {
                Ok(r) => {
                    println!("✓ ok (\"{}\")", &r.text[..r.text.len().min(40)]);
                    pass_count += 1;
                }
                Err(e) => println!("✗ transcribe failed: {e}"),
            },
            Err(e) => println!("✗ load failed: {e}"),
        }
    }

    println!("\n{pass_count}/{downloaded} downloaded models verified");
    Ok(())
}

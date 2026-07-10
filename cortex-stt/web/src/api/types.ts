// --- System ---

export interface HealthResponse {
	status: "starting" | "ok";
	version: string;
	loaded_models: number;
}

export interface GpuEngines {
	whisper: boolean;
}

export interface GpuInfo {
	name: string;
	memory_total_mb: number;
	memory_used_mb: number;
	memory_free_mb: number;
	driver_version: string;
}

export interface SystemInfo {
	cpu_count: number;
	total_memory_mb: number;
	available_memory_mb: number;
	has_avx: boolean;
	has_avx2: boolean;
	cuda_available: boolean;
	gpu_info: GpuInfo | null;
	gpu_engines: GpuEngines;
	os: string;
	arch: string;
}

export interface Metrics {
	total_transcriptions: number;
	http_transcriptions: number;
	loaded_models: number;
	total_models: number;
	api_keys_count: number;
	today_transcriptions: number;
	total_audio_duration_ms: number;
	today_audio_duration_ms: number;
	avg_inference_ms: number;
	error_count: number;
	today_error_count: number;
	uptime_secs: number;
}

// --- Models ---

export type ModelStatus =
	| "available"
	| "queued"
	| "downloading"
	| "downloaded"
	| "custom"
	| "error";

export type ModelFamily =
	| "whisper"
	| "parakeet"
	| "sensevoice"
	| "canary"
	| "cohere"
	| "fun"
	| "gigaam"
	| "granite"
	| "medasr"
	| "moonshine"
	| "nemotron"
	| "qwen3"
	| "voxtral"
	| "custom";

export type TimestampGranularity = "none" | "segment" | "word";

export interface ModelCapabilities {
	streaming: boolean;
	translate: boolean;
	lang_detect: boolean;
	timestamps: TimestampGranularity;
}

/** One installable quantization of a model (GGUF). */
export interface QuantSummary {
	quant: string;
	size_mb: number;
}

export interface ModelInfo {
	id: string;
	name: string;
	description: string;
	family: ModelFamily;
	languages: string[];
	capabilities: ModelCapabilities;
	quants: QuantSummary[];
	default_quant: string;
	/** Installed quant, or null when not downloaded. */
	downloaded_quant: string | null;
	/** Size of the downloaded (or default) quant, in MB. */
	size_mb: number;
	recommended: boolean;
	recommended_rank: number | null;
	speed_score: number | null;
	accuracy_score: number | null;
	status: ModelStatus;
	disk_usage_bytes: number;
	is_loaded: boolean;
}

export interface DownloadProgress {
	model_id: string;
	downloaded_bytes: number;
	total_bytes: number;
	speed_bps: number;
	eta_secs: number | null;
	status: "queued" | "downloading" | "verifying" | "completed" | "failed";
	error: string | null;
}

// --- Storage ---

export interface StorageInfo {
	models_bytes: number;
	audio_bytes: number;
	database_bytes: number;
	free_bytes: number;
}

// --- Engine ---

export interface EngineStatus {
	loaded_models: string[];
	loaded_count: number;
}

// --- History ---

export type TranscriptionSource = "http_api" | "ws_api";

export interface TranscriptionRecord {
	id: string;
	timestamp: string;
	source: TranscriptionSource;
	language: string | null;
	model_id: string;
	audio_duration_ms: number;
	inference_ms: number;
	model_load_ms: number;
	pool_wait_ms: number;
	cold_load_ms: number;
	text: string;
	segments: TranscriptionSegment[];
	audio_path: string | null;
	has_error: boolean;
	error_message: string | null;
	api_key_id: string | null;
	device: string;
	/** Capture device (microphone/satellite) that recorded the audio. */
	capture_device: string | null;
	/** Input-signal RMS level in dBFS (null on failure/legacy rows). */
	rms_db: number | null;
	peak_db: number | null;
	clip_ratio: number | null;
}

export interface TranscriptionSegment {
	start: number;
	end: number;
	text: string;
}

export interface HistoryFacets {
	models: string[];
	capture_devices: string[];
}

export interface HistoryFilters {
	source?: TranscriptionSource;
	model?: string;
	text?: string;
	from?: string;
	to?: string;
	has_error?: boolean;
	capture_device?: string;
	limit?: number;
	offset?: number;
}

// --- API Keys ---

export interface ApiKey {
	id: string;
	name: string;
	key: string;
	last4: string;
	created_at: string;
	last_used_at: string | null;
	/** Addon-managed keys (e.g. Home Assistant discovery bootstrap) — read-only. */
	system: boolean;
}

export interface GeneratedKey {
	id: string;
	name: string;
	key: string;
	last4: string;
	created_at: string;
}

// --- Settings ---

export type BackendKind = "auto" | "cpu" | "cuda";

/** Per-model compute backend override. */
export interface BackendOverride {
	backend: BackendKind;
	gpu_device: number;
}

export type RetentionPolicyType = "Count" | "Days" | "DiskLimitMb" | "Unlimited";

export interface RetentionPolicy {
	type: RetentionPolicyType;
	value?: number;
}

export interface AppSettings {
	/** Explicit default-model choice; null = server falls back to its configured default. Written only via PUT /api/engine/default. */
	default_model: string | null;
	pool_size: number;
	max_loaded_models: number;
	idle_timeout_secs: number | null;
	transcription_timeout_secs: number | null;
	save_audio: boolean;
	preload_default_model: boolean;
	audio_retention: RetentionPolicy;
	record_retention: RetentionPolicy;
	timezone: string;
	backend_overrides: Record<string, BackendOverride>;
}

// --- Errors ---

export interface ApiErrorBody {
	code: string;
	message: string;
	model_id?: string;
}

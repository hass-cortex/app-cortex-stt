// --- System ---

export interface HealthResponse {
	status: "starting" | "ok" | "degraded";
	version: string;
	loaded_models: number;
}

export interface SystemInfo {
	cpu_count: number;
	total_memory_mb: number;
	available_memory_mb: number;
	has_avx: boolean;
	has_avx2: boolean;
	cuda_available: boolean;
	os: string;
	arch: string;
}

export interface Metrics {
	total_transcriptions: number;
	wyoming_transcriptions: number;
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
	| "not_downloaded"
	| "downloading"
	| "downloaded"
	| "loading"
	| "loaded"
	| "error";
export type EngineType = "Whisper" | "Parakeet" | "SenseVoice" | "GigaAM" | "Moonshine" | "Canary";

export interface ModelInfo {
	id: string;
	name: string;
	description: string;
	engine_type: EngineType;
	filename: string;
	is_directory: boolean;
	size_mb: number;
	accuracy_score: number;
	speed_score: number;
	supported_languages: string[];
	requires_cuda: boolean;
	requires_avx: boolean;
	is_recommended: boolean;
	status: ModelStatus;
	disk_usage_bytes: number | null;
	is_loaded: boolean;
}

export interface DownloadProgress {
	model_id: string;
	downloaded_bytes: number;
	total_bytes: number;
	speed_bps: number;
	eta_secs: number | null;
	status: "downloading" | "verifying" | "extracting" | "completed" | "failed";
	error: string | null;
}

// --- Engine ---

export interface EngineStatus {
	loaded_models: string[];
	loaded_count: number;
}

// --- History ---

export type TranscriptionSource = "wyoming" | "http_api";

export interface TranscriptionRecord {
	id: string;
	timestamp: string;
	source: TranscriptionSource;
	language: string | null;
	model_id: string;
	audio_duration_ms: number;
	inference_ms: number;
	text: string;
	segments_json: string | null;
	audio_path: string | null;
	has_error: boolean;
	error_message: string | null;
}

export interface TranscriptionSegment {
	start: number;
	end: number;
	text: string;
}

export interface HistoryFilters {
	source?: TranscriptionSource;
	model?: string;
	from?: string;
	to?: string;
	has_error?: boolean;
	limit?: number;
	offset?: number;
}

// --- API Keys ---

export interface ApiKey {
	id: string;
	name: string;
	last4: string;
	created_at: string;
	last_used_at: string | null;
}

export interface GeneratedKey {
	id: string;
	name: string;
	key: string;
	last4: string;
	created_at: string;
}

// --- Settings ---

export type RetentionPolicyType = "Count" | "Days" | "DiskLimitMb" | "Unlimited";

export interface RetentionPolicy {
	type: RetentionPolicyType;
	value?: number;
}

export interface AppSettings {
	default_model: string;
	pool_size: number;
	max_loaded_models: number;
	idle_timeout_secs: number;
	transcription_timeout_secs: number;
	save_audio: boolean;
	audio_retention: RetentionPolicy;
	record_retention: RetentionPolicy;
	cors_allowed_origins: string[];
	log_level: string;
}

// --- Errors ---

export interface ApiError {
	error: {
		code: string;
		message: string;
		model_id?: string;
	};
}

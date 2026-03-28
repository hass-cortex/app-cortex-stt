// --- System ---

export interface HealthResponse {
	status: "starting" | "ok" | "degraded";
	version: string;
	default_model: string;
}

export interface SystemInfo {
	cpu: string;
	cpu_cores: number;
	ram_total_mb: number;
	ram_available_mb: number;
	gpu: string | null;
	gpu_memory_mb: number | null;
	has_avx: boolean;
	has_cuda: boolean;
	cuda_version: string | null;
	os: string;
	arch: string;
}

export interface Metrics {
	total_transcriptions: number;
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
	url: string;
	sha256: string;
	size_mb: number;
	accuracy_score: number;
	speed_score: number;
	supported_languages: string[];
	requires_cuda: boolean;
	requires_avx: boolean;
	is_recommended: boolean;
	is_custom: boolean;
	status: ModelStatus;
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

export interface PoolStatus {
	model_id: string;
	pool_size: number;
	available: number;
	busy: number;
	last_used: string | null;
}

export interface EngineStatus {
	default_model: string;
	loaded_pools: PoolStatus[];
	max_loaded_models: number;
	queue_depth: number;
}

// --- History ---

export type TranscriptionSource = "Wyoming" | "HttpApi";

export interface TranscriptionRecord {
	id: string;
	timestamp: string;
	source: TranscriptionSource;
	language: string | null;
	model_id: string;
	audio_duration_ms: number;
	inference_ms: number;
	text: string;
	has_audio: boolean;
	has_error: boolean;
	error_message: string | null;
}

export interface TranscriptionDetail extends TranscriptionRecord {
	segments: TranscriptionSegment[];
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

export interface PaginatedResponse<T> {
	items: T[];
	total: number;
	limit: number;
	offset: number;
}

// --- API Keys ---

export interface ApiKey {
	id: string;
	name: string;
	last4: string;
	created_at: string;
	last_used_at: string | null;
	request_count: number;
}

export interface GeneratedKey {
	id: string;
	name: string;
	key: string;
}

// --- Settings ---

export type RetentionPolicyType = "count" | "days" | "disk_limit" | "unlimited";

export interface RetentionPolicy {
	type: RetentionPolicyType;
	value?: number;
}

export interface AppSettings {
	save_audio: boolean;
	audio_retention: RetentionPolicy;
	record_retention: RetentionPolicy;
	log_level: string;
	cors_origins: string[];
	rate_limit_enabled: boolean;
	rate_limit_per_minute: number;
	transcription_timeout_secs: number;
	model_load_timeout_secs: number;
	pool_acquire_timeout_secs: number;
}

// --- Errors ---

export interface ApiError {
	error: {
		code: string;
		message: string;
		model_id?: string;
	};
}

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
	/** What the model produced, when it differs from `text` — family
	 *  post-processing stripped tags from it, or an output rendering
	 *  rewrote it. Null when nothing changed. */
	raw_text: string | null;
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

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/** Audio copied into the evaluation store that has no reference yet. */
export interface PendingCapture {
	id: string;
	created_at: string;
	audio_path: string;
	audio_duration_ms: number;
	origin_record_id: string | null;
	capture_device: string | null;
	/** What a model transcribed at capture time — a starting point, not truth. */
	origin_text: string | null;
}

/** Lossless audio plus the hand-typed truth about what it says. */
export interface EvalSample {
	id: string;
	created_at: string;
	audio_path: string;
	audio_duration_ms: number;
	reference_transcript: string;
	/** Orthography the reference is written in (BCP-47). Null when
	 *  unlabelled — a run rendering to a different locale is flagged
	 *  rather than scored against it. */
	reference_locale: string | null;
	origin_record_id: string | null;
	capture_device: string | null;
}

/** A sample as the list shows it, with what deleting it would destroy —
 *  the cascade reaches every run that used it. */
export interface EvalSampleListEntry extends EvalSample {
	run_count: number;
	result_count: number;
}

export type RunStatus = "running" | "completed" | "cancelled" | "failed";

export interface EvalRun {
	id: string;
	started_at: string;
	finished_at: string | null;
	status: RunStatus;
	app_version: string;
	engine_version: string;
	max_loaded_models: number;
	process_floor_bytes: number;
	available_memory_bytes: number;
	/** The language hint this run used; part of its configuration. */
	language: string | null;
	note: string | null;
}

/** A run as the list shows it — the row plus enough of its shape to tell
 *  one run from another. Model ids are in the run's own order. */
export interface EvalRunListEntry extends EvalRun {
	model_ids: string[];
	sample_count: number;
	/** Verdicts summed across every model in the run. */
	correct: number;
	scored: number;
	/** Correct cells nobody checked. */
	unchecked_correct: number;
}

export interface EvalResult {
	id: string;
	run_id: string;
	sample_id: string;
	model_id: string;
	text: string;
	raw_text: string | null;
	inference_ms: number;
	error_message: string | null;
	/** What the server's comparison alone makes of this output. */
	matches_reference: boolean;
	/** A person's ruling on this exact output string, where one exists.
	 *  It outranks the comparison, in both directions. */
	ruling: boolean | null;
}

export interface Judgement {
	sample_id: string;
	output_text: string;
	correct: boolean;
	created_at: string;
}

export interface ModelSummary {
	model_id: string;
	quant: string | null;
	file_size_bytes: number;
	cold_load_ms: number;
	load_failed: boolean;
	failure_reason: string | null;
	/** What the model costs on its own. */
	resident_bytes: number;
	/** What it needs with the rest of the working set resident. */
	coexist_bytes: number;
	fits_alongside: boolean | null;
	results: number;
	/** Cells whose verdict is correct — a person's ruling where there is
	 *  one, the comparison against the reference otherwise. */
	correct: number;
	/** Cells carrying a verdict at all: everything but an error. */
	scored: number;
	/** Correct cells nobody checked — the unverified verdicts that could
	 *  be inflating this figure. An unchecked wrong costs the model a
	 *  point rather than granting one, so it is not counted here. */
	unchecked_correct: number;
	median_inference_ms: number | null;
	max_inference_ms: number | null;
	median_rtf: number | null;
	changed_from_previous: number | null;
}

export interface RunSummary {
	run: EvalRun;
	sample_count: number;
	models: ModelSummary[];
	previous_run_id: string | null;
	/** Samples both runs covered — the only valid basis for comparison. */
	compared_samples: number;
	/** Completed runs skipped as a baseline for using a different
	 *  language hint, so an absent comparison can be explained. */
	skipped_baselines: number;
}

export interface DeviceCount {
	capture_device: string;
	count: number;
}

export interface SampleSetComposition {
	total: number;
	total_duration_ms: number;
	judged_outputs: number;
	by_capture_device: DeviceCount[];
}

export interface EvalOverview {
	latest: RunSummary | null;
	composition: SampleSetComposition;
	pending_count: number;
	coverage: DeviceCount[];
}

export interface ModelRunEntry {
	run: EvalRun;
	sample_count: number;
	compared_samples: number;
	summary: ModelSummary;
}

export interface RunProgress {
	run_id: string;
	status: RunStatus;
	model_index: number;
	model_total: number;
	current_model: string | null;
	sample_index: number;
	sample_total: number;
}

export interface EvalResultFilters {
	run_id?: string;
	sample_id?: string;
	model?: string;
	text?: string;
	capture_device?: string;
	limit?: number;
	offset?: number;
}

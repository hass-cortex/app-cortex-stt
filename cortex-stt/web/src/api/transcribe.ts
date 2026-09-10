import { postBinary } from "./client";
import type { TranscriptionSegment } from "./types";

/** Sync response from `POST /api/transcribe`. */
export interface TranscribeResponse {
	text: string;
	/** Present only when the model said something the transcript does not. */
	raw_text?: string;
	language?: string;
	segments: TranscriptionSegment[];
	words?: TranscriptionSegment[];
	truncated: boolean;
	model: string;
	duration_ms: number;
	inference_ms: number;
	model_load_ms: number;
	pool_wait_ms: number;
	cold_load_ms: number;
	device: string;
}

export interface TranscribeParams {
	model: string;
	/** BCP-47. The base subtag is the engine hint; the rest selects an
	 *  output rendering (ADR 0006). */
	language?: string;
	captureDevice?: string;
}

/** Canonical WAV in, transcript out. The caller encodes — the server
 *  decodes WAV and raw PCM only.
 *
 *  `timestamps` is deliberately left off: the server's `auto` picks the
 *  finest granularity the model actually supports, while naming one
 *  explicitly fails outright on families that cannot produce it. */
export function transcribeClip(
	wav: Blob,
	{ model, language, captureDevice }: TranscribeParams,
): Promise<TranscribeResponse> {
	const params: Record<string, string> = { model };
	if (language) params.language = language;
	if (captureDevice) params.capture_device = captureDevice;
	return postBinary<TranscribeResponse>("/api/transcribe", wav, "audio/wav", params);
}

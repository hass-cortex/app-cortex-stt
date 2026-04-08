import { del, get } from "./client";
import type { HistoryFilters, TranscriptionRecord } from "./types";

/** List transcription records with filters (returns flat array) */
export function listHistory(filters?: HistoryFilters): Promise<TranscriptionRecord[]> {
	const params: Record<string, string> = {};
	if (filters?.source) params.source = filters.source;
	if (filters?.model) params.model = filters.model;
	if (filters?.from) params.from = filters.from;
	if (filters?.to) params.to = filters.to;
	if (filters?.has_error !== undefined) params.has_error = String(filters.has_error);
	if (filters?.limit !== undefined) params.limit = String(filters.limit);
	if (filters?.offset !== undefined) params.offset = String(filters.offset);
	return get<TranscriptionRecord[]>("/api/history", params);
}

/** Get a single transcription record */
export function getHistoryDetail(id: string): Promise<TranscriptionRecord> {
	return get<TranscriptionRecord>(`/api/history/${encodeURIComponent(id)}`);
}

/** Delete a single transcription record */
export function deleteHistoryRecord(id: string): Promise<void> {
	return del(`/api/history/${encodeURIComponent(id)}`);
}

/** Delete all transcription records and audio files */
export function deleteAllHistory(): Promise<{ deleted_records: number; deleted_audio_files: number }> {
	return del<{ deleted_records: number; deleted_audio_files: number }>("/api/history");
}


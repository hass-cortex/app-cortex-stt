import { del, get } from "./client";
import type { HistoryFacets, HistoryFilters, TranscriptionRecord } from "./types";

/** List transcription records with filters (returns flat array) */
export function listHistory(filters?: HistoryFilters): Promise<TranscriptionRecord[]> {
	const params: Record<string, string> = {};
	if (filters?.source) params.source = filters.source;
	if (filters?.model) params.model = filters.model;
	if (filters?.text) params.text = filters.text;
	if (filters?.from) params.from = filters.from;
	if (filters?.to) params.to = filters.to;
	if (filters?.has_error !== undefined) params.has_error = String(filters.has_error);
	if (filters?.capture_device) params.capture_device = filters.capture_device;
	if (filters?.limit !== undefined) params.limit = String(filters.limit);
	if (filters?.offset !== undefined) params.offset = String(filters.offset);
	return get<TranscriptionRecord[]>("/api/history", params);
}

/** Distinct models + capture devices for the filter dropdowns */
export function getHistoryFacets(): Promise<HistoryFacets> {
	return get<HistoryFacets>("/api/history/facets");
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
export function deleteAllHistory(): Promise<{
	deleted_records: number;
	deleted_audio_files: number;
}> {
	return del<{ deleted_records: number; deleted_audio_files: number }>("/api/history");
}

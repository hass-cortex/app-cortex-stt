import { del, get, post } from "./client";
import type {
	HistoryFilters,
	PaginatedResponse,
	TranscriptionDetail,
	TranscriptionRecord,
} from "./types";

/** List transcription records with filters */
export function listHistory(
	filters?: HistoryFilters,
): Promise<PaginatedResponse<TranscriptionRecord>> {
	const params: Record<string, string> = {};
	if (filters?.source) params.source = filters.source;
	if (filters?.model) params.model = filters.model;
	if (filters?.from) params.from = filters.from;
	if (filters?.to) params.to = filters.to;
	if (filters?.has_error !== undefined) params.has_error = String(filters.has_error);
	if (filters?.limit !== undefined) params.limit = String(filters.limit);
	if (filters?.offset !== undefined) params.offset = String(filters.offset);
	return get<PaginatedResponse<TranscriptionRecord>>("/api/history", params);
}

/** Get a single transcription record with segments */
export function getHistoryDetail(id: string): Promise<TranscriptionDetail> {
	return get<TranscriptionDetail>(`/api/history/${encodeURIComponent(id)}`);
}

/** Delete a single transcription record */
export function deleteHistoryRecord(id: string): Promise<void> {
	return del(`/api/history/${encodeURIComponent(id)}`);
}

/** Run manual retention cleanup */
export function cleanupHistory(): Promise<{ deleted_records: number; deleted_audio: number }> {
	return post<{ deleted_records: number; deleted_audio: number }>("/api/history/cleanup");
}

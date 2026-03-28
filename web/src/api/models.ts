import { del, get, post } from "./client";
import type { ModelInfo } from "./types";

/** List all models (registry + custom + status) */
export function listModels(): Promise<ModelInfo[]> {
	return get<ModelInfo[]>("/api/models");
}

/** Start downloading a model */
export function downloadModel(modelId: string): Promise<{ status: string }> {
	return post<{ status: string }>(`/api/models/${encodeURIComponent(modelId)}/download`);
}

/** Cancel an in-progress download */
export function cancelDownload(modelId: string): Promise<void> {
	return del(`/api/models/${encodeURIComponent(modelId)}/download`);
}

/** Delete a downloaded model */
export function deleteModel(modelId: string): Promise<void> {
	return del(`/api/models/${encodeURIComponent(modelId)}`);
}

/** Rescan custom model directory */
export function scanCustomModels(): Promise<{ count: number }> {
	return post<{ count: number }>("/api/models/scan");
}

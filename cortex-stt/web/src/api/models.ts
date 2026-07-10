import { del, get, post } from "./client";
import type { ModelInfo } from "./types";

/** List all models (registry + custom + status) */
export function listModels(): Promise<ModelInfo[]> {
	return get<ModelInfo[]>("/api/models");
}

/** Start downloading a model. Omitting `quant` installs the model's default quant. */
export function downloadModel(modelId: string, quant?: string): Promise<{ status: string }> {
	const query = quant ? `?quant=${encodeURIComponent(quant)}` : "";
	return post<{ status: string }>(`/api/models/${encodeURIComponent(modelId)}/download${query}`);
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

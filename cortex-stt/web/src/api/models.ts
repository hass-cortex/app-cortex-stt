import { del, get, post } from "./client";
import type { ModelInfo } from "./types";

function quantQuery(quant?: string | null): string {
	return quant ? `?quant=${encodeURIComponent(quant)}` : "";
}

function modelPath(modelId: string): string {
	return `/api/models/${encodeURIComponent(modelId)}`;
}

/** List all models (catalog + retired + custom, each with its Variants) */
export function listModels(): Promise<ModelInfo[]> {
	return get<ModelInfo[]>("/api/models");
}

/** Download a quant of a model: adds that Variant (or updates it when
 *  already installed). Omitting `quant` installs the model's default quant. */
export function downloadModel(modelId: string, quant?: string): Promise<{ status: string }> {
	return post<{ status: string }>(`${modelPath(modelId)}/download${quantQuery(quant)}`);
}

/** Cancel a Variant's download, or dismiss its kept failure */
export function cancelDownload(modelId: string, quant: string | null): Promise<void> {
	return del(`${modelPath(modelId)}/download${quantQuery(quant)}`);
}

/** Uninstall one Variant, or every Variant of the model when `quant` is omitted */
export function deleteModel(modelId: string, quant?: string | null): Promise<void> {
	return del(`${modelPath(modelId)}${quantQuery(quant)}`);
}

/** Rescan custom model directory */
export function scanCustomModels(): Promise<{ count: number }> {
	return post<{ count: number }>("/api/models/scan");
}

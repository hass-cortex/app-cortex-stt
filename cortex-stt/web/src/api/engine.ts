import { get, post, put } from "./client";
import type { EngineStatus } from "./types";

/** Get current engine status */
export function getEngineStatus(): Promise<EngineStatus> {
	return get<EngineStatus>("/api/engine");
}

/** Set the default model */
export function setDefaultModel(modelId: string): Promise<void> {
	return put("/api/engine/default", { model_id: modelId });
}

/** Pre-load a Variant into the engine pool (the Preferred one when `quant` is omitted) */
export function loadModel(modelId: string, quant?: string | null): Promise<void> {
	return post("/api/engine/load", { model_id: modelId, quant: quant ?? null });
}

/** Unload a Variant from the engine pool */
export function unloadModel(modelId: string, quant?: string | null): Promise<void> {
	return post("/api/engine/unload", { model_id: modelId, quant: quant ?? null });
}

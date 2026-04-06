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

/** Pre-load a model into the engine pool */
export function loadModel(modelId: string, poolSize?: number): Promise<void> {
	return post("/api/engine/load", { model_id: modelId, pool_size: poolSize });
}

/** Unload a model from the engine pool */
export function unloadModel(modelId: string): Promise<void> {
	return post("/api/engine/unload", { model_id: modelId });
}

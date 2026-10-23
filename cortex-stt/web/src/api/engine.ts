import { get, post } from "./client";
import type { EngineStatus } from "./types";

/** Get current engine status */
export function getEngineStatus(): Promise<EngineStatus> {
	return get<EngineStatus>("/api/engine");
}

/** Load a Variant into the engine pool, by its key (`model:quant`) */
export function loadModel(key: string): Promise<void> {
	return post("/api/engine/load", { model: key });
}

/** Unload a Variant from the engine pool, by its key */
export function unloadModel(key: string): Promise<void> {
	return post("/api/engine/unload", { model: key });
}

import { get } from "./client";
import type { HealthResponse, Metrics, SystemInfo } from "./types";

/** Health check (no auth required) */
export function getHealth(): Promise<HealthResponse> {
	return get<HealthResponse>("/health");
}

/** Get hardware information */
export function getSystemInfo(): Promise<SystemInfo> {
	return get<SystemInfo>("/api/system");
}

/** Get transcription metrics */
export function getMetrics(): Promise<Metrics> {
	return get<Metrics>("/api/metrics");
}

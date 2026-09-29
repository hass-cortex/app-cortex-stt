import { getBrowserTimezone } from "@/utils/time";
import { get } from "./client";
import type { HealthResponse, Metrics, StorageInfo, SystemInfo } from "./types";

/** Health check (no auth required) */
export function getHealth(): Promise<HealthResponse> {
	return get<HealthResponse>("/health");
}

/** Get hardware information */
export function getSystemInfo(): Promise<SystemInfo> {
	return get<SystemInfo>("/api/system");
}

/** Get transcription metrics; the browser zone defines "today" when the timezone setting is "auto" */
export function getMetrics(): Promise<Metrics> {
	return get<Metrics>("/api/metrics", { tz: getBrowserTimezone() });
}

/** Get storage usage info */
export function getStorageInfo(): Promise<StorageInfo> {
	return get<StorageInfo>("/api/storage");
}

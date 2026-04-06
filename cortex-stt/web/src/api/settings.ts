import { get, put } from "./client";
import type { AppSettings } from "./types";

/** Get current settings */
export function getSettings(): Promise<AppSettings> {
	return get<AppSettings>("/api/settings");
}

/** Update settings */
export function updateSettings(settings: Partial<AppSettings>): Promise<AppSettings> {
	return put<AppSettings>("/api/settings", settings);
}

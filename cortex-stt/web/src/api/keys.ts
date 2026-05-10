import { del, get, post } from "./client";
import type { ApiKey, GeneratedKey } from "./types";

/** List all API keys (without the actual key values) */
export function listKeys(): Promise<ApiKey[]> {
	return get<ApiKey[]>("/api/keys");
}

/** Generate a new API key */
export function generateKey(name: string): Promise<GeneratedKey> {
	return post<GeneratedKey>("/api/keys", { name });
}

/** Revoke an API key */
export function revokeKey(id: string): Promise<void> {
	return del(`/api/keys/${encodeURIComponent(id)}`);
}

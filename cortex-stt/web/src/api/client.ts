import type { ApiErrorBody } from "./types";

const API_KEY_STORAGE_KEY = "cortex-stt-api-key";

export class ApiClientError extends Error {
	constructor(
		public readonly status: number,
		public readonly code: string,
		message: string,
	) {
		super(message);
		this.name = "ApiClientError";
	}
}

function getApiKey(): string | null {
	try {
		return localStorage.getItem(API_KEY_STORAGE_KEY);
	} catch {
		return null;
	}
}

export function setApiKey(key: string | null): void {
	try {
		if (key) {
			localStorage.setItem(API_KEY_STORAGE_KEY, key);
		} else {
			localStorage.removeItem(API_KEY_STORAGE_KEY);
		}
	} catch {
		// Ignore
	}
}

/** Determine the base URL. Uses ingress path when served via HA ingress. */
function getBaseUrl(): string {
	return (window as unknown as { __INGRESS_PATH__?: string }).__INGRESS_PATH__ || "";
}

async function handleResponse<T>(response: Response): Promise<T> {
	if (!response.ok) {
		let code = "UNKNOWN";
		let message = `HTTP ${response.status}`;
		try {
			const body: ApiErrorBody = await response.json();
			code = body.code;
			message = body.message;
		} catch {
			// Non-JSON error body
		}
		throw new ApiClientError(response.status, code, message);
	}
	return response.json();
}

function buildHeaders(): HeadersInit {
	const headers: Record<string, string> = {
		"Content-Type": "application/json",
	};
	const key = getApiKey();
	if (key) {
		headers.Authorization = `Bearer ${key}`;
	}
	return headers;
}

/** Typed GET request */
export async function get<T>(path: string, params?: Record<string, string>): Promise<T> {
	const url = new URL(`${getBaseUrl()}${path}`, window.location.origin);
	if (params) {
		for (const [k, v] of Object.entries(params)) {
			if (v !== undefined && v !== "") url.searchParams.set(k, v);
		}
	}
	const response = await fetch(url.toString(), {
		method: "GET",
		headers: buildHeaders(),
	});
	return handleResponse<T>(response);
}

/** Typed POST request */
export async function post<T>(path: string, body?: unknown): Promise<T> {
	const response = await fetch(`${getBaseUrl()}${path}`, {
		method: "POST",
		headers: buildHeaders(),
		body: body !== undefined ? JSON.stringify(body) : undefined,
	});
	return handleResponse<T>(response);
}

/** Typed PUT request */
export async function put<T>(path: string, body: unknown): Promise<T> {
	const response = await fetch(`${getBaseUrl()}${path}`, {
		method: "PUT",
		headers: buildHeaders(),
		body: JSON.stringify(body),
	});
	return handleResponse<T>(response);
}

/** Typed DELETE request */
export async function del<T = void>(path: string): Promise<T> {
	const response = await fetch(`${getBaseUrl()}${path}`, {
		method: "DELETE",
		headers: buildHeaders(),
	});
	if (response.status === 204) return undefined as T;
	return handleResponse<T>(response);
}

/** Subscribe to SSE stream. Returns a cleanup function.
 *  When `eventName` is provided, listens for named events via `addEventListener`;
 *  otherwise listens for unnamed events via `onmessage`. */
export function subscribeSSE(
	path: string,
	onMessage: (data: unknown) => void,
	onError?: (error: Event) => void,
	eventName?: string,
): () => void {
	const base = `${getBaseUrl()}${path}`;
	const key = getApiKey();
	const url = key
		? `${base}${base.includes("?") ? "&" : "?"}api_key=${encodeURIComponent(key)}`
		: base;
	const eventSource = new EventSource(url);

	const handler = (event: MessageEvent) => {
		try {
			const data = JSON.parse(event.data);
			onMessage(data);
		} catch {
			// Ignore unparseable messages
		}
	};

	if (eventName) {
		eventSource.addEventListener(eventName, handler);
	} else {
		eventSource.onmessage = handler;
	}

	eventSource.onerror = (event) => {
		onError?.(event);
	};

	return () => eventSource.close();
}

/** Build audio URL for playback (with auth query param if needed) */
export function audioUrl(recordId: string): string {
	const key = getApiKey();
	const base = `${getBaseUrl()}/api/history/${recordId}/audio`;
	return key ? `${base}?api_key=${encodeURIComponent(key)}` : base;
}

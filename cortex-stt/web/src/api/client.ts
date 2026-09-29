import { ingressBasePath } from "@/lib/ingress";
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
	return ingressBasePath();
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
	// A write that reports success by saying nothing. Handing an empty body
	// to json() throws — after the write has already landed, so the caller
	// sees a failure for something that worked and never refetches.
	if (response.status === 204 || response.headers.get("content-length") === "0") {
		return undefined as T;
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
	return handleResponse<T>(response);
}

/** Typed DELETE carrying a JSON body. For resources whose identity is a
 *  composite value (an output string, say) that has no place in a path. */
export async function delWithBody<T = void>(path: string, body: unknown): Promise<T> {
	const response = await fetch(`${getBaseUrl()}${path}`, {
		method: "DELETE",
		headers: buildHeaders(),
		body: JSON.stringify(body),
	});
	return handleResponse<T>(response);
}

/** POST raw bytes (an audio file) rather than JSON. */
export async function postBinary<T>(
	path: string,
	body: ArrayBuffer | Blob,
	contentType: string,
	params?: Record<string, string>,
): Promise<T> {
	const url = new URL(`${getBaseUrl()}${path}`, window.location.origin);
	if (params) {
		for (const [k, v] of Object.entries(params)) {
			if (v !== undefined && v !== "") url.searchParams.set(k, v);
		}
	}
	const headers: Record<string, string> = { "Content-Type": contentType };
	const key = getApiKey();
	if (key) headers.Authorization = `Bearer ${key}`;
	const response = await fetch(url.toString(), { method: "POST", headers, body });
	return handleResponse<T>(response);
}

/** One EventSource per URL, shared by every subscriber to it. Browsers
 *  allow six HTTP/1.1 connections per origin (HA ingress included); one
 *  EventSource per hook call filled them, and a click's fetch queued
 *  behind the streams until one closed. */
interface SharedStream {
	source: EventSource;
	refs: number;
	/** Last payload per event type, replayed to late subscribers: a stream
	 *  that sends its current state on connect has already sent it. */
	last: Map<string, unknown>;
}

const streams = new Map<string, SharedStream>();

function openStream(url: string): SharedStream {
	const existing = streams.get(url);
	if (existing && existing.source.readyState !== EventSource.CLOSED) return existing;
	const stream: SharedStream = { source: new EventSource(url), refs: 0, last: new Map() };
	streams.set(url, stream);
	return stream;
}

/** Subscribe to SSE stream. Returns a cleanup function.
 *  When `eventName` is provided, listens for that named event; otherwise
 *  for unnamed events. */
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
	const stream = openStream(url);
	stream.refs++;
	const type = eventName ?? "message";

	const handler = (event: MessageEvent) => {
		let data: unknown;
		try {
			data = JSON.parse(event.data);
		} catch {
			return; // Ignore unparseable messages
		}
		stream.last.set(type, data);
		onMessage(data);
	};
	const errorHandler = (event: Event) => onError?.(event);

	stream.source.addEventListener(type, handler as EventListener);
	stream.source.addEventListener("error", errorHandler);
	let closed = false;
	if (stream.last.has(type)) {
		const replay = stream.last.get(type);
		queueMicrotask(() => {
			if (!closed) onMessage(replay);
		});
	}

	return () => {
		if (closed) return;
		closed = true;
		stream.source.removeEventListener(type, handler as EventListener);
		stream.source.removeEventListener("error", errorHandler);
		stream.refs--;
		if (stream.refs === 0) {
			stream.source.close();
			if (streams.get(url) === stream) streams.delete(url);
		}
	};
}

/** Build audio URL for playback (with auth query param if needed) */
export function audioUrl(recordId: string): string {
	return withKey(`${getBaseUrl()}/api/history/${recordId}/audio`);
}

/** Audio for an evaluation sample or pending capture (always WAV). */
export function evalAudioUrl(id: string): string {
	return withKey(`${getBaseUrl()}/api/eval/samples/${encodeURIComponent(id)}/audio`);
}

/** Append the key for URLs consumed by elements that cannot set headers. */
function withKey(base: string): string {
	const key = getApiKey();
	return key ? `${base}?api_key=${encodeURIComponent(key)}` : base;
}

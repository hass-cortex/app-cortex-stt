/**
 * HA ingress awareness — the single place that reads the
 * `window.__INGRESS_PATH__` global injected by the server into
 * `index.html` (see `src/main.rs` index injection).
 */

/** Base path for router + API calls; empty when served directly. */
export function ingressBasePath(): string {
	return (window as unknown as { __INGRESS_PATH__?: string }).__INGRESS_PATH__ || "";
}

/** Served through HA ingress — HA already authenticated the user. */
export function isIngress(): boolean {
	return ingressBasePath() !== "";
}

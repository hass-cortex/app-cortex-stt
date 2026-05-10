/** Format bytes to human readable (e.g., "1.5 GB") */
export function formatBytes(bytes: number): string {
	if (bytes === 0) return "0 B";
	const units = ["B", "KB", "MB", "GB", "TB"];
	const i = Math.floor(Math.log(bytes) / Math.log(1024));
	const value = bytes / 1024 ** i;
	return `${value.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

/** Format megabytes to human readable */
export function formatMB(mb: number): string {
	return formatBytes(mb * 1024 * 1024);
}

/** Format milliseconds to human readable duration */
export function formatDuration(ms: number): string {
	if (ms == null) return "—";
	if (ms < 1000) return `${Math.round(ms)}ms`;
	if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
	const minutes = Math.floor(ms / 60_000);
	const seconds = Math.round((ms % 60_000) / 1000);
	return `${minutes}m ${seconds}s`;
}

/** Format seconds to mm:ss for audio player */
export function formatAudioTime(seconds: number): string {
	const m = Math.floor(seconds / 60);
	const s = Math.floor(seconds % 60);
	return `${m}:${s.toString().padStart(2, "0")}`;
}

/** Format relative time (e.g., "2 minutes ago") */
export function formatRelativeTime(iso: string): string {
	const diff = Date.now() - new Date(iso).getTime();
	const seconds = Math.floor(diff / 1000);

	if (seconds < 60) return "just now";
	if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
	if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
	return `${Math.floor(seconds / 86400)}d ago`;
}

/** Format a score (0-1) as percentage */
export function formatScore(score: number): string {
	return `${Math.round(score * 100)}%`;
}

/** Format a number with locale-aware thousands separators */
export function formatNumber(n: number): string {
	if (n == null) return "—";
	return n.toLocaleString();
}

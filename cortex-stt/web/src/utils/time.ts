/**
 * Parse a backend timestamp into a `Date` in UTC.
 *
 * Backend timestamps arrive in one of two shapes:
 *   1. SQLite `datetime('now')` → `"YYYY-MM-DD HH:MM:SS"` — UTC but no `Z`
 *   2. Chrono `Utc::now()` via serde_json → RFC3339 with `Z`
 *
 * `new Date(s)` would interpret form (1) as *local time*, drifting by the
 * browser's UTC offset (e.g. 8 hours on TPE). Append `Z` when missing so
 * both forms parse as UTC. All frontend timestamp consumers must go
 * through this helper, not raw `new Date(...)`.
 */
export function parseUTC(timestamp: string): Date {
	return new Date(timestamp.endsWith("Z") ? timestamp : `${timestamp}Z`);
}

/**
 * Format a UTC timestamp string for display in the given timezone.
 * Defaults to "auto" (browser timezone) when no timezone is provided.
 */
export function formatTimestamp(utcTimestamp: string, timezone = "auto"): string {
	const date = parseUTC(utcTimestamp);
	const tz = timezone === "auto" ? getBrowserTimezone() : timezone;
	return date.toLocaleString("default", {
		timeZone: tz,
		year: "numeric",
		month: "short",
		day: "numeric",
		hour: "2-digit",
		minute: "2-digit",
		second: "2-digit",
	});
}

export function getBrowserTimezone(): string {
	try {
		return Intl.DateTimeFormat().resolvedOptions().timeZone;
	} catch {
		return "UTC";
	}
}

export const COMMON_TIMEZONES = [
	{ value: "auto", label: "Auto (detect from browser)" },
	{ value: "Asia/Taipei", label: "Asia/Taipei (UTC+8)" },
	{ value: "Asia/Tokyo", label: "Asia/Tokyo (UTC+9)" },
	{ value: "Asia/Shanghai", label: "Asia/Shanghai (UTC+8)" },
	{ value: "America/New_York", label: "America/New York (UTC-5/-4)" },
	{ value: "America/Los_Angeles", label: "America/Los Angeles (UTC-8/-7)" },
	{ value: "Europe/London", label: "Europe/London (UTC+0/+1)" },
	{ value: "Europe/Berlin", label: "Europe/Berlin (UTC+1/+2)" },
	{ value: "UTC", label: "UTC" },
];

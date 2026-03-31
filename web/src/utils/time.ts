/**
 * Format a UTC timestamp string for display in the given timezone.
 */
export function formatTimestamp(utcTimestamp: string, timezone: string): string {
  const date = new Date(utcTimestamp.endsWith("Z") ? utcTimestamp : `${utcTimestamp}Z`);
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

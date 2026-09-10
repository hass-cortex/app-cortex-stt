import type { TranscriptionRecord } from "@/api/types";

/**
 * Below this the input is quiet enough to be worth reporting.
 *
 * What a quiet capture actually predicts, measured on this deployment
 * (n=1483 records): an *empty* transcript, 4.9% of the time against 0.4%
 * for a normal-level one, and the gap holds within every capture device.
 * What it does NOT predict is a worse transcript — over 443 human-judged
 * evaluation results the correlation between level and correctness runs
 * the other way (r=-0.26). So: quiet alone is a fact worth printing,
 * quiet plus nothing to show is the only case worth warning about.
 */
export const QUIET_DBFS = -40;

export function isQuiet(rmsDb: number | null): boolean {
	return rmsDb !== null && rmsDb < QUIET_DBFS;
}

/** A quiet capture that returned nothing — the one case the level explains. */
export function isQuietAndSilent(record: TranscriptionRecord): boolean {
	return isQuiet(record.rms_db) && !record.has_error && !record.text.trim();
}

export function formatDbfs(rmsDb: number | null): string {
	return rmsDb === null ? "—" : `${rmsDb.toFixed(1)} dBFS`;
}

/**
 * Clipping is reported as a plain figure, not a warning.
 *
 * Every clipped record in this deployment's history sits under 0.6% of
 * samples, which is trace. Two significant digits so a real fraction
 * never prints as "0.0%" — a threshold worth alarming on can be picked
 * the day the data shows one.
 */
export function formatClipRatio(ratio: number): string {
	const pct = ratio * 100;
	return `${pct >= 1 ? pct.toFixed(1) : pct.toPrecision(2)}%`;
}

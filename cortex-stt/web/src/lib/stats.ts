/**
 * Aggregations the dashboard needs that the API does not expose.
 *
 * Everything here is derived from the transcription history the server
 * already returns, so no endpoint had to grow a statistics mode. Pure
 * functions: no clock reads, no fetching — the caller supplies `now`.
 */

import type { TranscriptionRecord } from "@/api/types";
import { parseUTC } from "@/utils/time";

export interface HourBucket {
	/** Start of the hour, in the browser's zone. */
	start: Date;
	count: number;
	/** Median inference time inside the hour, or null when it is empty. */
	p50: number | null;
}

/** Successful records only — an error carries no inference time worth plotting. */
function timed(records: TranscriptionRecord[]): TranscriptionRecord[] {
	return records.filter((r) => !r.has_error && r.inference_ms > 0);
}

/**
 * Bucket records into the last `hours` clock hours, oldest first. The
 * final bucket is the hour `now` falls in, so the rightmost bar is
 * always "this hour", partial by definition.
 */
export function bucketByHour(records: TranscriptionRecord[], now: Date, hours = 24): HourBucket[] {
	const end = new Date(now);
	end.setMinutes(0, 0, 0);
	const buckets: { start: Date; values: number[] }[] = [];
	for (let i = hours - 1; i >= 0; i--) {
		buckets.push({ start: new Date(end.getTime() - i * 3_600_000), values: [] });
	}
	const first = buckets[0]?.start.getTime() ?? 0;

	for (const record of records) {
		const at = parseUTC(record.timestamp).getTime();
		const index = Math.floor((at - first) / 3_600_000);
		const bucket = buckets[index];
		if (!bucket) continue;
		bucket.values.push(record.inference_ms);
	}

	return buckets.map((b) => ({
		start: b.start,
		count: b.values.length,
		p50: percentile(b.values, 50),
	}));
}

/** Nearest-rank percentile. Returns null for an empty set. */
export function percentile(values: number[], p: number): number | null {
	if (values.length === 0) return null;
	const sorted = [...values].sort((a, b) => a - b);
	const rank = Math.ceil((p / 100) * sorted.length);
	return sorted[Math.min(sorted.length - 1, Math.max(0, rank - 1))] ?? null;
}

export interface Bin {
	from: number;
	to: number;
	count: number;
}

/**
 * Fixed-width bins over [min, max]. Values outside the range land in the
 * end bins rather than vanishing — a clipped tail still has to be visible.
 */
export function histogram(values: number[], min: number, max: number, bins = 8): Bin[] {
	const width = (max - min) / bins;
	const out: Bin[] = Array.from({ length: bins }, (_, i) => ({
		from: min + i * width,
		to: min + (i + 1) * width,
		count: 0,
	}));
	for (const v of values) {
		const index = Math.min(bins - 1, Math.max(0, Math.floor((v - min) / width)));
		const bin = out[index];
		if (bin) bin.count += 1;
	}
	return out;
}

export interface LatencySummary {
	p50: number | null;
	p95: number | null;
	max: number | null;
	bins: Bin[];
	/** Bin range actually used, so the axis can label it. */
	min: number;
	binMax: number;
	samples: number;
}

/** Round up to a readable axis edge (100 ms steps below 2 s, 500 ms above). */
function niceCeil(ms: number): number {
	const step = ms > 2000 ? 500 : 100;
	return Math.max(step, Math.ceil(ms / step) * step);
}

export function summariseLatency(records: TranscriptionRecord[]): LatencySummary {
	const values = timed(records).map((r) => r.inference_ms);
	const p50 = percentile(values, 50);
	const p95 = percentile(values, 95);
	const max = values.length ? Math.max(...values) : null;
	const lo = values.length ? Math.min(...values) : 0;
	const min = Math.max(0, Math.floor(lo / 100) * 100);
	const binMax = niceCeil(p95 ?? 1000);
	return {
		p50,
		p95,
		max,
		bins: values.length ? histogram(values, min, binMax) : [],
		min,
		binMax,
		samples: values.length,
	};
}

/**
 * Median real-time factor: inference over audio duration, per record.
 * Taken as a median of ratios rather than a ratio of sums so one long
 * clip cannot define the number.
 */
export function medianRtf(records: TranscriptionRecord[]): number | null {
	const ratios = timed(records)
		.filter((r) => r.audio_duration_ms > 0)
		.map((r) => r.inference_ms / r.audio_duration_ms);
	const p50 = percentile(ratios, 50);
	return p50 === null ? null : p50;
}

export interface ModelLatency {
	p50: number;
	runs: number;
}

/**
 * Per-model median inference time, measured on this machine. This is the
 * only latency number the UI is allowed to present as fact — the catalog's
 * speed score is a vendor claim about other hardware.
 */
export function medianByModel(records: TranscriptionRecord[]): Map<string, ModelLatency> {
	const grouped = new Map<string, number[]>();
	for (const record of timed(records)) {
		const list = grouped.get(record.model_id);
		if (list) list.push(record.inference_ms);
		else grouped.set(record.model_id, [record.inference_ms]);
	}
	const out = new Map<string, ModelLatency>();
	for (const [modelId, values] of grouped) {
		const p50 = percentile(values, 50);
		if (p50 !== null) out.set(modelId, { p50, runs: values.length });
	}
	return out;
}

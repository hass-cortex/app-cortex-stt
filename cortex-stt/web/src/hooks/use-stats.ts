import { useMemo } from "react";
import type { ModelInfo, TranscriptionRecord } from "@/api/types";
import { useHistoryList } from "@/hooks/use-history";
import {
	bucketByHour,
	type HourBucket,
	type LatencySummary,
	type ModelLatency,
	medianBy,
	medianRtf,
	summariseLatency,
} from "@/lib/stats";
import { variantKey } from "@/lib/variant";
import { parseUTC } from "@/utils/time";

/** Records are pulled in one window and aggregated in the browser; a
 *  statistics endpoint would have to invent the same buckets server-side.
 *  Capped so a long history cannot turn the dashboard into a 10 MB fetch. */
const WINDOW_LIMIT = 2000;

/**
 * Anchor the range to the top of the current hour. A `Date.now()` bound
 * would produce a new query key on every render and refetch forever.
 */
function hourAnchor(): Date {
	const anchor = new Date();
	anchor.setMinutes(0, 0, 0);
	return anchor;
}

function useWindow(hours: number): {
	records: TranscriptionRecord[];
	anchor: Date;
	isLoading: boolean;
} {
	const anchor = useMemo(hourAnchor, []);
	const from = useMemo(
		() => new Date(anchor.getTime() - (hours - 1) * 3_600_000).toISOString(),
		[anchor, hours],
	);
	const { data, isLoading } = useHistoryList({ from, limit: WINDOW_LIMIT });
	return { records: data ?? [], anchor, isLoading };
}

export interface DashboardStats {
	hours: HourBucket[];
	/** Newest first — the same window the charts read, so the dashboard
	 *  makes exactly one history request. */
	recent: TranscriptionRecord[];
	latency: LatencySummary;
	rtf: number | null;
	peak: HourBucket | null;
	isLoading: boolean;
}

export function useDashboardStats(): DashboardStats {
	const { records, anchor, isLoading } = useWindow(24);

	return useMemo(() => {
		const hours = bucketByHour(records, anchor, 24);
		const peak = hours.reduce<HourBucket | null>(
			(best, h) => (best === null || h.count > best.count ? h : best),
			null,
		);
		const recent = [...records]
			.sort((a, b) => parseUTC(b.timestamp).getTime() - parseUTC(a.timestamp).getTime())
			.slice(0, 6);

		return {
			hours,
			recent,
			latency: summariseLatency(records),
			rtf: medianRtf(records),
			peak: peak && peak.count > 0 ? peak : null,
			isLoading,
		};
	}, [records, anchor, isLoading]);
}

/**
 * Median inference time per Variant key over the last week, measured
 * here. The catalog's speed score describes someone else's hardware and
 * is not a substitute — a Variant with no runs reports nothing rather
 * than a guess.
 */
export function useMeasuredLatency(models: ModelInfo[] | undefined): {
	byVariant: Map<string, ModelLatency>;
	isLoading: boolean;
} {
	const { records, isLoading } = useWindow(24 * 7);
	return useMemo(() => {
		// A record from before Variants names no quant; it counts toward the
		// model's Variant while the model has only one installed.
		const sole = new Map<string, string>();
		for (const m of models ?? []) {
			const [only, ...rest] = m.variants.filter((v) => v.status === "downloaded");
			if (only && rest.length === 0) sole.set(m.id, variantKey(m.id, only.quant));
		}
		const keyOf = (r: TranscriptionRecord) =>
			r.quant ? variantKey(r.model_id, r.quant) : (sole.get(r.model_id) ?? r.model_id);
		return { byVariant: medianBy(records, keyOf), isLoading };
	}, [records, models, isLoading]);
}

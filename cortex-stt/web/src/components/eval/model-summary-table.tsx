import { RotateCcw } from "lucide-react";
import type { ModelSummary, RunSummary } from "@/api/types";
import { MemoryFigure } from "@/components/eval/memory-figure";
import { Badge } from "@/components/ui/badge";
import { SortHeader } from "@/components/ui/sort-header";
import { type SortColumn, useTableSort } from "@/hooks/use-table-sort";
import { formatDuration } from "@/lib/format";

/**
 * What each column contributes to an ordering.
 *
 * A figure a failed candidate does not have is null, not zero: it sinks
 * to the bottom whichever way the sort runs, so "sort by median" never
 * reports a model that produced nothing as the fastest one.
 */
const SORT_COLUMNS: Record<string, SortColumn<ModelSummary>> = {
	model: { value: (m) => m.model_id, first: "asc" },
	score: {
		// The rate, not the count: 3/4 beats 3/8, and both beat 1/1 on
		// nothing but luck — which is why the denominator stays on screen.
		value: (m) => (m.load_failed || m.scored === 0 ? null : m.correct / m.scored),
		first: "desc",
	},
	rtf: { value: (m) => m.median_rtf, first: "asc" },
	median: { value: (m) => m.median_inference_ms, first: "asc" },
	slowest: { value: (m) => m.max_inference_ms, first: "asc" },
	// The second memory figure is the one that decides whether a candidate
	// can be the default, so it is the one the column sorts on.
	memory: { value: (m) => (m.load_failed ? null : m.coexist_bytes), first: "asc" },
	load: { value: (m) => (m.load_failed ? null : m.cold_load_ms), first: "asc" },
	changed: { value: (m) => m.changed_from_previous, first: "desc" },
};

/**
 * How a run's candidates are presented, defined once.
 *
 * The overview shows it for the latest run and the run detail shows it
 * for whichever run is open — the figures belong to a run, not to
 * "the most recent one", and a run detail that carried only transcripts
 * would leave every measurement stranded on another screen.
 */
export function ModelSummaryTable({ summary }: { summary: RunSummary }) {
	const { run, models } = summary;
	const sort = useTableSort(models, SORT_COLUMNS);
	const showChanged = models.some((m) => m.changed_from_previous !== null);

	return (
		<>
			{/* Run order is the order the models were executed in, and the
			    memory and load figures were taken in that sequence — so it
			    stays one click away rather than being lost to the first sort. */}
			{sort.active && (
				<div className="flex justify-end">
					<button
						type="button"
						onClick={sort.reset}
						aria-label="Sort by run order — the order the models were executed in"
						className="inline-flex items-center gap-1 text-[11px] text-text-muted hover:text-text-primary cursor-pointer transition-colors"
					>
						<RotateCcw size={11} strokeWidth={1.8} />
						Run order
					</button>
				</div>
			)}

			<div className="overflow-x-auto">
				<table className="w-full min-w-[720px] text-sm">
					<thead>
						<tr className="text-left text-xs text-text-muted">
							<SortHeader column="model" active={sort.active} onToggle={sort.toggle}>
								Model
							</SortHeader>
							<SortHeader column="score" active={sort.active} onToggle={sort.toggle}>
								Correct
							</SortHeader>
							<SortHeader column="rtf" active={sort.active} onToggle={sort.toggle}>
								Median RTF
							</SortHeader>
							<SortHeader column="median" active={sort.active} onToggle={sort.toggle}>
								Median
							</SortHeader>
							<SortHeader column="slowest" active={sort.active} onToggle={sort.toggle}>
								Slowest
							</SortHeader>
							<SortHeader column="memory" active={sort.active} onToggle={sort.toggle}>
								Resident / together
							</SortHeader>
							<SortHeader column="load" active={sort.active} onToggle={sort.toggle}>
								Load
							</SortHeader>
							{showChanged && (
								<SortHeader column="changed" active={sort.active} onToggle={sort.toggle}>
									Changed
								</SortHeader>
							)}
						</tr>
					</thead>
					<tbody>
						{sort.rows.map((m) => (
							<tr key={m.model_id} className="border-t border-border">
								<td className="py-2 pr-4 text-text-primary">{m.model_id}</td>
								<td className="py-2 pr-4 font-mono tabular-nums">{score(m)}</td>
								<td className="py-2 pr-4 font-mono tabular-nums">
									{m.median_rtf === null ? "—" : `${m.median_rtf.toFixed(2)}×`}
								</td>
								<td className="py-2 pr-4 font-mono tabular-nums">
									{m.median_inference_ms === null ? "—" : `${m.median_inference_ms} ms`}
								</td>
								<td className="py-2 pr-4 font-mono tabular-nums text-text-secondary">
									{m.max_inference_ms === null ? "—" : `${m.max_inference_ms} ms`}
								</td>
								<td className="py-2 pr-4">
									<MemoryFigure model={m} />
								</td>
								<td className="py-2 pr-4">
									{m.load_failed ? (
										<Badge variant="error">failed</Badge>
									) : m.fits_alongside === false ? (
										<Badge variant="warning">will not fit alongside</Badge>
									) : (
										<span className="font-mono tabular-nums text-text-secondary">
											{formatDuration(m.cold_load_ms)}
										</span>
									)}
								</td>
								{showChanged && (
									<td className="py-2 pr-4 font-mono tabular-nums">
										{m.changed_from_previous === null ? (
											"—"
										) : m.changed_from_previous > 0 ? (
											<span className="text-accent font-semibold">{m.changed_from_previous}</span>
										) : (
											<span className="text-text-muted">0</span>
										)}
									</td>
								)}
							</tr>
						))}
					</tbody>
				</table>
			</div>

			{run.status === "cancelled" && (
				<p className="mt-3 text-xs text-text-muted">
					Stopped before it finished. The candidates below are the ones it reached — the rest were
					never loaded, so they are absent rather than reported as zero.
				</p>
			)}
			{models.some((m) => m.failure_reason) && (
				<ul className="mt-3 space-y-1 text-xs text-error">
					{models
						.filter((m) => m.failure_reason)
						.map((m) => (
							<li key={m.model_id}>
								{m.model_id}: {m.failure_reason}
							</li>
						))}
				</ul>
			)}
			{models.some((m) => m.load_failed || m.fits_alongside === false) && (
				<p className="mt-3 text-xs text-text-muted">
					A candidate that loads on its own can still fail as the default: this deployment keeps{" "}
					{run.max_loaded_models} models resident, so the second memory figure is the one that
					decides.
				</p>
			)}
			{!summary.previous_run_id && summary.skipped_baselines > 0 && (
				<p className="mt-2 text-xs text-warning">
					No comparison: {summary.skipped_baselines} earlier run(s) exist but used a different
					language hint. The hint changes what a model transcribes, so comparing across hints would
					report a setting as a model change.
				</p>
			)}
			{summary.previous_run_id && (
				<p className="mt-2 text-xs text-text-muted">
					"Changed" counts samples whose output differs from the previous run, over the{" "}
					{summary.compared_samples} sample(s) both runs covered. Inference is deterministic, so a
					change is exact — unlike the timings above, which are only comparable within this run.
				</p>
			)}
		</>
	);
}

/**
 * The score, and how much of it rests on nobody's word.
 *
 * Only the unchecked *correct* cells are counted: those are the points
 * the comparison granted on its own, so they are what could be inflating
 * the figure. Unchecked wrong cells are already withholding a point.
 */
function score(m: ModelSummary): string {
	if (m.load_failed) return "—";
	if (m.scored === 0) return "no results";
	return m.unchecked_correct === 0
		? `${m.correct} / ${m.scored}`
		: `${m.correct} / ${m.scored} · ${m.unchecked_correct} unchecked`;
}

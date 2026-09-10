import { FlaskConical } from "lucide-react";
import type { ModelRunEntry } from "@/api/types";
import { MemoryFigure } from "@/components/eval/memory-figure";
import { Badge } from "@/components/ui/badge";
import { Card, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { useModelRuns } from "@/hooks/use-eval";
import { formatDuration, formatRelativeTime } from "@/lib/format";

interface ModelRunsViewProps {
	modelId: string;
	onOpenRun: (runId: string) => void;
}

/**
 * One model across every run it appeared in.
 *
 * Sits above the Runs list whenever its model filter is set. The list
 * answers "which runs included this model"; this answers "how has it
 * behaved across them", which a flat list of runs cannot show. Clearing
 * belongs to the filter bar, so this view has no control of its own.
 */
export function ModelRunsView({ modelId, onOpenRun }: ModelRunsViewProps) {
	const { data: entries = [], isLoading } = useModelRuns(modelId);

	if (isLoading) return null;

	if (entries.length === 0) {
		return (
			<Card>
				<CardHeader title={modelId} />
				<EmptyState
					icon={<FlaskConical size={28} />}
					title="Never evaluated"
					description={`${modelId} has not been in a run yet. Start one and include it as a candidate.`}
				/>
			</Card>
		);
	}

	return (
		<Card>
			<CardHeader title={modelId} description={`${entries.length} run(s)`} />
			<div className="overflow-x-auto">
				<table className="w-full min-w-[640px] text-sm">
					<thead>
						<tr className="text-left text-xs uppercase tracking-wider text-text-muted">
							<th className="py-2 pr-4">Run</th>
							<th className="py-2 pr-4">Version</th>
							<th className="py-2 pr-4">Correct</th>
							<th className="py-2 pr-4">Median RTF</th>
							<th className="py-2 pr-4">Resident / together</th>
							<th className="py-2 pr-4">Changed</th>
						</tr>
					</thead>
					<tbody>
						{entries.map((e: ModelRunEntry) => (
							<tr key={e.run.id} className="border-t border-border">
								<td className="py-2 pr-4">
									<button
										type="button"
										onClick={() => onOpenRun(e.run.id)}
										className="text-text-primary hover:text-accent cursor-pointer"
									>
										{formatRelativeTime(e.run.started_at)}
									</button>
									<span className="block text-xs text-text-muted">{e.sample_count} samples</span>
								</td>
								<td className="py-2 pr-4 font-mono text-xs text-text-secondary">
									{e.run.app_version} / tc {e.run.engine_version}
									<span className="block text-text-muted">hint: {e.run.language ?? "none"}</span>
								</td>
								<td className="py-2 pr-4 font-mono tabular-nums">
									{e.summary.load_failed ? (
										<Badge variant="error">failed</Badge>
									) : e.summary.scored === 0 ? (
										"no results"
									) : (
										`${e.summary.correct} / ${e.summary.scored}`
									)}
								</td>
								<td className="py-2 pr-4 font-mono tabular-nums">
									{e.summary.median_rtf === null ? "—" : `${e.summary.median_rtf.toFixed(2)}×`}
									{e.summary.median_inference_ms !== null && (
										<span className="block text-xs text-text-muted">
											{formatDuration(e.summary.median_inference_ms)} median
										</span>
									)}
								</td>
								<td className="py-2 pr-4">
									<MemoryFigure model={e.summary} />
								</td>
								<td className="py-2 pr-4 font-mono tabular-nums">
									{e.summary.changed_from_previous === null ? (
										<span className="text-text-muted">—</span>
									) : e.summary.changed_from_previous > 0 ? (
										<span className="text-accent font-semibold">
											{e.summary.changed_from_previous}
										</span>
									) : (
										<span className="text-text-muted">0</span>
									)}
									<span className="block text-xs text-text-muted">of {e.compared_samples}</span>
								</td>
							</tr>
						))}
					</tbody>
				</table>
			</div>
			<p className="mt-3 text-xs text-text-muted">
				"Changed" is exact — inference is deterministic, so a differing output means the model or
				the runtime changed. The timings are not comparable between rows: each was measured under
				whatever else the machine was doing at the time.
			</p>
		</Card>
	);
}

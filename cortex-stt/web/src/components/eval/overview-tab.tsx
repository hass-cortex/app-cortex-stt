import { FlaskConical } from "lucide-react";
import { useState } from "react";
import type { RunSummary } from "@/api/types";
import { ModelSummaryTable } from "@/components/eval/model-summary-table";
import { RunTable } from "@/components/eval/run-table";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { useRuns, useSetRunNote } from "@/hooks/use-eval";
import { formatRelativeTime } from "@/lib/format";

interface OverviewTabProps {
	latest: RunSummary | null;
	pendingCount: number;
	onOpenRun: (runId: string) => void;
}

export function OverviewTab({ latest, pendingCount, onOpenRun }: OverviewTabProps) {
	if (!latest) {
		return (
			<EmptyState
				icon={<FlaskConical size={28} />}
				title="No runs yet"
				description={
					pendingCount > 0
						? `${pendingCount} recording(s) are waiting to be labelled. Label them, then start a run.`
						: "Add recordings to the evaluation set, label them, then run your installed models against them."
				}
			/>
		);
	}

	const { run } = latest;

	return (
		<div className="space-y-6">
			<Card>
				<CardHeader
					title="Latest run"
					description={`${formatRelativeTime(run.started_at)} · ${run.app_version} · transcribe-cpp ${run.engine_version} · ${latest.sample_count} samples · hint: ${run.language ?? "none"}`}
					action={
						<Button size="sm" variant="secondary" onClick={() => onOpenRun(run.id)}>
							Open
						</Button>
					}
				/>
				<ModelSummaryTable summary={latest} />
				<RunNote runId={run.id} note={run.note} />
			</Card>

			<CrossRunTable onOpenRun={onOpenRun} />
		</div>
	);
}

/** The conclusion, in the author's own words. Nothing forces it to be
 *  filled; the runs that carry one are the runs where a decision was
 *  actually made. */
function RunNote({ runId, note }: { runId: string; note: string | null }) {
	const [editing, setEditing] = useState(false);
	const [value, setValue] = useState(note ?? "");
	const save = useSetRunNote();

	if (!editing) {
		return (
			<button
				type="button"
				onClick={() => {
					setValue(note ?? "");
					setEditing(true);
				}}
				className="mt-4 block w-full text-left text-sm cursor-pointer"
			>
				{note ? (
					<span className="text-text-secondary italic">{note}</span>
				) : (
					<span className="text-text-muted">Add a note about what you concluded</span>
				)}
			</button>
		);
	}

	return (
		<div className="mt-4 space-y-2">
			<textarea
				value={value}
				onChange={(e) => setValue(e.target.value)}
				rows={2}
				className="w-full px-3 py-2 rounded-lg bg-surface-2 border border-border text-text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent"
			/>
			<div className="flex justify-end gap-2">
				<Button size="sm" variant="ghost" onClick={() => setEditing(false)}>
					Cancel
				</Button>
				<Button
					size="sm"
					loading={save.isPending}
					onClick={() =>
						save.mutate(
							{ id: runId, note: value.trim() || null },
							{ onSuccess: () => setEditing(false) },
						)
					}
				>
					Save
				</Button>
			</div>
		</div>
	);
}

/** Across runs, the only trustworthy signal is whether output changed:
 *  inference is deterministic, so a change is exact. Latency is not
 *  comparable across runs at this sample size, so it is absent here on
 *  purpose. */
function CrossRunTable({ onOpenRun }: { onOpenRun: (id: string) => void }) {
	const { data: allRuns = [] } = useRuns();
	const runs = allRuns.slice(0, 10);
	// One run is not a comparison, and this card exists to compare.
	if (runs.length < 2) return null;

	return (
		<Card>
			<CardHeader
				title="Across runs"
				description="Output changes are exact. Latency is not comparable between runs, so it is not shown."
			/>
			{/* The same table the Runs tab draws, so a run reads the same in
			    both places. Deleting is not offered here — this is a summary,
			    and the Runs tab is where runs are managed. */}
			<RunTable runs={runs} onOpen={onOpenRun} />
		</Card>
	);
}

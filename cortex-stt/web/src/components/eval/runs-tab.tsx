import { ArrowLeft, FlaskConical, X } from "lucide-react";
import { useState } from "react";
import { ModelSummaryTable } from "@/components/eval/model-summary-table";
import { ResultMatrix } from "@/components/eval/result-matrix";
import { RunTable } from "@/components/eval/run-table";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useDeleteRun, useEvalResults, useRunSummary, useRuns, useSamples } from "@/hooks/use-eval";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { formatRelativeTime } from "@/lib/format";

interface RunsTabProps {
	selectedRunId: string | null;
	onSelectRun: (id: string | null) => void;
	/** Lives in the URL, not in local state: the Models page links here
	 *  with `?model=`, and a filter that a link can set has to survive a
	 *  reload and a shared URL. */
	modelFilter: string | null;
	onModelFilter: (id: string | null) => void;
}

/** "Ran without a language hint" needs a value of its own: the empty
 *  string is already taken by "any hint", which filters nothing. Not a
 *  possible BCP-47 tag, so it cannot collide with a real one. */
const NO_HINT = "(none)";

export function RunsTab({ selectedRunId, onSelectRun, modelFilter, onModelFilter }: RunsTabProps) {
	const [text, setText] = useState("");
	const { data: runs = [] } = useRuns(text);
	// Facets come from the unsearched list, or the dropdowns would empty
	// as you type. Same query key the Overview uses, so no extra request.
	const { data: allRuns = [] } = useRuns();
	const [status, setStatus] = useState("");
	const [hint, setHint] = useState("");
	const remove = useDeleteRun();
	const runRemove = useMutationToast(remove, {
		success: "Run deleted",
		error: "Could not delete the run",
	});

	if (selectedRunId) {
		return <RunDetail runId={selectedRunId} onBack={() => onSelectRun(null)} />;
	}

	// From the runs themselves rather than the catalog: a filter offering
	// a model that was never evaluated only produces an empty list.
	const models = [...new Set(allRuns.flatMap((r) => r.model_ids))].sort();
	const hints = [...new Set(allRuns.map((r) => r.language ?? NO_HINT))].sort();

	// Only what a row already carries is filtered here; the text search
	// is server-side because it reaches into references and outputs.
	const shown = runs.filter((r) => {
		if (modelFilter && !r.model_ids.includes(modelFilter)) return false;
		if (status && r.status !== status) return false;
		if (hint && (r.language ?? NO_HINT) !== hint) return false;
		return true;
	});

	if (allRuns.length === 0) {
		return (
			<EmptyState
				icon={<FlaskConical size={28} />}
				title="No runs yet"
				description="Start one from the Overview tab."
			/>
		);
	}

	const filtered = !!modelFilter || !!status || !!hint || !!text.trim();

	return (
		<div className="space-y-4">
			<div className="flex flex-col sm:flex-row gap-3">
				<Input
					type="text"
					placeholder="Search transcripts, models, note..."
					value={text}
					onChange={(e) => setText(e.target.value)}
					className="sm:w-64"
				/>
				<Select
					options={[
						{ value: "", label: "All models" },
						...models.map((m) => ({ value: m, label: m })),
					]}
					value={modelFilter ?? ""}
					onChange={(e) => onModelFilter(e.target.value || null)}
					className="sm:w-56"
				/>
				<Select
					options={[
						{ value: "", label: "All statuses" },
						{ value: "completed", label: "Completed" },
						{ value: "running", label: "Running" },
						{ value: "failed", label: "Failed" },
						{ value: "cancelled", label: "Cancelled" },
					]}
					value={status}
					onChange={(e) => setStatus(e.target.value)}
					className="sm:w-40"
				/>
				<Select
					options={[
						{ value: "", label: "Any hint" },
						...hints.map((h) => ({ value: h, label: h === NO_HINT ? "no hint" : h })),
					]}
					value={hint}
					onChange={(e) => setHint(e.target.value)}
					className="sm:w-36"
				/>
			</div>

			<Card>
				<CardHeader
					title="Runs"
					description={
						filtered ? `${shown.length} of ${runs.length} · newest first` : "Newest first"
					}
					action={
						filtered ? (
							<Button
								size="sm"
								variant="ghost"
								onClick={() => {
									setText("");
									setStatus("");
									setHint("");
									onModelFilter(null);
								}}
							>
								Clear filters
							</Button>
						) : undefined
					}
				/>
				{shown.length === 0 ? (
					<EmptyState
						icon={<FlaskConical size={28} />}
						title="No runs match"
						description="Widen the filters, or clear them to see every run."
					/>
				) : (
					<RunTable
						runs={shown}
						onOpen={onSelectRun}
						onDelete={runRemove}
						focusModel={modelFilter}
					/>
				)}
			</Card>
		</div>
	);
}

function RunDetail({ runId, onBack }: { runId: string; onBack: () => void }) {
	const { data: summary } = useRunSummary(runId);
	const { data: results = [] } = useEvalResults({ run_id: runId, limit: 5000 });
	const { data: samples = [] } = useSamples();
	const [sampleFilter, setSampleFilter] = useState<string | null>(null);

	if (!summary) return null;

	const shown = sampleFilter ? results.filter((r) => r.sample_id === sampleFilter) : results;

	return (
		<div className="space-y-4">
			{/* Leaving the detail is navigation, not an action on the run, so
			    it sits where the reading starts and names its destination. */}
			<button
				type="button"
				onClick={onBack}
				className="inline-flex items-center gap-1.5 text-[12.5px] text-text-secondary hover:text-text-primary cursor-pointer transition-colors"
			>
				<ArrowLeft size={14} strokeWidth={1.8} />
				All runs
			</button>

			<Card>
				<CardHeader
					title={formatRelativeTime(summary.run.started_at)}
					description={`${summary.run.app_version} · transcribe-cpp ${summary.run.engine_version} · ${summary.sample_count} samples · hint: ${summary.run.language ?? "none"}${
						summary.previous_run_id
							? ` · ${summary.compared_samples} comparable with the previous run`
							: ""
					}`}
				/>
				<ModelSummaryTable summary={summary} />
				{summary.run.note && (
					<p className="mt-3 text-sm text-text-secondary italic">{summary.run.note}</p>
				)}
			</Card>

			<Card>
				<ResultMatrix
					samples={samples}
					results={shown}
					onSelectSample={(id) => setSampleFilter(id === sampleFilter ? null : id)}
					/* The filter narrows this table, so its release sits on this
					   table's toolbar — not up in the run summary it does not touch. */
					filterNotice={
						sampleFilter ? (
							<button
								type="button"
								onClick={() => setSampleFilter(null)}
								className="num inline-flex items-center gap-1.5 px-2.5 py-[5px] rounded-[7px] bg-accent-wash border border-accent-quiet text-[11.5px] text-text-primary cursor-pointer hover:bg-surface-3 transition-colors"
							>
								<X size={12} strokeWidth={2} />
								one sample — show all {results.length} results
							</button>
						) : null
					}
				/>
			</Card>
		</div>
	);
}

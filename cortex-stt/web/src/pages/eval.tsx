import { Play } from "lucide-react";
import { useState } from "react";
import { useSearchParams } from "react-router";
import { ModelRunsView } from "@/components/eval/model-runs-view";
import { NewRunDialog } from "@/components/eval/new-run-dialog";
import { OverviewTab } from "@/components/eval/overview-tab";
import { RunProgressPanel } from "@/components/eval/run-progress";
import { RunsTab } from "@/components/eval/runs-tab";
import { SamplesTab } from "@/components/eval/samples-tab";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useEvalOverview, useRunProgress } from "@/hooks/use-eval";

type Tab = "overview" | "samples" | "runs";

const TABS: { id: Tab; label: string }[] = [
	{ id: "overview", label: "Overview" },
	{ id: "samples", label: "Samples" },
	{ id: "runs", label: "Runs" },
];

export function EvalPage() {
	// `?model=` is how the Models page links here. It is the Runs tab's
	// model filter, not a separate mode: a link setting a filter is one
	// consumer of the filter bar, not a second way to view runs.
	const [params, setParams] = useSearchParams();
	const modelFilter = params.get("model");
	const [tab, setTab] = useState<Tab>(modelFilter ? "runs" : "overview");
	const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
	const [dialogOpen, setDialogOpen] = useState(false);
	// Which samples the dialog should open on. Null is the whole set —
	// what the header's own New run button means.
	const [runSampleIds, setRunSampleIds] = useState<string[] | null>(null);
	const { data: overview } = useEvalOverview();
	const progress = useRunProgress();

	const openRun = (id: string) => {
		setSelectedRunId(id);
		setTab("runs");
	};

	const setModelFilter = (id: string | null) => {
		if (id) {
			params.set("model", id);
		} else {
			params.delete("model");
		}
		setParams(params, { replace: true });
	};

	return (
		<div className="space-y-6">
			<div className="flex flex-wrap items-center justify-between gap-3">
				<div>
					<h1 className="text-xl font-semibold text-text-primary">Evaluation</h1>
					<p className="text-sm text-text-muted">
						Compare installed models on recordings whose correct transcription you typed in.
					</p>
				</div>
				<Button
					icon={<Play size={14} />}
					disabled={!!progress || (overview?.composition.total ?? 0) === 0}
					onClick={() => {
						setRunSampleIds(null);
						setDialogOpen(true);
					}}
				>
					New run
				</Button>
			</div>

			{progress && (
				<Card>
					<RunProgressPanel progress={progress} />
				</Card>
			)}

			<div className="flex gap-1 border-b border-border">
				{TABS.map((t) => (
					<button
						type="button"
						key={t.id}
						onClick={() => {
							setTab(t.id);
							if (t.id !== "runs") setSelectedRunId(null);
						}}
						className={`px-3 py-2 text-sm font-medium border-b-2 -mb-px cursor-pointer transition-colors ${
							tab === t.id
								? "border-accent text-text-primary"
								: "border-transparent text-text-muted hover:text-text-secondary"
						}`}
					>
						{t.label}
						{t.id === "samples" && (overview?.pending_count ?? 0) > 0 && (
							<span className="ml-1.5 px-1.5 py-0.5 rounded-full bg-accent/15 text-accent text-xs">
								{overview?.pending_count}
							</span>
						)}
					</button>
				))}
			</div>

			{tab === "overview" && (
				<OverviewTab
					latest={overview?.latest ?? null}
					pendingCount={overview?.pending_count ?? 0}
					onOpenRun={openRun}
				/>
			)}
			{tab === "samples" && (
				<SamplesTab
					composition={overview?.composition}
					onRunSamples={(ids) => {
						setRunSampleIds(ids);
						setDialogOpen(true);
					}}
				/>
			)}
			{tab === "runs" && (
				<>
					{/* One model across every run it appeared in. The flat list
					    cannot show a trend, so it sits above the filtered list
					    rather than replacing it. */}
					{modelFilter && !selectedRunId && (
						<ModelRunsView modelId={modelFilter} onOpenRun={setSelectedRunId} />
					)}
					<RunsTab
						selectedRunId={selectedRunId}
						onSelectRun={setSelectedRunId}
						modelFilter={modelFilter}
						onModelFilter={setModelFilter}
					/>
				</>
			)}

			<NewRunDialog
				open={dialogOpen}
				onClose={() => setDialogOpen(false)}
				sampleCount={overview?.composition.total ?? 0}
				availableMemoryBytes={overview?.latest?.run.available_memory_bytes}
				initialSampleIds={runSampleIds}
				onStarted={openRun}
			/>
		</div>
	);
}

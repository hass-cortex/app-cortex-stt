import { Info, Square } from "lucide-react";
import type { RunProgress } from "@/api/types";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import { ProgressBar } from "@/components/ui/progress-bar";
import { useCancelRun } from "@/hooks/use-eval";
import { useMutationToast } from "@/hooks/use-mutation-toast";

/**
 * Progress reported the way the run actually happens: one model at a
 * time, every sample, then the next. Smoothing that into a single bar
 * would make the pause while a model loads look like a stall.
 */
export function RunProgressPanel({ progress }: { progress: RunProgress }) {
	const cancel = useCancelRun();
	const runCancel = useMutationToast(cancel, {
		success: "Stopping after the current sample",
		error: "Could not stop the run",
	});

	const overall =
		progress.model_total === 0
			? 0
			: ((progress.model_index + progress.sample_index / Math.max(progress.sample_total, 1)) /
					progress.model_total) *
				100;

	return (
		<div className="space-y-2">
			<div className="flex flex-wrap items-baseline justify-between gap-2 text-sm">
				<span className="text-text-primary font-medium">
					{progress.current_model ?? "Preparing"}
				</span>
				<div className="flex items-baseline gap-3">
					<span className="text-xs text-text-muted font-mono tabular-nums">
						model {progress.model_index + 1}/{progress.model_total} · sample {progress.sample_index}
						/{progress.sample_total}
					</span>
					{/* Stopping is cooperative — the run finishes the sample it is
					    on — so the button reports the request, not the stop. */}
					<Button
						size="sm"
						variant="ghost"
						icon={<Square size={11} strokeWidth={2} fill="currentColor" />}
						loading={cancel.isPending}
						disabled={cancel.isSuccess}
						onClick={() => runCancel(progress.run_id)}
					>
						{cancel.isSuccess ? "Stopping…" : "Stop"}
					</Button>
					<Hint
						label="Stop"
						content="Finishes the current sample, then stops. Results measured so far are kept."
						width={240}
						className="text-text-faint hover:text-text-secondary"
					>
						<Info size={12} strokeWidth={1.8} />
					</Hint>
				</div>
			</div>
			<ProgressBar value={overall} />
		</div>
	);
}

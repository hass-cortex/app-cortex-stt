import { useMemo } from "react";
import type { EvalResult, EvalSample } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { useEvalResults, useRuns } from "@/hooks/use-eval";
import { verdictOf } from "@/lib/verdict";

interface SampleHistoryProps {
	sample: EvalSample;
}

/**
 * One sample across every model that has transcribed it.
 *
 * The transpose of the run detail matrix, which reads a run across its
 * samples. This reads a sample across its models, which is the question
 * a person has while looking at a sample: who gets this one right.
 *
 * Rows are ordered by verdict rather than by name — "who gets it right"
 * should not require reading every row.
 */
export function SampleHistory({ sample }: SampleHistoryProps) {
	const { data: results = [] } = useEvalResults({ sample_id: sample.id, limit: 2000 });
	const { data: runs = [] } = useRuns();

	const startedAt = useMemo(() => new Map(runs.map((r) => [r.id, r.started_at])), [runs]);

	const byModel = useMemo(() => {
		const m = new Map<string, { latest: EvalResult; when: string; texts: Set<string> }>();
		for (const r of results) {
			const when = startedAt.get(r.run_id) ?? "";
			const cur = m.get(r.model_id);
			if (!cur) {
				m.set(r.model_id, { latest: r, when, texts: new Set([r.text]) });
				continue;
			}
			cur.texts.add(r.text);
			// Newest wins: a model's current answer is the one that matters,
			// and the set below still records that it has said other things.
			if (when > cur.when) {
				cur.latest = r;
				cur.when = when;
			}
		}
		return [...m.entries()]
			.map(([model_id, v]) => ({
				model_id,
				text: v.latest.text,
				...verdictOf(v.latest),
				distinct: v.texts.size,
				runs: results.filter((r) => r.model_id === model_id).length,
			}))
			.sort(
				(a, b) =>
					Number(b.correct) - Number(a.correct) ||
					Number(b.confirmed) - Number(a.confirmed) ||
					a.model_id.localeCompare(b.model_id),
			);
	}, [results, startedAt]);

	if (byModel.length === 0) {
		return (
			<p className="text-xs text-text-muted">
				No model has transcribed this sample yet. Include it in a run.
			</p>
		);
	}

	const right = byModel.filter((m) => m.correct).length;
	const unchecked = byModel.filter((m) => m.correct && !m.confirmed).length;

	return (
		<div className="space-y-2">
			<p className="text-xs text-text-muted">
				{right} of {byModel.length} models that have tried this get it right
				{unchecked > 0 && ` · ${unchecked} on the comparison's word alone`}.
			</p>
			<div className="overflow-x-auto">
				<table className="w-full min-w-[560px] text-sm">
					<thead>
						<tr className="text-left text-xs uppercase tracking-wider text-text-muted">
							<th className="py-1 pr-4">Model</th>
							<th className="py-1 pr-4">Latest output</th>
							<th className="py-1 pr-4">Verdict</th>
							<th className="py-1 pr-4">Runs</th>
						</tr>
					</thead>
					<tbody>
						{byModel.map((m) => (
							<tr key={m.model_id} className="border-t border-border">
								<td className="py-1.5 pr-4 font-mono text-xs text-text-secondary whitespace-nowrap">
									{m.model_id}
								</td>
								<td className="py-1.5 pr-4">
									<span className={m.correct ? "text-success" : "text-error"}>
										{m.text || <span className="text-text-muted italic">empty</span>}
									</span>
								</td>
								<td className="py-1.5 pr-4">
									<Badge variant={m.correct ? "success" : "error"}>
										{m.correct ? "correct" : "wrong"}
									</Badge>
									{/* Flagged on the correct rows only: an unchecked wrong is
									    not granting the model anything to be wrong about. */}
									{m.correct && !m.confirmed && (
										<span className="ml-1.5 text-xs text-text-muted">unchecked</span>
									)}
								</td>
								<td className="py-1.5 pr-4 text-xs text-text-muted font-mono tabular-nums whitespace-nowrap">
									{m.runs}
									{/* More than one distinct answer means the model is not
									    stable on this clip — worth more than the latest row. */}
									{m.distinct > 1 && (
										<span className="text-amber-500"> · {m.distinct} answers</span>
									)}
								</td>
							</tr>
						))}
					</tbody>
				</table>
			</div>
		</div>
	);
}

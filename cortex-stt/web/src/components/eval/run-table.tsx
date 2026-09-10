import { ChevronRight, Trash2 } from "lucide-react";
import type { EvalRunListEntry } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { formatRelativeTime } from "@/lib/format";

interface RunTableProps {
	runs: EvalRunListEntry[];
	onOpen: (id: string) => void;
	/** Omitted where deleting is not offered (the Overview summary). */
	onDelete?: (id: string) => void;
	/** Sort this model to the front of each Candidates cell. Set when the
	 *  list is filtered to one model, where the row is read to answer
	 *  "what else ran alongside it". */
	focusModel?: string | null;
}

/**
 * One row per run, shared by the Overview summary and the Runs tab so the
 * same run reads the same way in both places.
 *
 * No elapsed time: `started_at` / `finished_at` are SQLite's timestamp
 * format rather than ISO-8601, so they must not be parsed here.
 */
export function RunTable({ runs, onOpen, onDelete, focusModel }: RunTableProps) {
	return (
		<div className="overflow-x-auto">
			<table className="w-full min-w-[760px] text-sm">
				<thead>
					<tr className="text-left text-xs uppercase tracking-wider text-text-muted">
						<th className="py-2 pr-4 font-medium">Started</th>
						<th className="py-2 pr-4 font-medium">Candidates</th>
						<th className="py-2 pr-4 font-medium text-right">Models</th>
						<th className="py-2 pr-4 font-medium text-right">Samples</th>
						<th className="py-2 pr-4 font-medium text-right">Correct</th>
						<th className="py-2 pr-4 font-medium">Hint</th>
						<th className="py-2 pr-4 font-medium">Version</th>
						<th className="py-2 pr-4 font-medium">Status</th>
						<th className="py-2 w-px" />
					</tr>
				</thead>
				<tbody>
					{runs.map((r) => (
						<tr key={r.id} className="border-t border-border group hover:bg-surface-2">
							{/* A real button, not a click handler on the row: the row
							    has to be reachable by keyboard, and it already holds
							    a second control. The whole cell is the target, so
							    the affordance is a cell rather than eight characters
							    of relative time. */}
							<td className="p-0 whitespace-nowrap">
								<button
									type="button"
									onClick={() => onOpen(r.id)}
									className="flex w-full items-center gap-1.5 py-2 pr-4 text-left text-text-primary group-hover:text-accent cursor-pointer"
								>
									<ChevronRight
										size={14}
										className="text-text-muted group-hover:text-accent shrink-0"
									/>
									{formatRelativeTime(r.started_at)}
								</button>
							</td>
							<td className="py-2 pr-4 font-mono text-xs text-text-secondary max-w-xs truncate">
								{r.model_ids.length > 0 ? describeModels(r.model_ids, focusModel) : "none"}
							</td>
							<td className="py-2 pr-4 text-right font-mono tabular-nums text-xs">
								{r.model_ids.length}
							</td>
							<td className="py-2 pr-4 text-right font-mono tabular-nums text-xs">
								{r.sample_count}
							</td>
							<td className="py-2 pr-4 text-right font-mono tabular-nums text-xs">
								{r.scored > 0 ? (
									`${r.correct}/${r.scored}`
								) : (
									<span className="text-text-muted">—</span>
								)}
							</td>
							<td className="py-2 pr-4 font-mono text-xs text-text-secondary whitespace-nowrap">
								{r.language ?? <span className="text-text-muted">none</span>}
							</td>
							<td className="py-2 pr-4 font-mono text-xs text-text-muted whitespace-nowrap">
								{r.app_version} / tc {r.engine_version}
							</td>
							<td className="py-2 pr-4">
								{/* Cancelled is neither good nor bad news: it is the run
								    doing what it was told. Only "running" earns the
								    attention colour. */}
								<Badge
									variant={
										r.status === "completed"
											? "success"
											: r.status === "failed"
												? "error"
												: r.status === "cancelled"
													? "default"
													: "info"
									}
								>
									{r.status}
								</Badge>
							</td>
							<td className="py-2">
								{onDelete && (
									<Button
										size="sm"
										variant="ghost"
										icon={<Trash2 size={14} />}
										onClick={() => onDelete(r.id)}
									/>
								)}
							</td>
						</tr>
					))}
				</tbody>
			</table>

			{/* A note is prose, not a column: it would blow the row height out
			    for every run that carries one. */}
			{runs.some((r) => r.note) && (
				<dl className="pt-3 space-y-1 text-xs">
					{runs
						.filter((r) => r.note)
						.map((r) => (
							<div key={r.id} className="flex gap-2">
								<dt className="text-text-muted whitespace-nowrap">
									{formatRelativeTime(r.started_at)}
								</dt>
								<dd className="text-text-secondary">{r.note}</dd>
							</div>
						))}
				</dl>
			)}
		</div>
	);
}

/** Names beat a count for a short list; past three, the count carries
 *  more than a truncated string of ids would. */
function describeModels(ids: string[], focus?: string | null): string {
	// When narrowed to one model, lead with it: the row is being read to
	// answer "what else ran alongside it", not "what is in this run".
	const ordered = focus ? [focus, ...ids.filter((m) => m !== focus)] : ids;
	if (ordered.length <= 3) return ordered.join(", ");
	return `${ordered.slice(0, 3).join(", ")} +${ordered.length - 3} more`;
}

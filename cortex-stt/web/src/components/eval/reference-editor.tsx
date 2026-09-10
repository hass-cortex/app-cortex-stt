import { useEffect, useState } from "react";
import type { EvalSampleListEntry } from "@/api/types";
import { Button } from "@/components/ui/button";
import { useToast } from "@/components/ui/toast";
import { useSetReference } from "@/hooks/use-eval";

/**
 * Correcting what a sample says.
 *
 * Unlike the field that first labels a capture, this one starts on the
 * current reference: it is a person's own earlier answer, not a model's
 * guess, so there is nothing here to be tempted into accepting without
 * listening.
 *
 * A run's cells fall back to a comparison against the reference wherever
 * nobody ruled on the output, so a correction re-scores every run this
 * sample appears in. The note says so before the save, because a typo
 * fixed here can move a model's score on a screen you are not looking at.
 */
export function ReferenceEditor({ sample }: { sample: EvalSampleListEntry }) {
	const [value, setValue] = useState(sample.reference_transcript);
	const save = useSetReference();
	const { toast } = useToast();

	// Follow the row: a refetch, or another edit, replaces what is here
	// unless it is being typed into.
	useEffect(() => {
		setValue(sample.reference_transcript);
	}, [sample.reference_transcript]);

	const trimmed = value.trim();
	const changed = trimmed !== sample.reference_transcript && trimmed.length > 0;

	return (
		<div className="space-y-2">
			<label
				htmlFor={`reference-${sample.id}`}
				className="num block text-[10px] tracking-[0.06em] text-text-faint"
			>
				REFERENCE
			</label>
			<div className="flex flex-col sm:flex-row gap-2 sm:items-start">
				<textarea
					id={`reference-${sample.id}`}
					value={value}
					onChange={(e) => setValue(e.target.value)}
					rows={1}
					autoComplete="off"
					className="flex-1 min-w-0 px-3 py-2 rounded-lg bg-surface-2 border border-border text-text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent"
				/>
				<div className="flex gap-2 shrink-0">
					<Button
						size="sm"
						disabled={!changed}
						loading={save.isPending}
						onClick={() =>
							save.mutate(
								{ id: sample.id, reference: trimmed },
								{
									onSuccess: () => toast("Reference updated", "success"),
									onError: (err) => toast(`Could not update: ${err.message}`, "error"),
								},
							)
						}
					>
						Save
					</Button>
					{changed && (
						<Button size="sm" variant="ghost" onClick={() => setValue(sample.reference_transcript)}>
							Revert
						</Button>
					)}
				</div>
			</div>
			{changed && sample.result_count > 0 && (
				<p className="text-xs text-warning">
					{sample.result_count} result(s) across {sample.run_count} run(s) are scored against this
					text; saving re-scores every one of them. Rulings a person made stay as they are.
				</p>
			)}
		</div>
	);
}

import { useEffect, useState } from "react";
import { audioUrl } from "@/api/client";
import type { TranscriptionRecord } from "@/api/types";
import { PlayButton } from "@/components/eval/play-button";
import { ReferenceField } from "@/components/eval/reference-field";
import { Button } from "@/components/ui/button";
import { Modal } from "@/components/ui/modal";
import { useToast } from "@/components/ui/toast";
import { useAddRecordsToEval } from "@/hooks/use-eval";
import { describeCaptureOutcome, formatDuration } from "@/lib/format";
import { formatClock } from "@/utils/time";

interface BatchLabelDialogProps {
	open: boolean;
	onClose: () => void;
	/** The selected recordings the list can still render — only these
	 *  carry the transcript and duration a field needs beside it. */
	records: TranscriptionRecord[];
	/** Selected ids the current filter is hiding. They go in unlabelled:
	 *  the clip is still wanted, there is just nothing to show while
	 *  typing a reference for it. */
	hiddenIds: string[];
	/** Ids the evaluation set already holds; shown as refused up front. */
	taken: Set<string>;
	timezone: string;
	onDone: () => void;
}

/**
 * Bringing a selection into the evaluation set, with the chance to say
 * what each clip actually contains before it goes.
 *
 * Every field starts empty and offers the model's own transcript as one
 * click, for the reason `ReferenceField` carries: a reference typed
 * without listening is a model's mistake promoted to ground truth, and
 * a screen showing twenty of them at once is where that temptation is
 * strongest.
 *
 * A blank row is a legitimate answer — it queues the clip for labelling
 * later — so the whole selection can come in even when only some of it
 * is worth typing now.
 */
export function BatchLabelDialog({
	open,
	onClose,
	records,
	hiddenIds,
	taken,
	timezone,
	onDone,
}: BatchLabelDialogProps) {
	const [references, setReferences] = useState<Record<string, string>>({});
	const add = useAddRecordsToEval();
	const { toast } = useToast();

	// A fresh selection gets fresh fields, never the last batch's text.
	useEffect(() => {
		if (open) setReferences({});
	}, [open]);

	const eligible = records.filter((r) => r.audio_path && !taken.has(r.id));
	const alreadyIn = records.filter((r) => taken.has(r.id)).length;
	const noAudio = records.filter((r) => !r.audio_path).length;
	const typed = eligible.filter((r) => (references[r.id] ?? "").trim()).length;

	const submit = () => {
		add.mutate(
			[
				...eligible.map((r) => ({ recordId: r.id, reference: references[r.id] ?? "" })),
				// Sent too, so clearing the selection afterwards is honest:
				// everything ticked was offered to the server, which decides
				// what it can take.
				...hiddenIds.map((id) => ({ recordId: id, reference: "" })),
			],
			{
				onSuccess: (outcome) => {
					toast(describeCaptureOutcome(outcome), outcome.added === 0 ? "error" : "success");
					onDone();
					onClose();
				},
				onError: (err) => toast(`Could not add: ${err.message}`, "error"),
			},
		);
	};

	return (
		<Modal
			open={open}
			onClose={onClose}
			title="Add to evaluation"
			width="lg"
			footer={
				<div className="flex justify-end gap-2">
					<Button variant="ghost" onClick={onClose}>
						Cancel
					</Button>
					<Button
						disabled={eligible.length + hiddenIds.length === 0}
						loading={add.isPending}
						onClick={submit}
					>
						{typed === 0
							? `Add ${eligible.length + hiddenIds.length} to labelling`
							: `Add ${eligible.length + hiddenIds.length} (${typed} labelled)`}
					</Button>
				</div>
			}
		>
			<div className="space-y-4">
				<p className="text-xs text-text-muted">
					Type what a clip actually says to add it as a sample outright. Leave one blank and it
					joins the labelling queue instead — nothing is lost either way.
				</p>

				{(alreadyIn > 0 || noAudio > 0) && (
					<p className="text-xs text-warning">
						{[
							alreadyIn > 0 ? `${alreadyIn} already in the set` : null,
							noAudio > 0 ? `${noAudio} without audio` : null,
						]
							.filter(Boolean)
							.join(", ")}{" "}
						— skipped.
					</p>
				)}

				{hiddenIds.length > 0 && (
					<p className="text-xs text-warning">
						{hiddenIds.length} more selected but not shown by the current filter. They go in
						unlabelled, to the labelling queue.
					</p>
				)}

				{eligible.length === 0 && hiddenIds.length === 0 ? (
					<p className="py-6 text-center text-sm text-text-muted">
						Nothing here can be taken. Every selected recording is either already in the set or has
						no audio left.
					</p>
				) : (
					<div className="max-h-[26rem] overflow-y-auto divide-y divide-border pr-1">
						{eligible.map((r) => (
							<div key={r.id} className="py-3 first:pt-0 flex gap-2.5">
								<div className="pt-6">
									<PlayButton src={audioUrl(r.id)} />
								</div>
								<div className="flex-1 min-w-0">
									<ReferenceField
										id={`batch-reference-${r.id}`}
										rows={1}
										value={references[r.id] ?? ""}
										onChange={(value) => setReferences((prev) => ({ ...prev, [r.id]: value }))}
										suggestion={r.text || null}
										label={
											<span className="num text-[11px] text-text-muted">
												{formatClock(r.timestamp, timezone)} · {r.model_id} ·{" "}
												{formatDuration(r.audio_duration_ms)}
											</span>
										}
									/>
								</div>
							</div>
						))}
					</div>
				)}
			</div>
		</Modal>
	);
}

import { Mic, SkipForward } from "lucide-react";
import { useEffect, useState } from "react";
import { evalAudioUrl } from "@/api/client";
import type { PendingCapture } from "@/api/types";
import { ClipPlayer } from "@/components/audio/clip-player";
import { ReferenceField } from "@/components/eval/reference-field";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { useDiscardPending, usePending, usePromotePending } from "@/hooks/use-eval";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { clipFileName, formatDuration } from "@/lib/format";

/**
 * The labelling screen: one capture at a time, with a player, a field
 * and a way out.
 *
 * This is where captures land that were taken without a reference — the
 * import picker, an upload, or History's "label later".
 */
export function Labeller() {
	const { data: pending = [], isLoading } = usePending();
	const promote = usePromotePending();
	const discard = useDiscardPending();
	const [reference, setReference] = useState("");

	const current: PendingCapture | undefined = pending[0];

	// A fresh capture gets a fresh field — never the previous one's text.
	useEffect(() => {
		setReference("");
	}, []);

	const runPromote = useMutationToast(promote, {
		success: "Sample added",
		error: "Could not add sample",
	});
	const runDiscard = useMutationToast(discard, {
		success: "Capture skipped",
		error: "Could not skip",
	});

	if (isLoading) return null;

	if (!current) {
		return (
			<EmptyState
				icon={<Mic size={28} />}
				title="Nothing to label"
				description="Add recordings from the picker, from an upload, or with the shortcut on the History page."
			/>
		);
	}

	const done = () => setReference("");

	return (
		<Card>
			<div className="space-y-4">
				<div className="flex items-baseline justify-between gap-3">
					<h3 className="text-sm font-semibold text-text-primary">Waiting to be labelled</h3>
					<span className="text-xs text-text-muted font-mono tabular-nums">
						{pending.length} left
					</span>
				</div>

				<ClipPlayer
					src={evalAudioUrl(current.id)}
					durationMs={current.audio_duration_ms}
					downloadName={clipFileName("capture", current.origin_text, current.id.slice(0, 8))}
				/>

				<div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-text-muted">
					<span>{new Date(current.created_at).toLocaleString()}</span>
					{current.capture_device && <span>{current.capture_device}</span>}
					<span className="font-mono tabular-nums">
						{formatDuration(current.audio_duration_ms)}
					</span>
				</div>

				<ReferenceField
					id="reference-transcript"
					value={reference}
					onChange={setReference}
					suggestion={current.origin_text}
				/>

				<div className="flex justify-between gap-2">
					<Button
						variant="ghost"
						icon={<SkipForward size={14} />}
						loading={discard.isPending}
						onClick={() => {
							runDiscard(current.id);
							done();
						}}
					>
						Skip
					</Button>
					<Button
						disabled={!reference.trim()}
						loading={promote.isPending}
						onClick={() => {
							runPromote({ id: current.id, reference });
							done();
						}}
					>
						Confirm and next
					</Button>
				</div>
			</div>
		</Card>
	);
}

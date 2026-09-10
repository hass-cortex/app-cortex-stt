import { useMemo, useState } from "react";
import type { TranscriptionRecord } from "@/api/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Modal } from "@/components/ui/modal";
import { Select } from "@/components/ui/select";
import { useToast } from "@/components/ui/toast";
import { useAddRecordsToEval, useTakenOrigins } from "@/hooks/use-eval";
import { useHistoryFacets, useHistoryList } from "@/hooks/use-history";
import { describeCaptureOutcome, formatDuration } from "@/lib/format";

/** Pre-0.4.0 recordings are Ogg Opus. The gate is the server's, but
 *  showing why a row is unavailable beats letting it fail on click. */
function isLossless(record: TranscriptionRecord): boolean {
	return !!record.audio_path?.toLowerCase().endsWith(".wav");
}

interface ImportPickerProps {
	open: boolean;
	onClose: () => void;
}

/**
 * Choosing recordings to evaluate, from inside the evaluation domain.
 *
 * This is the picker, not the History page: the place you are choosing
 * is the place that has to show what is already taken and what cannot
 * be taken at all.
 */
export function ImportPicker({ open, onClose }: ImportPickerProps) {
	const [model, setModel] = useState("");
	const [device, setDevice] = useState("");
	const [text, setText] = useState("");
	const [selected, setSelected] = useState<Set<string>>(new Set());

	const { data: facets } = useHistoryFacets();
	const { data: records = [], isLoading } = useHistoryList({
		model: model || undefined,
		capture_device: device || undefined,
		text: text || undefined,
		limit: 300,
	});
	const { data: taken = [] } = useTakenOrigins();
	const takenSet = useMemo(() => new Set(taken), [taken]);
	const capture = useAddRecordsToEval();
	const { toast } = useToast();

	const eligible = records.filter((r) => r.audio_path);

	const toggle = (id: string) => {
		setSelected((prev) => {
			const next = new Set(prev);
			if (next.has(id)) next.delete(id);
			else next.add(id);
			return next;
		});
	};

	const addSelected = async () => {
		const outcome = await capture.mutateAsync(
			[...selected].map((recordId) => ({ recordId, reference: "" })),
		);
		toast(
			describeCaptureOutcome(outcome),
			outcome.added === 0
				? "error"
				: outcome.alreadyTaken + outcome.notLossless + outcome.noAudio + outcome.failed > 0
					? "warning"
					: "success",
		);
		setSelected(new Set());
		onClose();
	};

	return (
		<Modal
			open={open}
			onClose={onClose}
			title="Add from history"
			footer={
				<div className="flex justify-end gap-2">
					<Button variant="ghost" onClick={onClose}>
						Cancel
					</Button>
					<Button disabled={selected.size === 0} loading={capture.isPending} onClick={addSelected}>
						Add {selected.size} to labelling
					</Button>
				</div>
			}
		>
			<div className="space-y-3">
				<div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
					<Select
						value={model}
						onChange={(e) => setModel(e.target.value)}
						placeholder="All models"
						options={(facets?.models ?? []).map((m) => ({ value: m, label: m }))}
					/>
					<Select
						value={device}
						onChange={(e) => setDevice(e.target.value)}
						placeholder="All microphones"
						options={(facets?.capture_devices ?? []).map((d) => ({ value: d, label: d }))}
					/>
					<Input value={text} placeholder="Contains…" onChange={(e) => setText(e.target.value)} />
				</div>

				<div className="max-h-96 overflow-y-auto divide-y divide-border">
					{isLoading && <p className="py-6 text-center text-sm text-text-muted">Loading…</p>}
					{!isLoading && eligible.length === 0 && (
						<p className="py-6 text-center text-sm text-text-muted">
							No recordings with audio match these filters.
						</p>
					)}
					{eligible.map((record) => {
						const already = takenSet.has(record.id);
						const lossy = !isLossless(record);
						const blocked = already || lossy;
						return (
							<button
								type="button"
								key={record.id}
								disabled={blocked}
								onClick={() => toggle(record.id)}
								className={`w-full flex items-center gap-3 py-2 px-1 text-left ${
									blocked ? "opacity-45 cursor-not-allowed" : "cursor-pointer hover:bg-surface-3"
								}`}
							>
								<span className="w-4 text-center font-mono text-xs">
									{blocked ? "⊘" : selected.has(record.id) ? "☑" : "☐"}
								</span>
								<span className="flex-1 min-w-0 truncate text-sm">
									{record.text || <span className="text-text-muted">(empty)</span>}
								</span>
								<span className="text-xs text-text-muted whitespace-nowrap">
									{record.capture_device ?? "—"}
								</span>
								<span className="text-xs text-text-muted font-mono tabular-nums w-14 text-right">
									{formatDuration(record.audio_duration_ms)}
								</span>
								<span className="text-xs text-text-muted w-28 text-right">
									{already ? "In the set" : lossy ? "Lossy audio" : ""}
								</span>
							</button>
						);
					})}
				</div>
			</div>
		</Modal>
	);
}

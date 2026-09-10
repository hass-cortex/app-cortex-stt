import { AlertTriangle, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { useConfirm } from "@/components/ui/confirm";
import { useDeleteAllHistory } from "@/hooks/use-history";
import { useMutationToast } from "@/hooks/use-mutation-toast";

export function DangerZone() {
	const confirm = useConfirm();
	const deleteAllMutation = useDeleteAllHistory();
	const runDeleteAll = useMutationToast(deleteAllMutation, {
		success: (data) =>
			`Deleted ${data.deleted_records} records, ${data.deleted_audio_files} audio files`,
		error: "Delete failed",
	});

	const handleDeleteAll = async () => {
		const ok = await confirm({
			title: "Delete every transcription record?",
			body: "Every row and every audio file goes, not just the ones older than the retention window. Evaluation samples survive — each owns its own copy of the audio.",
			confirmLabel: "Delete everything",
			destructive: true,
		});
		if (ok) runDeleteAll(undefined);
	};

	return (
		<Card className="border-error/30">
			<CardHeader title="Danger Zone" />
			<div className="space-y-4">
				<div className="flex items-start gap-3 p-3 bg-error/5 rounded-lg">
					<AlertTriangle size={16} className="text-error shrink-0 mt-0.5" />
					<div className="space-y-1">
						<p className="text-sm font-medium text-text-primary">Delete All Records</p>
						<p className="text-xs text-text-secondary">
							Permanently delete all transcription records and audio files. This action cannot be
							undone.
						</p>
						<Button
							variant="danger"
							size="sm"
							icon={<Trash2 size={14} />}
							onClick={handleDeleteAll}
							loading={deleteAllMutation.isPending}
							className="mt-2"
						>
							Delete All
						</Button>
					</div>
				</div>
			</div>
		</Card>
	);
}

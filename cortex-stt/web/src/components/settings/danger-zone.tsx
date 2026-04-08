import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { useToast } from "@/components/ui/toast";
import { useDeleteAllHistory } from "@/hooks/use-history";
import { AlertTriangle, Trash2 } from "lucide-react";

export function DangerZone() {
	const deleteAllMutation = useDeleteAllHistory();
	const { toast } = useToast();

	const handleDeleteAll = () => {
		if (
			!window.confirm(
				"Delete ALL transcription records and audio files? This action cannot be undone.",
			)
		) {
			return;
		}
		deleteAllMutation.mutate(undefined, {
			onSuccess: (data) =>
				toast(
					`Deleted ${data.deleted_records} records, ${data.deleted_audio_files} audio files`,
					"success",
				),
			onError: (err) => toast(`Delete failed: ${err.message}`, "error"),
		});
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
							Permanently delete all transcription records and audio files. This action
							cannot be undone.
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

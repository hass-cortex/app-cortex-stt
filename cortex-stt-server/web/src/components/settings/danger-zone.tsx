import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { useToast } from "@/components/ui/toast";
import { useCleanupHistory } from "@/hooks/use-history";
import { AlertTriangle, Trash2 } from "lucide-react";

export function DangerZone() {
	const cleanupMutation = useCleanupHistory();
	const { toast } = useToast();

	const handleCleanup = () => {
		if (!window.confirm("Run retention cleanup now? This deletes old records and audio files.")) {
			return;
		}
		cleanupMutation.mutate(undefined, {
			onSuccess: (data) =>
				toast(
					`Cleanup complete: ${data.deleted_records} records, ${data.deleted_audio_files} audio files deleted`,
					"success",
				),
			onError: (err) => toast(`Cleanup failed: ${err.message}`, "error"),
		});
	};

	return (
		<Card className="border-error/30">
			<CardHeader title="Danger Zone" />
			<div className="space-y-4">
				<div className="flex items-start gap-3 p-3 bg-error/5 rounded-lg">
					<AlertTriangle size={16} className="text-error shrink-0 mt-0.5" />
					<div className="space-y-1">
						<p className="text-sm font-medium text-text-primary">Run Retention Cleanup</p>
						<p className="text-xs text-text-secondary">
							Immediately delete records and audio files that exceed the configured retention
							policy. This action cannot be undone.
						</p>
						<Button
							variant="danger"
							size="sm"
							icon={<Trash2 size={14} />}
							onClick={handleCleanup}
							loading={cleanupMutation.isPending}
							className="mt-2"
						>
							Run Cleanup Now
						</Button>
					</div>
				</div>
			</div>
		</Card>
	);
}

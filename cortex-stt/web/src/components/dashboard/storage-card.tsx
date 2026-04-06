import { Card, CardHeader } from "@/components/ui/card";
import { ProgressBar } from "@/components/ui/progress-bar";
import { Spinner } from "@/components/ui/spinner";
import { useStorageInfo } from "@/hooks/use-system";
import { formatBytes } from "@/lib/format";
import { Database, HardDrive, Mic, Package } from "lucide-react";

export function StorageCard() {
	const { data, isLoading } = useStorageInfo();

	if (isLoading) {
		return (
			<Card>
				<CardHeader title="Storage" />
				<div className="flex justify-center py-8">
					<Spinner />
				</div>
			</Card>
		);
	}

	if (!data) return null;

	const used = data.models_bytes + data.audio_bytes + data.database_bytes;
	const total = used + data.free_bytes;
	const usedPercent = total > 0 ? Math.round((used / total) * 100) : 0;

	const items = [
		{ icon: Package, label: "Models", bytes: data.models_bytes },
		{ icon: Mic, label: "Audio", bytes: data.audio_bytes },
		{ icon: Database, label: "Database", bytes: data.database_bytes },
	];

	return (
		<Card>
			<CardHeader title="Storage" />
			<div className="space-y-3">
				<div className="flex items-center gap-3">
					<HardDrive size={16} className="text-text-muted shrink-0" />
					<div className="flex-1">
						<div className="flex items-center justify-between text-sm mb-1">
							<span className="text-text-primary">{formatBytes(used)} used</span>
							<span className="text-text-muted">{formatBytes(data.free_bytes)} free</span>
						</div>
						<ProgressBar
							value={usedPercent}
							variant={usedPercent > 90 ? "error" : usedPercent > 70 ? "warning" : "success"}
							size="sm"
						/>
					</div>
				</div>

				<div className="space-y-2 pt-1">
					{items.map(({ icon: Icon, label, bytes }) => (
						<div key={label} className="flex items-center justify-between text-xs">
							<div className="flex items-center gap-2 text-text-secondary">
								<Icon size={12} className="text-text-muted" />
								{label}
							</div>
							<span className="text-text-primary">{formatBytes(bytes)}</span>
						</div>
					))}
				</div>
			</div>
		</Card>
	);
}

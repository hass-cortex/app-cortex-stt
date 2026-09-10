import { Badge } from "@/components/ui/badge";
import { Card, CardHeader } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { useEngineStatus } from "@/hooks/use-engine";
import { useSettings } from "@/hooks/use-settings";
import { useSystemInfo } from "@/hooks/use-system";
import { formatMB } from "@/lib/format";

export function EngineCard() {
	const { data: engine, isLoading } = useEngineStatus();
	const { data: settings } = useSettings();
	const { data: system } = useSystemInfo();

	if (isLoading) {
		return (
			<Card>
				<CardHeader title="Engine" />
				<div className="flex justify-center py-8">
					<Spinner />
				</div>
			</Card>
		);
	}

	const loaded = engine?.loaded_models ?? [];
	const defaultModel = settings?.default_model;
	const ramUsed = system ? system.total_memory_mb - system.available_memory_mb : null;
	const ramPercent =
		system && system.total_memory_mb > 0 && ramUsed !== null
			? (ramUsed / system.total_memory_mb) * 100
			: 0;

	return (
		<Card>
			<CardHeader
				title="Engine"
				action={
					<Badge variant={loaded.length > 0 ? "success" : "default"}>{loaded.length} loaded</Badge>
				}
			/>

			<div className="mt-3 space-y-2">
				{loaded.length === 0 && (
					<p className="text-[12px] text-text-muted">
						Nothing resident. The first request loads the default model.
					</p>
				)}
				{loaded.map((modelId) => (
					<div key={modelId} className="flex items-center gap-2.5">
						<span className="w-[7px] h-[7px] rounded-full bg-success shrink-0" />
						<span className="text-[12.5px] text-text-primary truncate">{modelId}</span>
						{modelId === defaultModel && <Badge variant="accent">default</Badge>}
					</div>
				))}
				{defaultModel && !loaded.includes(defaultModel) && (
					<div className="flex items-center gap-2.5">
						<span className="w-[7px] h-[7px] rounded-full border border-border shrink-0" />
						<span className="text-[12.5px] text-text-secondary truncate">{defaultModel}</span>
						<Badge variant="accent">default</Badge>
					</div>
				)}
			</div>

			{system && ramUsed !== null && (
				<div className="mt-3 pt-3 border-t border-border-soft">
					<div className="num flex justify-between text-[11px] text-text-muted">
						<span>resident memory</span>
						<span className="text-text-primary">
							{formatMB(ramUsed)} / {formatMB(system.total_memory_mb)}
						</span>
					</div>
					<div className="mt-[7px] flex h-1.5 gap-0.5">
						<div className="bg-accent rounded-[3px]" style={{ width: `${ramPercent}%` }} />
						<div className="flex-1 bg-surface-3 rounded-[3px]" />
					</div>
				</div>
			)}
		</Card>
	);
}

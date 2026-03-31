import type { ComputeDevice } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useToast } from "@/components/ui/toast";
import { useLoadModel, useUnloadModel } from "@/hooks/use-engine";
import { useModels } from "@/hooks/use-models";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { Play, PowerOff, Save } from "lucide-react";
import { useState } from "react";

interface ModelLifecycleProps {
	loadedModels: string[];
}

export function ModelLifecycle({ loadedModels }: ModelLifecycleProps) {
	const { data: models } = useModels();
	const { data: settings } = useSettings();
	const loadMutation = useLoadModel();
	const unloadMutation = useUnloadModel();
	const updateSettingsMutation = useUpdateSettings();
	const { toast } = useToast();

	const [loadModelId, setLoadModelId] = useState("");
	const [loadPoolSize, setLoadPoolSize] = useState("1");

	const currentIdle = settings?.idle_timeout_secs;
	const [keepLoaded, setKeepLoaded] = useState(currentIdle === null || currentIdle === undefined);
	const [idleTimeout, setIdleTimeout] = useState(String(currentIdle ?? 300));
	const [maxLoaded, setMaxLoaded] = useState(String(settings?.max_loaded_models ?? 3));

	const downloadedModels = (models ?? []).filter(
		(m) => m.status === "downloaded" || m.status === "loaded" || m.status === "loading",
	);

	const modelOptions = downloadedModels.map((m) => ({
		value: m.id,
		label: m.name,
	}));

	const handleLoad = () => {
		if (!loadModelId) return;
		const poolSize = Number.parseInt(loadPoolSize, 10);
		loadMutation.mutate(
			{ modelId: loadModelId, poolSize: poolSize > 0 ? poolSize : undefined },
			{
				onSuccess: () => toast(`Loading ${loadModelId}...`, "info"),
				onError: (err) => toast(`Failed: ${err.message}`, "error"),
			},
		);
	};

	const handleSaveLifecycle = () => {
		updateSettingsMutation.mutate(
			{
				idle_timeout_secs: keepLoaded ? null : Number.parseInt(idleTimeout, 10),
				max_loaded_models: Number.parseInt(maxLoaded, 10) || 1,
			},
			{
				onSuccess: () => toast("Lifecycle settings saved", "success"),
				onError: (err) => toast(`Failed: ${err.message}`, "error"),
			},
		);
	};

	return (
		<Card>
			<CardHeader title="Model Lifecycle" description="Control how models are loaded and unloaded" />
			<div className="space-y-5">
				{/* Loaded models */}
				{loadedModels.length > 0 && (
					<div className="space-y-2">
						<span className="text-sm font-medium text-text-secondary">
							Loaded ({loadedModels.length})
						</span>
						<p className="text-xs text-text-muted">Device changes take effect on next model load.</p>
						<div className="space-y-2">
							{loadedModels.map((modelId) => (
								<div key={modelId} className="flex items-center justify-between p-2.5 bg-surface-3 rounded-lg">
									<div className="flex items-center gap-2 min-w-0">
										<span className="text-sm font-medium text-text-primary truncate">
											{modelId}
										</span>
										<Badge variant="success">loaded</Badge>
									</div>
									<div className="flex items-center gap-2 shrink-0">
										<Select
											options={[
												{ value: "auto", label: "Auto" },
												{ value: "cpu", label: "CPU" },
												{ value: "gpu", label: "GPU" },
											]}
											value={settings?.device_overrides?.[modelId] ?? "auto"}
											onChange={(e) => {
												const overrides = { ...settings?.device_overrides, [modelId]: e.target.value as ComputeDevice };
												updateSettingsMutation.mutate(
													{ device_overrides: overrides },
													{
														onSuccess: () => toast("Device setting saved. Reload model to apply.", "info"),
														onError: (err) => toast(`Failed: ${err.message}`, "error"),
													},
												);
											}}
											className="w-24"
										/>
										<Button
											size="sm"
											variant="ghost"
											icon={<PowerOff size={14} />}
											onClick={() =>
												unloadMutation.mutate(modelId, {
													onSuccess: () => toast(`${modelId} unloaded`, "success"),
													onError: (err) => toast(`Unload failed: ${err.message}`, "error"),
												})
											}
											loading={unloadMutation.isPending}
											title="Unload model"
										/>
									</div>
								</div>
							))}
						</div>
					</div>
				)}

				{/* Manual load */}
				<div className="space-y-2">
					<span className="text-sm font-medium text-text-secondary">Load Model</span>
					<div className="flex items-center gap-2">
						<Select
							options={modelOptions}
							value={loadModelId}
							onChange={(e) => setLoadModelId(e.target.value)}
							placeholder="Select model"
							className="flex-1"
						/>
						<Input
							value={loadPoolSize}
							onChange={(e) => setLoadPoolSize(e.target.value)}
							type="number"
							min="1"
							max="8"
							className="w-20"
							placeholder="Pool"
						/>
						<Button
							icon={<Play size={14} />}
							onClick={handleLoad}
							loading={loadMutation.isPending}
							disabled={!loadModelId}
						>
							Load
						</Button>
					</div>
				</div>

				<hr className="border-border" />

				{/* Idle timeout */}
				<div className="space-y-2">
					<label className="flex items-center gap-2 cursor-pointer">
						<input
							type="checkbox"
							checked={keepLoaded}
							onChange={(e) => setKeepLoaded(e.target.checked)}
							className="rounded border-border"
						/>
						<span className="text-sm text-text-primary">Keep models loaded (no auto-unload)</span>
					</label>
					{!keepLoaded && (
						<Input
							label="Idle timeout (seconds)"
							type="number"
							min="30"
							max="3600"
							value={idleTimeout}
							onChange={(e) => setIdleTimeout(e.target.value)}
						/>
					)}
					<p className="text-xs text-text-muted">
						{keepLoaded
							? "Models stay in memory until manually unloaded or server restart."
							: "Models are unloaded after being idle for this duration."}
					</p>
					<Input
						label="Max loaded models"
						type="number"
						min="1"
						max="10"
						value={maxLoaded}
						onChange={(e) => setMaxLoaded(e.target.value)}
					/>
					<p className="text-xs text-text-muted">
						Maximum models in memory simultaneously. Oldest model is unloaded when limit is reached.
					</p>
					<Button
						size="sm"
						icon={<Save size={14} />}
						onClick={handleSaveLifecycle}
						loading={updateSettingsMutation.isPending}
					>
						Save
					</Button>
				</div>
			</div>
		</Card>
	);
}

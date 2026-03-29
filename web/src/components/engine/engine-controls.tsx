import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useToast } from "@/components/ui/toast";
import { useLoadModel, useSetDefaultModel } from "@/hooks/use-engine";
import { useModels } from "@/hooks/use-models";
import { Play } from "lucide-react";
import { useState } from "react";

interface EngineControlsProps {
	defaultModel: string;
}

export function EngineControls({ defaultModel }: EngineControlsProps) {
	const { data: models } = useModels();
	const setDefaultMutation = useSetDefaultModel();
	const loadMutation = useLoadModel();
	const { toast } = useToast();

	const [selectedDefault, setSelectedDefault] = useState(defaultModel);
	const [loadModelId, setLoadModelId] = useState("");
	const [loadPoolSize, setLoadPoolSize] = useState("1");

	const downloadedModels = (models ?? []).filter(
		(m) => m.status === "downloaded" || m.status === "loaded" || m.status === "loading",
	);

	const modelOptions = downloadedModels.map((m) => ({
		value: m.id,
		label: m.name,
	}));

	const handleSetDefault = () => {
		setDefaultMutation.mutate(selectedDefault, {
			onSuccess: () => toast("Default model updated", "success"),
			onError: (err) => toast(`Failed: ${err.message}`, "error"),
		});
	};

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

	return (
		<Card>
			<CardHeader title="Engine Controls" />
			<div className="space-y-5">
				{/* Default model selection */}
				<div className="space-y-2">
					<span className="text-sm font-medium text-text-secondary">Default Model</span>
					<p className="text-xs text-text-muted">
						Used for HTTP requests without an explicit model parameter.
					</p>
					<div className="flex items-center gap-2">
						<Select
							options={modelOptions}
							value={selectedDefault}
							onChange={(e) => setSelectedDefault(e.target.value)}
							placeholder="Select model"
							className="flex-1"
						/>
						<Button
							onClick={handleSetDefault}
							loading={setDefaultMutation.isPending}
							disabled={selectedDefault === defaultModel}
						>
							Apply
						</Button>
					</div>
					{selectedDefault === defaultModel && <Badge variant="success">Current default</Badge>}
				</div>

				<hr className="border-border" />

				{/* Pre-load model */}
				<div className="space-y-2">
					<span className="text-sm font-medium text-text-secondary">Pre-load Model</span>
					<p className="text-xs text-text-muted">
						Load a model into memory before any requests arrive, with optional pool size.
					</p>
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
			</div>
		</Card>
	);
}

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { useModels, useScanCustomModels } from "@/hooks/use-models";
import { Package, RefreshCw } from "lucide-react";
import { useState } from "react";
import { ModelCard } from "./model-card";

const engineOptions = [
	{ value: "", label: "All engines" },
	{ value: "Whisper", label: "Whisper" },
	{ value: "Parakeet", label: "Parakeet" },
	{ value: "SenseVoice", label: "SenseVoice" },
	{ value: "GigaAM", label: "GigaAM" },
	{ value: "Moonshine", label: "Moonshine" },
	{ value: "Canary", label: "Canary" },
	{ value: "CohereTranscribe", label: "Cohere" },
];

function isDownloaded(status: string): boolean {
	return status === "downloaded" || status === "loaded" || status === "loading" || status === "custom";
}

function isInProgress(status: string): boolean {
	return status === "downloading" || status === "queued";
}

export function ModelList() {
	const { data: models, isLoading, error } = useModels();
	const scanMutation = useScanCustomModels();
	const [search, setSearch] = useState("");
	const [engineFilter, setEngineFilter] = useState("");

	if (isLoading) {
		return (
			<div className="flex justify-center py-16">
				<Spinner size="lg" />
			</div>
		);
	}

	if (error) {
		return (
			<EmptyState
				icon={<Package size={40} />}
				title="Failed to load models"
				description={error.message}
			/>
		);
	}

	const filtered = (models ?? []).filter((m) => {
		if (
			search &&
			!m.name.toLowerCase().includes(search.toLowerCase()) &&
			!m.id.toLowerCase().includes(search.toLowerCase())
		) {
			return false;
		}
		if (engineFilter && m.engine_type !== engineFilter) return false;
		return true;
	});

	const downloaded = filtered.filter((m) => isDownloaded(m.status) || isInProgress(m.status));
	const available = filtered.filter((m) => !isDownloaded(m.status) && !isInProgress(m.status));

	return (
		<div className="space-y-6">
			{/* Filters */}
			<div className="flex flex-col sm:flex-row gap-3">
				<div className="flex-1">
					<Input
						placeholder="Search models..."
						value={search}
						onChange={(e) => setSearch(e.target.value)}
					/>
				</div>
				<Select
					options={engineOptions}
					value={engineFilter}
					onChange={(e) => setEngineFilter(e.target.value)}
					className="sm:w-40"
				/>
				<Button
					variant="secondary"
					size="md"
					icon={<RefreshCw size={14} />}
					onClick={() => scanMutation.mutate()}
					loading={scanMutation.isPending}
				>
					Scan
				</Button>
			</div>

			{filtered.length === 0 ? (
				<EmptyState
					icon={<Package size={40} />}
					title="No models found"
					description="Try adjusting your search or filters."
				/>
			) : (
				<>
					{/* Downloaded models */}
					{downloaded.length > 0 && (
						<div className="space-y-3">
							<h2 className="text-sm font-semibold text-text-secondary">
								Downloaded ({downloaded.length})
							</h2>
							<div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
								{downloaded.map((model) => (
									<ModelCard key={model.id} model={model} />
								))}
							</div>
						</div>
					)}

					{/* Available models */}
					{available.length > 0 && (
						<div className="space-y-3">
							<h2 className="text-sm font-semibold text-text-secondary">
								Available ({available.length})
							</h2>
							<div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
								{available.map((model) => (
									<ModelCard key={model.id} model={model} />
								))}
							</div>
						</div>
					)}
				</>
			)}
		</div>
	);
}

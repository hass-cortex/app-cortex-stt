import { Package } from "lucide-react";
import { useMemo, useState } from "react";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { useModels } from "@/hooks/use-models";
import { ModelCard } from "./model-card";

/** Title-case a family slug for display (e.g. "sensevoice" → "Sensevoice"). */
function familyLabel(family: string): string {
	return family.charAt(0).toUpperCase() + family.slice(1);
}

function isDownloaded(status: string): boolean {
	return status === "downloaded" || status === "custom";
}

function isInProgress(status: string): boolean {
	return status === "downloading" || status === "queued";
}

export function ModelList() {
	const { data: models, isLoading, error } = useModels();
	const [search, setSearch] = useState("");
	const [familyFilter, setFamilyFilter] = useState("");
	const [languageFilter, setLanguageFilter] = useState("");

	const familyOptions = useMemo(() => {
		const families = new Set<string>();
		for (const m of models ?? []) families.add(m.family);
		return [
			{ value: "", label: "All families" },
			...Array.from(families)
				.sort()
				.map((f) => ({ value: f, label: familyLabel(f) })),
		];
	}, [models]);

	const languageOptions = useMemo(() => {
		const langs = new Set<string>();
		for (const m of models ?? []) {
			for (const l of m.languages) langs.add(l);
		}
		return [
			{ value: "", label: "All languages" },
			...Array.from(langs)
				.sort()
				.map((l) => ({ value: l, label: l })),
		];
	}, [models]);

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
		if (familyFilter && m.family !== familyFilter) return false;
		if (languageFilter && !m.languages.includes(languageFilter)) return false;
		return true;
	});

	const loaded = filtered.filter((m) => m.is_loaded);
	const downloaded = filtered.filter(
		(m) => (isDownloaded(m.status) || isInProgress(m.status)) && !m.is_loaded,
	);
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
					options={familyOptions}
					value={familyFilter}
					onChange={(e) => setFamilyFilter(e.target.value)}
					className="sm:w-40"
				/>
				<Select
					options={languageOptions}
					value={languageFilter}
					onChange={(e) => setLanguageFilter(e.target.value)}
					className="sm:w-40"
				/>
			</div>

			{filtered.length === 0 ? (
				<EmptyState
					icon={<Package size={40} />}
					title="No models found"
					description="Try adjusting your search or filters."
				/>
			) : (
				<>
					{/* Loaded models */}
					{loaded.length > 0 && (
						<div className="space-y-3">
							<h2 className="text-sm font-semibold text-text-secondary">
								Loaded ({loaded.length})
							</h2>
							<div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
								{loaded.map((model) => (
									<ModelCard key={model.id} model={model} />
								))}
							</div>
						</div>
					)}

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

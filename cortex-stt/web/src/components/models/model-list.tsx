import { Package, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { EmptyState } from "@/components/ui/empty-state";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { useModels } from "@/hooks/use-models";
import { useMeasuredLatency } from "@/hooks/use-stats";
import { InstalledModelHead, InstalledModelRow } from "./installed-model-row";
import { CatalogHead, ModelRow } from "./model-row";

// The catalog's `recommended` flag (Handy's `curated`) is not shown. It is
// Handy's opinion, it cannot be checked here, and several curated models
// (parakeet-unified-en-0.6b, canary-180m-flash) cannot transcribe Chinese
// at all. The language filter is the honest way to narrow the catalog.

function isOnDisk(status: string): boolean {
	return status === "downloaded" || status === "custom";
}

function isInProgress(status: string): boolean {
	return status === "downloading" || status === "queued";
}

function Section({
	title,
	note,
	count,
	head,
	spaced = false,
	children,
}: {
	title: string;
	note?: string;
	count: string;
	head: React.ReactNode;
	/** Children are cards, set apart rather than ruled. */
	spaced?: boolean;
	children: React.ReactNode;
}) {
	return (
		<div>
			<div className="flex items-baseline gap-2.5 mb-2">
				<h2 className="text-[13px] font-semibold text-text-primary whitespace-nowrap">{title}</h2>
				<span className="num text-[11px] text-text-faint min-w-0">
					{count}
					{note ? ` · ${note}` : ""}
				</span>
			</div>
			{head}
			<div className={`flex flex-col ${spaced ? "gap-2.5" : "gap-px"}`}>{children}</div>
		</div>
	);
}

export function ModelList() {
	const { data: models, isLoading, error } = useModels();
	const { byVariant } = useMeasuredLatency(models);
	const [search, setSearch] = useState("");
	const [familyFilter, setFamilyFilter] = useState("");
	const [languageFilter, setLanguageFilter] = useState("");
	const [installedOnly, setInstalledOnly] = useState(false);

	const familyOptions = useMemo(() => {
		const families = new Set<string>();
		for (const m of models ?? []) families.add(m.family);
		return [
			{ value: "", label: "All families" },
			...Array.from(families)
				.sort()
				.map((f) => ({ value: f, label: f })),
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

	const needle = search.toLowerCase();
	const filtered = (models ?? []).filter((m) => {
		if (needle && !m.name.toLowerCase().includes(needle) && !m.id.toLowerCase().includes(needle)) {
			return false;
		}
		if (familyFilter && m.family !== familyFilter) return false;
		if (languageFilter && !m.languages.includes(languageFilter)) return false;
		if (installedOnly && !isOnDisk(m.status) && !isInProgress(m.status)) return false;
		return true;
	});

	const inProgress = filtered.filter((m) => isInProgress(m.status));
	const onDisk = filtered
		.filter((m) => isOnDisk(m.status) && !isInProgress(m.status))
		.sort((a, b) => Number(b.is_loaded) - Number(a.is_loaded) || a.name.localeCompare(b.name));
	const catalog = filtered.filter((m) => !isOnDisk(m.status) && !isInProgress(m.status));
	const installedCount = onDisk.reduce(
		(n, m) => n + m.variants.filter((v) => v.status === "downloaded").length,
		0,
	);

	return (
		<div className="flex flex-col gap-5">
			<div className="flex flex-col sm:flex-row gap-2.5">
				<div className="flex-1 flex items-center gap-2.5 h-9 px-3 bg-surface-2 border border-border rounded-lg">
					<Search size={15} strokeWidth={1.8} className="text-text-muted shrink-0" />
					<input
						value={search}
						onChange={(e) => setSearch(e.target.value)}
						placeholder={`Search ${models?.length ?? 0} models by name, family or language…`}
						className="w-full bg-transparent text-[12.5px] text-text-primary placeholder:text-text-muted focus:outline-none"
					/>
				</div>
				<Select
					options={familyOptions}
					value={familyFilter}
					onChange={(e) => setFamilyFilter(e.target.value)}
					className="sm:w-36"
				/>
				<Select
					options={languageOptions}
					value={languageFilter}
					onChange={(e) => setLanguageFilter(e.target.value)}
					className="sm:w-36"
				/>
				<button
					type="button"
					onClick={() => setInstalledOnly((v) => !v)}
					className={`h-9 px-3 rounded-lg text-[12px] border transition-colors cursor-pointer whitespace-nowrap ${
						installedOnly
							? "bg-accent-wash border-accent-quiet text-text-primary"
							: "bg-surface-2 border-border text-text-muted hover:text-text-primary"
					}`}
				>
					On disk only
				</button>
			</div>

			{filtered.length === 0 ? (
				<EmptyState
					icon={<Package size={40} />}
					title="No models found"
					description="Try adjusting the search or filters."
				/>
			) : (
				<>
					{inProgress.length > 0 && (
						<Section title="Downloading" count={`${inProgress.length}`} head={<CatalogHead />}>
							{inProgress.map((model) => (
								<ModelRow key={model.id} model={model} />
							))}
						</Section>
					)}
					{onDisk.length > 0 && (
						<Section
							title="On disk"
							count={`${onDisk.length} models · ${installedCount} quants`}
							note="p50 measured from your own history, last 7 days, per quant"
							head={<InstalledModelHead />}
							spaced
						>
							{onDisk.map((model) => (
								<InstalledModelRow key={model.id} model={model} measured={byVariant} />
							))}
						</Section>
					)}
					{catalog.length > 0 && (
						<Section
							title="Catalog"
							count={`${catalog.length}`}
							note="not on disk"
							head={<CatalogHead />}
						>
							{catalog.map((model) => (
								<ModelRow key={model.id} model={model} />
							))}
						</Section>
					)}
				</>
			)}
		</div>
	);
}

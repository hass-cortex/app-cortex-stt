import { ModelList } from "@/components/models/model-list";

export function ModelsPage() {
	return (
		<div className="space-y-6">
			<div>
				<h1 className="text-xl font-bold text-text-primary">Models</h1>
				<p className="text-sm text-text-secondary mt-1">
					Browse, download, and manage speech-to-text models
				</p>
			</div>
			<ModelList />
		</div>
	);
}

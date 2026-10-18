import { useModels } from "@/hooks/use-models";

/** Resolve exact identities only. Historical IDs absent from the catalog
 * remain visible verbatim; never guess a quant from today's default. */
export function useModelLabel() {
	const { data: models = [] } = useModels();
	const names = new Map(models.map((model) => [model.id, model.name]));
	for (const model of models) {
		if (model.catalog_id && model.quant) {
			const suffix = ` · ${model.quant}`;
			const baseName = model.name.endsWith(suffix)
				? model.name.slice(0, -suffix.length)
				: model.catalog_id;
			names.set(model.catalog_id, `${baseName} · quant unknown`);
		}
	}
	return (id: string) => names.get(id) ?? id;
}

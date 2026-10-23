/** The server's key for a Variant (`model:quant`, or the bare id of a
 *  custom model) — how evaluation names a candidate. */
export function variantKey(modelId: string, quant: string | null): string {
	return quant ? `${modelId}:${quant}` : modelId;
}

/** A Variant key split back into its model and quant. */
export function splitKey(key: string): { model: string; quant: string | null } {
	const at = key.indexOf(":");
	return at < 0
		? { model: key, quant: null }
		: { model: key.slice(0, at), quant: key.slice(at + 1) };
}

/** How a Variant reads beside its model's name. */
export function variantName(quant: string | null): string {
	return quant ?? "custom";
}

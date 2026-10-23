/** The server's key for a Variant (`model:quant`, or the bare id of a
 *  custom model) — how evaluation names a candidate. */
export function variantKey(modelId: string, quant: string | null): string {
	return quant ? `${modelId}:${quant}` : modelId;
}

/** How a Variant reads beside its model's name. */
export function variantName(quant: string | null): string {
	return quant ?? "custom";
}

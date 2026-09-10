import type { ModelSummary } from "@/api/types";
import { Hint } from "@/components/ui/hint";
import { formatBytes } from "@/lib/format";

/**
 * Two figures, never one.
 *
 * A run measures a candidate alone, but the service keeps
 * `max_loaded_models` of them resident. Reporting only the solo figure
 * answers "can it load" when the question being asked is "can I make it
 * the default" — a model can pass the first and fail the second.
 */
export function MemoryFigure({ model }: { model: ModelSummary }) {
	if (model.load_failed) {
		return <span className="text-text-muted">—</span>;
	}
	return (
		<Hint
			label="Memory"
			content="Resident alone / needed alongside the rest of the working set"
			width={240}
			className={`font-mono tabular-nums ${model.fits_alongside === false ? "text-error" : ""}`}
		>
			{formatBytes(model.resident_bytes)} <span className="text-text-muted">/</span>{" "}
			{formatBytes(model.coexist_bytes)}
		</Hint>
	);
}

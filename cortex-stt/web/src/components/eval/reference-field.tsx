import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";

interface ReferenceFieldProps {
	/** Distinct per instance: History renders one of these per record. */
	id: string;
	value: string;
	onChange: (value: string) => void;
	/** What a model transcribed at the time. Offered, never applied. */
	suggestion?: string | null;
	rows?: number;
	/** Names the clip when several of these are stacked; the default suits
	 *  a screen showing one at a time. */
	label?: ReactNode;
}

/**
 * The one place a reference transcript is typed.
 *
 * The field starts empty on purpose. Pre-filling it with the model's
 * transcript looks helpful but makes "save without listening" free —
 * and that is precisely how a model's mistake becomes the reference
 * transcript everything else is scored against. One click is a cheap
 * price for keeping that deliberate, so the transcript is offered as a
 * button instead.
 *
 * That rule has to hold wherever the typing happens, which is why this
 * is a component and not markup repeated per screen: History puts the
 * field directly under the model's own transcript, the most tempting
 * place in the app to quietly pre-fill it.
 */
export function ReferenceField({
	id,
	value,
	onChange,
	suggestion,
	rows = 2,
	label = "What was actually said",
}: ReferenceFieldProps) {
	return (
		<div className="space-y-2">
			<label htmlFor={id} className="block text-sm font-medium text-text-secondary">
				{label}
			</label>
			<textarea
				id={id}
				value={value}
				onChange={(e) => onChange(e.target.value)}
				rows={rows}
				autoComplete="off"
				className="w-full px-3 py-2 rounded-lg bg-surface-2 border border-border text-text-primary text-base focus:outline-none focus:ring-2 focus:ring-accent"
			/>
			{suggestion && (
				<div className="flex items-center gap-2 flex-wrap">
					<span className="text-xs text-text-muted">
						Transcribed at the time:{" "}
						<span className="font-mono text-text-secondary">{suggestion}</span>
					</span>
					<Button size="sm" variant="ghost" onClick={() => onChange(suggestion)}>
						Use this
					</Button>
				</div>
			)}
		</div>
	);
}

import { Badge } from "@/components/ui/badge";

interface RawOutputProps {
	text: string;
	className?: string;
}

/**
 * Raw model output — what the model emitted before its family's
 * post-processing cleaned it up. A record archives it losslessly, tags
 * and all; the evaluation grid deliberately keeps a narrower thing, and
 * neither store decides for the other.
 *
 * The record only carries this when it differs from the transcript, so
 * the badge states the reason it is on screen at all.
 */
export function RawOutput({ text, className = "" }: RawOutputProps) {
	return (
		<div className={`px-3 py-2.5 bg-surface-3 rounded-lg select-text ${className}`}>
			<div className="flex items-center gap-2">
				<span className="num text-[10px] tracking-[0.06em] text-text-faint">RAW MODEL OUTPUT</span>
				<Badge variant="warning">differs from transcript</Badge>
			</div>
			<p className="num mt-1.5 text-[12px] leading-relaxed text-text-secondary break-all">{text}</p>
		</div>
	);
}

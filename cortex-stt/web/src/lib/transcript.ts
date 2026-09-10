/**
 * Punctuation a model may put at the end of an utterance.
 *
 * Display only — whether two transcripts say the same thing is decided
 * on the server (see `src/eval/compare.rs`), so that the grid and the
 * summary figures beside it cannot disagree.
 *
 * Both widths of every mark: the same model returns `?` or `？` depending
 * on the language hint, and neither is a transcription error.
 */
const TRAILING_PUNCTUATION = /[\s。．.，,、；;：:？?！!…⋯～~]+$/u;

export function withoutTrailingPunctuation(text: string): string {
	return text.replace(TRAILING_PUNCTUATION, "");
}

export type DiffKind = "same" | "added" | "removed";

export interface DiffPart {
	text: string;
	kind: DiffKind;
}

/** Past this length the table costs more than the highlight is worth —
 *  and an utterance that long is read, not scanned. */
const MAX_DIFF_CHARS = 400;

/**
 * Character-level difference between a reference and one output.
 *
 * Characters, not words: the references here are Chinese, where a word
 * boundary is itself a guess, and a one-character substitution is the
 * usual failure. `added` is what the model put there, `removed` is what
 * the reference has and the output does not.
 */
export function diffCharacters(reference: string, output: string): DiffPart[] {
	const a = [...reference];
	const b = [...output];
	if (a.length > MAX_DIFF_CHARS || b.length > MAX_DIFF_CHARS) {
		return [{ text: output, kind: "same" }];
	}

	// Longest common subsequence, then walk it back into runs.
	const lcs = new Uint16Array((a.length + 1) * (b.length + 1));
	const at = (i: number, j: number) => i * (b.length + 1) + j;
	const common = (i: number, j: number) => lcs[at(i, j)] ?? 0;
	for (let i = a.length - 1; i >= 0; i--) {
		for (let j = b.length - 1; j >= 0; j--) {
			lcs[at(i, j)] =
				a[i] === b[j] ? common(i + 1, j + 1) + 1 : Math.max(common(i + 1, j), common(i, j + 1));
		}
	}

	const parts: DiffPart[] = [];
	const push = (text: string, kind: DiffKind) => {
		const last = parts[parts.length - 1];
		if (last?.kind === kind) last.text += text;
		else parts.push({ text, kind });
	};

	let i = 0;
	let j = 0;
	while (i < a.length && j < b.length) {
		if (a[i] === b[j]) {
			push(b[j] as string, "same");
			i++;
			j++;
		} else if (common(i + 1, j) >= common(i, j + 1)) {
			push(a[i] as string, "removed");
			i++;
		} else {
			push(b[j] as string, "added");
			j++;
		}
	}
	while (i < a.length) push(a[i++] as string, "removed");
	while (j < b.length) push(b[j++] as string, "added");

	return parts;
}

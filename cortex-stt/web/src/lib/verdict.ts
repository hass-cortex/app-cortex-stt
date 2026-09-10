import type { EvalResult } from "@/api/types";

/**
 * The verdict standing over one cell, and who put it there.
 *
 * Both halves come from the server, which is also what scores the
 * summary tables: a second implementation of the comparison here would
 * eventually disagree with those figures about the same cell.
 *
 * `confirmed` is not decoration. A rule's word and a person's are worth
 * different amounts, and a screen that renders them identically invites
 * a reading of "31/31" that nobody has actually checked.
 */
export interface Verdict {
	correct: boolean;
	confirmed: boolean;
}

export function verdictOf(result: EvalResult): Verdict {
	return { correct: result.ruling ?? result.matches_reference, confirmed: result.ruling !== null };
}

import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { ApiClientError, subscribeSSE } from "@/api/client";
import {
	cancelRun,
	captureRecord,
	clearJudgement,
	deleteRun,
	deleteSample,
	deleteSamples,
	discardPending,
	getComposition,
	getEvalOverview,
	getModelRuns,
	getRunSummary,
	listPending,
	listResults,
	listRuns,
	listSamples,
	listTakenOrigins,
	promotePending,
	putJudgement,
	setReference,
	setRunNote,
	startRun,
	uploadCapture,
} from "@/api/eval";
import type { EvalResultFilters, RunProgress } from "@/api/types";
import { useInvalidatingMutation } from "@/hooks/use-invalidating-mutation";
import { queryKeys } from "@/lib/constants";

/** Anything that changes the sample set also changes the overview's
 *  counts and the picker's "already taken" set. */
const SAMPLE_INVALIDATES = [
	queryKeys.eval.samples(),
	queryKeys.eval.pending(),
	queryKeys.eval.taken(),
	queryKeys.eval.overview(),
];

/** A ruling has no run of its own — it re-scores every run that ever
 *  produced that string, so invalidate the whole eval tree. */
const JUDGEMENT_INVALIDATES = [queryKeys.eval.all];

/** Deep enough for the Runs tab, the deepest consumer. */
const RUN_LIST_LIMIT = 50;

export function useEvalOverview() {
	return useQuery({ queryKey: queryKeys.eval.overview(), queryFn: getEvalOverview });
}

export function useComposition() {
	return useQuery({ queryKey: [...queryKeys.eval.all, "composition"], queryFn: getComposition });
}

export function useSamples() {
	return useQuery({ queryKey: queryKeys.eval.samples(), queryFn: listSamples });
}

export function usePending() {
	return useQuery({ queryKey: queryKeys.eval.pending(), queryFn: listPending });
}

export function useTakenOrigins() {
	return useQuery({ queryKey: queryKeys.eval.taken(), queryFn: listTakenOrigins });
}

/** The run list. Depth is fixed — the key carries no limit, so a
 *  per-caller one would let one caller's depth decide every caller's
 *  data; callers slice instead. The search text is keyed, so it can vary.
 *
 *  Matching happens server-side: it spans the references a run covered
 *  and what the models produced, neither of which is in a row. */
export function useRuns(text?: string) {
	return useQuery({
		queryKey: queryKeys.eval.runs(text),
		queryFn: () => listRuns(RUN_LIST_LIMIT, text),
		// Keep the previous list on screen while a keystroke refetches,
		// rather than blanking the table between characters.
		placeholderData: keepPreviousData,
	});
}

export function useRunSummary(id: string | null) {
	return useQuery({
		queryKey: queryKeys.eval.run(id ?? ""),
		queryFn: () => getRunSummary(id ?? ""),
		enabled: !!id,
	});
}

export function useEvalResults(filters?: EvalResultFilters) {
	return useQuery({
		queryKey: queryKeys.eval.results(filters as Record<string, string> | undefined),
		queryFn: () => listResults(filters),
		enabled: !!filters?.run_id || !!filters?.sample_id || !!filters?.model,
	});
}

export function useModelRuns(modelId: string | null) {
	return useQuery({
		queryKey: queryKeys.eval.modelRuns(modelId ?? ""),
		queryFn: () => getModelRuns(modelId ?? ""),
		enabled: !!modelId,
	});
}

export function useCaptureRecord() {
	return useInvalidatingMutation({
		mutationFn: (recordId: string) => captureRecord(recordId),
		invalidates: SAMPLE_INVALIDATES,
	});
}

/**
 * Take a history record and label it in one action.
 *
 * The store has no endpoint for this — a capture and its promotion are
 * two writes — so the chain lives here rather than being spelled out at
 * the call site. If the promotion is what fails, the capture stays in
 * the labelling queue carrying its origin text, which is exactly where
 * capturing without a reference would have left it; nothing is lost and
 * nothing needs undoing.
 */
export function useLabelRecord() {
	return useInvalidatingMutation({
		mutationFn: async (vars: { recordId: string; reference: string }) => {
			const pending = await captureRecord(vars.recordId);
			return promotePending(pending.id, vars.reference);
		},
		invalidates: SAMPLE_INVALIDATES,
	});
}

/** What a batch of captures actually did. Each id is refused for its own
 *  reason, so the caller can say which rather than just how many. */
export interface CaptureBatchOutcome {
	added: number;
	/** Of `added`, how many arrived with a reference already typed. */
	labelled: number;
	alreadyTaken: number;
	notLossless: number;
	noAudio: number;
	/** Anything else — a vanished record, a write that failed. */
	failed: number;
	/** The first message behind `failed`, for a toast that says something. */
	firstError?: string;
}

/**
 * Take history records into the evaluation set, labelling the ones that
 * arrive with a reference and queueing the rest.
 *
 * Sequential: each capture resamples a WAV and writes a file, and the
 * duplicate check reads the taken-origin set, so firing them at once
 * would race two captures of the same record past that check.
 *
 * An empty reference queues the clip rather than failing. Refusals are
 * ordinary too — already taken, lossy audio, audio dropped by retention
 * — so they are counted by reason and the batch carries on. This hook
 * asks for every id it is given; whether a caller narrows the list
 * first is the caller's business.
 */
export function useAddRecordsToEval() {
	return useInvalidatingMutation({
		mutationFn: async (
			items: { recordId: string; reference: string }[],
		): Promise<CaptureBatchOutcome> => {
			const out: CaptureBatchOutcome = {
				added: 0,
				labelled: 0,
				alreadyTaken: 0,
				notLossless: 0,
				noAudio: 0,
				failed: 0,
			};
			for (const item of items) {
				try {
					const pending = await captureRecord(item.recordId);
					out.added += 1;
					const reference = item.reference.trim();
					if (reference) {
						await promotePending(pending.id, reference);
						out.labelled += 1;
					}
				} catch (err) {
					countRefusal(out, err);
				}
			}
			return out;
		},
		invalidates: SAMPLE_INVALIDATES,
	});
}

/** Sort one capture failure into the bucket that explains it. */
function countRefusal(out: CaptureBatchOutcome, err: unknown) {
	const code = err instanceof ApiClientError ? err.code : "";
	if (code === "EVAL_SAMPLE_EXISTS") out.alreadyTaken += 1;
	else if (code === "EVAL_AUDIO_NOT_LOSSLESS") out.notLossless += 1;
	else if (code === "NO_AUDIO") out.noAudio += 1;
	else {
		out.failed += 1;
		out.firstError ??= err instanceof Error ? err.message : String(err);
	}
}

export function useUploadCapture() {
	return useInvalidatingMutation({
		mutationFn: (vars: { file: File; captureDevice?: string }) =>
			uploadCapture(vars.file, vars.captureDevice),
		invalidates: SAMPLE_INVALIDATES,
	});
}

export function useDiscardPending() {
	return useInvalidatingMutation({
		mutationFn: (id: string) => discardPending(id),
		invalidates: SAMPLE_INVALIDATES,
	});
}

export function usePromotePending() {
	return useInvalidatingMutation({
		mutationFn: (vars: { id: string; reference: string }) =>
			promotePending(vars.id, vars.reference),
		invalidates: SAMPLE_INVALIDATES,
	});
}

/**
 * Rewrite what a sample says.
 *
 * Every run that ever covered this sample is re-scored by it: a cell's
 * verdict falls back to a comparison against the reference wherever
 * nobody ruled on the output, so the whole eval tree goes stale, not
 * just the sample list. Human rulings are keyed on (sample, output text)
 * and survive untouched.
 */
export function useSetReference() {
	return useInvalidatingMutation({
		mutationFn: (vars: { id: string; reference: string }) => setReference(vars.id, vars.reference),
		invalidates: [...SAMPLE_INVALIDATES, queryKeys.eval.all],
	});
}

export function useDeleteSamples() {
	return useInvalidatingMutation({
		mutationFn: (ids: string[]) => deleteSamples(ids),
		invalidates: [...SAMPLE_INVALIDATES, queryKeys.eval.all],
	});
}

export function useDeleteSample() {
	return useInvalidatingMutation({
		mutationFn: (id: string) => deleteSample(id),
		invalidates: [...SAMPLE_INVALIDATES, queryKeys.eval.all],
	});
}

export function useStartRun() {
	return useInvalidatingMutation({
		mutationFn: (vars: { modelIds: string[]; language?: string; sampleIds?: string[] }) =>
			startRun(vars.modelIds, vars.language, vars.sampleIds),
		invalidates: [queryKeys.eval.runs(), queryKeys.eval.overview()],
	});
}

export function useSetRunNote() {
	return useInvalidatingMutation({
		mutationFn: (vars: { id: string; note: string | null }) => setRunNote(vars.id, vars.note),
		invalidates: [queryKeys.eval.all],
	});
}

export function useDeleteRun() {
	return useInvalidatingMutation({
		mutationFn: (id: string) => deleteRun(id),
		invalidates: [queryKeys.eval.all],
	});
}

export function useCancelRun() {
	return useInvalidatingMutation({
		mutationFn: (runId: string) => cancelRun(runId),
		invalidates: [queryKeys.eval.all],
	});
}

export function useJudge() {
	return useInvalidatingMutation({
		mutationFn: (vars: { sampleId: string; outputText: string; correct: boolean | null }) =>
			vars.correct === null
				? clearJudgement(vars.sampleId, vars.outputText)
				: putJudgement(vars.sampleId, vars.outputText, vars.correct),
		invalidates: JUDGEMENT_INVALIDATES,
	});
}

/** What a run writes while it is going: its own summary, the results
 *  matrix, the run list's counts, and the overview's latest-run block.
 *  The sample set and the rulings cannot change from a run, so they stay
 *  out of the live refresh. */
const LIVE_RUN_KEYS = [
	queryKeys.eval.overview(),
	[...queryKeys.eval.all, "runs"],
	[...queryKeys.eval.all, "run"],
	[...queryKeys.eval.all, "results"],
];

/** A floor on how often a run's own progress refetches the rows beneath
 *  it. A fast candidate reports several samples a second, and each one is
 *  four queries — the table does not need to be more current than a
 *  reader can read. */
const LIVE_REFRESH_MS = 3000;

/** Live run progress. `null` between runs.
 *
 *  A watch channel on the server, so a screen opened mid-run gets the
 *  current position immediately rather than waiting for the next sample. */
export function useRunProgress(): RunProgress | null {
	const [progress, setProgress] = useState<RunProgress | null>(null);
	const queryClient = useQueryClient();

	useEffect(() => {
		let last: string | null = null;
		let refreshed = 0;
		return subscribeSSE(
			"/api/eval/runs/progress",
			(data) => {
				const next = data as RunProgress | null;
				setProgress(next);
				if (next) {
					// Results are written per sample, so the tables under the bar
					// are already out of date by the time the bar moves.
					const now = performance.now();
					if (now - refreshed >= LIVE_REFRESH_MS) {
						refreshed = now;
						for (const key of LIVE_RUN_KEYS) {
							queryClient.invalidateQueries({ queryKey: key });
						}
					}
				} else if (last) {
					// The run ending settles everything, including the figures
					// that are only final once every sample is in.
					queryClient.invalidateQueries({ queryKey: queryKeys.eval.all });
				}
				last = next?.run_id ?? null;
			},
			undefined,
			"progress",
		);
	}, [queryClient]);

	return progress;
}

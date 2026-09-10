import { del, delWithBody, get, post, postBinary, put } from "./client";
import type {
	EvalOverview,
	EvalResult,
	EvalResultFilters,
	EvalRunListEntry,
	EvalSample,
	EvalSampleListEntry,
	ModelRunEntry,
	PendingCapture,
	RunSummary,
	SampleSetComposition,
} from "./types";

export function getEvalOverview(): Promise<EvalOverview> {
	return get<EvalOverview>("/api/eval/overview");
}

export function getComposition(): Promise<SampleSetComposition> {
	return get<SampleSetComposition>("/api/eval/composition");
}

export function listSamples(): Promise<EvalSampleListEntry[]> {
	return get<EvalSampleListEntry[]>("/api/eval/samples");
}

export function deleteSample(id: string): Promise<void> {
	return del(`/api/eval/samples/${encodeURIComponent(id)}`);
}

/** Delete several samples in one request. Each carries the same cascade
 *  as a single delete. */
export function deleteSamples(ids: string[]): Promise<{ requested: number; deleted: number }> {
	return post("/api/eval/samples/delete", { ids });
}

export function setReference(
	id: string,
	reference: string,
	referenceLocale?: string | null,
): Promise<void> {
	return put(`/api/eval/samples/${encodeURIComponent(id)}/reference`, {
		reference,
		reference_locale: referenceLocale ?? null,
	});
}

export function listPending(): Promise<PendingCapture[]> {
	return get<PendingCapture[]>("/api/eval/pending");
}

/** Copy a history record's audio in. The caller never pre-checks:
 *  "already taken" and "lossy audio" are decided by the server. */
export function captureRecord(recordId: string): Promise<PendingCapture> {
	return post<PendingCapture>("/api/eval/pending", { record_id: recordId });
}

export function discardPending(id: string): Promise<void> {
	return del(`/api/eval/pending/${encodeURIComponent(id)}`);
}

export function promotePending(
	id: string,
	reference: string,
	referenceLocale?: string | null,
): Promise<EvalSample> {
	return post<EvalSample>(`/api/eval/pending/${encodeURIComponent(id)}/promote`, {
		reference,
		reference_locale: referenceLocale ?? null,
	});
}

/** History record ids already claimed, so the picker can grey them out. */
export function listTakenOrigins(): Promise<string[]> {
	return get<string[]>("/api/eval/taken");
}

export function listRuns(limit = 50, text?: string): Promise<EvalRunListEntry[]> {
	return get<EvalRunListEntry[]>("/api/eval/runs", {
		limit: String(limit),
		...(text ? { text } : {}),
	});
}

export function getRunSummary(id: string): Promise<RunSummary> {
	return get<RunSummary>(`/api/eval/runs/${encodeURIComponent(id)}`);
}

export function startRun(
	modelIds: string[],
	language?: string,
	sampleIds?: string[],
): Promise<{ run_id: string }> {
	return post<{ run_id: string }>("/api/eval/runs", {
		model_ids: modelIds,
		language: language || null,
		sample_ids: sampleIds ?? null,
	});
}

export function setRunNote(id: string, note: string | null): Promise<void> {
	return put(`/api/eval/runs/${encodeURIComponent(id)}/note`, { note });
}

export function deleteRun(id: string): Promise<void> {
	return del(`/api/eval/runs/${encodeURIComponent(id)}`);
}

export function listResults(filters?: EvalResultFilters): Promise<EvalResult[]> {
	const params: Record<string, string> = {};
	if (filters?.run_id) params.run_id = filters.run_id;
	if (filters?.sample_id) params.sample_id = filters.sample_id;
	if (filters?.model) params.model = filters.model;
	if (filters?.text) params.text = filters.text;
	if (filters?.capture_device) params.capture_device = filters.capture_device;
	if (filters?.limit !== undefined) params.limit = String(filters.limit);
	if (filters?.offset !== undefined) params.offset = String(filters.offset);
	return get<EvalResult[]>("/api/eval/results", params);
}

/** Stop the run in flight. Rejects with EVAL_RUN_NOT_RUNNING if this is
 *  not the run currently going — a stale screen cannot stop a later run. */
export function cancelRun(id: string): Promise<void> {
	return post(`/api/eval/runs/${encodeURIComponent(id)}/cancel`);
}

export function putJudgement(
	sampleId: string,
	outputText: string,
	correct: boolean,
): Promise<void> {
	return put("/api/eval/judgements", {
		sample_id: sampleId,
		output_text: outputText,
		correct,
	});
}

export function clearJudgement(sampleId: string, outputText: string): Promise<void> {
	// The key is (sample, output string); an output string has no place in
	// a URL path, so the identity travels in the body.
	return delWithBody("/api/eval/judgements", {
		sample_id: sampleId,
		output_text: outputText,
	});
}

/** Upload a clip that never was a history record. */
export function uploadCapture(file: File, captureDevice?: string): Promise<PendingCapture> {
	return postBinary<PendingCapture>("/api/eval/pending/upload", file, "audio/wav", {
		capture_device: captureDevice ?? "",
	});
}

export function getModelRuns(modelId: string): Promise<ModelRunEntry[]> {
	return get<ModelRunEntry[]>(`/api/eval/models/${encodeURIComponent(modelId)}`);
}

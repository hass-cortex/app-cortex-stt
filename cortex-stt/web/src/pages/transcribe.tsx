import {
	Check,
	ChevronDown,
	ChevronRight,
	Copy,
	FlaskConical,
	Mic,
	Play,
	Square,
	Upload,
	X,
} from "lucide-react";
import { type ChangeEvent, type DragEvent, useCallback, useMemo, useRef, useState } from "react";
import { type TranscribeResponse, transcribeClip } from "@/api/transcribe";
import type { ModelInfo } from "@/api/types";
import { ClipPlayer } from "@/components/audio/clip-player";
import { ClipTimeline, segmentsDivideClip } from "@/components/audio/clip-timeline";
import { RawOutput } from "@/components/transcript/raw-output";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { useToast } from "@/components/ui/toast";
import { useSetDefaultModel } from "@/hooks/use-engine";
import { useUploadCapture } from "@/hooks/use-eval";
import { useModels } from "@/hooks/use-models";
import { useRecorder } from "@/hooks/use-recorder";
import { type DecodedClip, decodeClip, encodeWav } from "@/lib/audio";
import { copyToClipboard } from "@/lib/clipboard";
import { clipFileName, formatBytes, formatDuration } from "@/lib/format";

/** So a run started here is identifiable in history later. */
const CAPTURE_DEVICE = "web-transcribe";
const MAX_MODELS = 3;

interface RunState {
	modelId: string;
	status: "queued" | "running" | "done" | "failed";
	result?: TranscribeResponse;
	error?: string;
}

interface Clip extends DecodedClip {
	name: string;
	wav: Blob;
	/** Created once per clip: building it in render leaks a URL per frame. */
	url: string;
}

function installed(models: ModelInfo[] | undefined): ModelInfo[] {
	return (models ?? []).filter((m) => m.status === "downloaded" || m.status === "custom");
}

export function TranscribePage() {
	const { data: models } = useModels();
	const { toast } = useToast();
	const recorder = useRecorder();
	const setDefault = useSetDefaultModel();
	const uploadCapture = useUploadCapture();
	const fileInput = useRef<HTMLInputElement>(null);

	const [clip, setClip] = useState<Clip | null>(null);
	const [decoding, setDecoding] = useState(false);
	const [language, setLanguage] = useState("");
	const [selected, setSelected] = useState<string[]>([]);
	const [runs, setRuns] = useState<RunState[]>([]);
	const [focused, setFocused] = useState<string | null>(null);

	const pool = installed(models);
	const running = runs.some((r) => r.status === "running" || r.status === "queued");

	const loadBlob = useCallback(
		async (blob: Blob, name: string) => {
			setDecoding(true);
			try {
				const decoded = await decodeClip(await blob.arrayBuffer());
				const wav = encodeWav(decoded.samples);
				setClip((previous) => {
					if (previous) URL.revokeObjectURL(previous.url);
					return { ...decoded, name, wav, url: URL.createObjectURL(wav) };
				});
				setRuns([]);
				setFocused(null);
			} catch {
				toast("Could not decode that file — the browser does not recognise the format.", "error");
			} finally {
				setDecoding(false);
			}
		},
		[toast],
	);

	const onPick = (event: ChangeEvent<HTMLInputElement>) => {
		const file = event.target.files?.[0];
		if (file) void loadBlob(file, file.name);
	};

	const onDrop = (event: DragEvent<HTMLElement>) => {
		event.preventDefault();
		const file = event.dataTransfer.files?.[0];
		if (file) void loadBlob(file, file.name);
	};

	const toggleModel = (id: string) => {
		setSelected((prev) =>
			prev.includes(id)
				? prev.filter((m) => m !== id)
				: prev.length >= MAX_MODELS
					? prev
					: [...prev, id],
		);
	};

	/** Sequential on purpose: the pool admits one inference at a time, so
	 *  running them together would only queue them and make every latency
	 *  reading include another model's wait. */
	const run = async () => {
		if (!clip || selected.length === 0) return;
		setRuns(selected.map((modelId) => ({ modelId, status: "queued" })));
		setFocused(selected[0] ?? null);

		for (const modelId of selected) {
			setRuns((prev) => prev.map((r) => (r.modelId === modelId ? { ...r, status: "running" } : r)));
			try {
				const result = await transcribeClip(clip.wav, {
					model: modelId,
					language: language.trim() || undefined,
					captureDevice: CAPTURE_DEVICE,
				});
				setRuns((prev) =>
					prev.map((r) => (r.modelId === modelId ? { ...r, status: "done", result } : r)),
				);
			} catch (error) {
				setRuns((prev) =>
					prev.map((r) =>
						r.modelId === modelId
							? { ...r, status: "failed", error: error instanceof Error ? error.message : "Failed" }
							: r,
					),
				);
			}
		}
	};

	const baseline = runs.find((r) => r.result)?.result?.text;

	const modelName = useMemo(() => {
		const byId = new Map(pool.map((m) => [m.id, m.name]));
		return (id: string) => byId.get(id) ?? id;
	}, [pool]);

	return (
		<div className="flex flex-col gap-4">
			<div className="flex items-end justify-between gap-4 flex-wrap">
				<div>
					<h1 className="text-[21px] font-semibold tracking-[-0.01em] text-text-primary">
						Transcribe
					</h1>
					<p className="text-[12.5px] text-text-secondary mt-1 max-w-2xl">
						Hear what a model makes of a recording before you make it the default. Each run is a
						real request, so it lands in history as <span className="num">{CAPTURE_DEVICE}</span>.
					</p>
				</div>
				<div className="flex gap-2">
					{clip && (
						<Button
							variant="ghost"
							icon={<X size={14} strokeWidth={1.8} />}
							onClick={() => {
								setClip(null);
								setRuns([]);
							}}
						>
							Clear
						</Button>
					)}
					<Button
						icon={<Play size={14} strokeWidth={1.8} />}
						disabled={!clip || selected.length === 0 || running}
						loading={running}
						onClick={run}
					>
						{selected.length > 1 ? `Run ${selected.length} models` : "Run"}
					</Button>
				</div>
			</div>

			<div className="flex flex-col xl:flex-row gap-4 items-start">
				{/* input */}
				<div className="w-full xl:w-[372px] shrink-0 flex flex-col gap-4">
					<Card>
						<CardHeader title="Input" description={clip?.name ?? "nothing loaded"} />

						{clip ? (
							<div className="mt-3">
								<ClipPlayer
									src={clip.url}
									peaks={clip.peaks}
									durationMs={clip.durationMs}
									meta={`${(clip.durationMs / 1000).toFixed(2)} s`}
									// What saves is the decoded 16 kHz mono WAV the models were
									// given, not the file that was loaded.
									downloadName={clipFileName(clip.name.replace(/\.[^.]+$/, ""))}
								/>
								<div className="num mt-3 grid grid-cols-2 gap-y-2 gap-x-3 text-[11px] text-text-muted">
									<span>16 kHz · mono · PCM16</span>
									<span className="text-right">{formatBytes(clip.wav.size)}</span>
									<span>peak {clip.peakDb.toFixed(1)} dBFS</span>
									<span
										className={`text-right ${clip.rmsDb < -40 ? "text-warning" : "text-text-primary"}`}
									>
										rms {clip.rmsDb.toFixed(1)} dBFS
									</span>
								</div>
							</div>
						) : (
							<button
								type="button"
								onClick={() => fileInput.current?.click()}
								onDrop={onDrop}
								onDragOver={(e) => e.preventDefault()}
								className="mt-3 w-full flex flex-col items-center justify-center gap-2 py-10 px-4 border border-dashed border-accent-quiet rounded-lg text-center cursor-pointer"
							>
								{decoding ? (
									<Spinner />
								) : (
									<>
										<Upload size={24} strokeWidth={1.5} className="text-accent-ink" />
										<p className="text-[12.5px] text-text-primary">Drop an audio file here</p>
										<p className="num text-[11px] text-text-muted">
											wav · mp3 · flac · ogg — decoded here, sent as WAV
										</p>
									</>
								)}
							</button>
						)}

						<div className="mt-3.5 flex gap-2">
							<Button
								variant="secondary"
								icon={<Upload size={14} strokeWidth={1.8} />}
								onClick={() => fileInput.current?.click()}
							>
								{clip ? "Replace file" : "Choose file"}
							</Button>
							{recorder.supported ? (
								<Button
									variant={recorder.recording ? "danger" : "secondary"}
									icon={
										recorder.recording ? (
											<Square size={14} strokeWidth={1.8} />
										) : (
											<Mic size={14} strokeWidth={1.8} />
										)
									}
									onClick={async () => {
										if (recorder.recording) {
											const blob = await recorder.stop();
											if (blob) await loadBlob(blob, "recording.wav");
										} else {
											await recorder.start();
										}
									}}
								>
									{recorder.recording ? "Stop" : "Record"}
								</Button>
							) : (
								<Button variant="secondary" disabled icon={<Mic size={14} strokeWidth={1.8} />}>
									Record
								</Button>
							)}
							<input
								ref={fileInput}
								type="file"
								accept="audio/*"
								className="hidden"
								onChange={onPick}
							/>
						</div>
						{recorder.error && <p className="mt-2 text-[11.5px] text-error">{recorder.error}</p>}
						{!recorder.supported && (
							<p className="mt-2 text-[11.5px] text-text-muted">
								Recording needs a secure context — this page is served over plain HTTP. Reach it
								over HTTPS, or drop a file instead.
							</p>
						)}
					</Card>

					<Card>
						<CardHeader title="Request" />
						<div className="mt-3.5 flex flex-col gap-3.5">
							<Input
								label="LANGUAGE"
								placeholder="auto — e.g. zh-TW, en, ja"
								value={language}
								onChange={(e) => setLanguage(e.target.value)}
							/>
							<p className="text-[11px] text-text-muted leading-relaxed -mt-1">
								The base subtag is the hint the model matches; anything after it selects how the
								transcript is written.
							</p>

							<div className="flex flex-col gap-2">
								<span className="num text-[10.5px] tracking-[0.07em] text-text-muted">
									MODELS TO COMPARE
								</span>
								<div className="flex flex-wrap gap-1.5">
									{pool.length === 0 && (
										<span className="text-[12px] text-text-muted">
											No models on disk. Download one first.
										</span>
									)}
									{pool.map((model) => {
										const on = selected.includes(model.id);
										return (
											<button
												key={model.id}
												type="button"
												onClick={() => toggleModel(model.id)}
												className={`px-2.5 py-1.5 rounded-md text-[11.5px] border transition-colors cursor-pointer ${
													on
														? "bg-accent-wash border-accent-quiet text-text-primary"
														: "bg-surface-3 border-border text-text-secondary hover:text-text-primary"
												}`}
											>
												{model.name}
												{model.is_loaded && <span className="ml-1.5 text-success">•</span>}
											</button>
										);
									})}
								</div>
								<span className="num text-[10.5px] text-text-faint">
									up to {MAX_MODELS} · a dot marks a model already resident; the others pay their
									load time on the first request
								</span>
							</div>
						</div>
					</Card>
				</div>

				{/* results */}
				<div className="flex-1 min-w-0 flex flex-col gap-3">
					<div className="flex items-baseline gap-2.5 flex-wrap">
						<span className="text-[13.5px] font-semibold text-text-primary">Results</span>
						{clip && (
							<span className="num text-[11px] text-text-muted">
								{formatDuration(clip.durationMs)} clip
								{language.trim() && ` · ${language.trim()}`}
								{runs.length > 0 &&
									` · ${runs.filter((r) => r.status === "done" || r.status === "failed").length} of ${runs.length} finished`}
							</span>
						)}
					</div>

					{runs.length === 0 && (
						<Card>
							<p className="text-[12.5px] text-text-muted py-6 text-center">
								{clip ? "Pick one or more models, then run." : "Load a recording to get started."}
							</p>
						</Card>
					)}

					{runs.map((state) => {
						const differs =
							state.result && baseline !== undefined && state.result.text !== baseline;
						// The detail lives inside the card it describes: a panel at the
						// bottom of the list cannot say which of three models it is for.
						// A card with nothing under it does not open at all.
						const divided =
							!!clip && segmentsDivideClip(state.result?.segments ?? [], clip.durationMs);
						const hasDetail = !!state.result?.raw_text || divided;
						const open = focused === state.modelId && hasDetail;
						return (
							<div
								key={state.modelId}
								className={`bg-surface-2 border rounded-[10px] transition-colors ${
									open ? "border-accent-quiet" : "border-border"
								}`}
							>
								<button
									type="button"
									onClick={() =>
										setFocused((prev) => (prev === state.modelId ? null : state.modelId))
									}
									disabled={!hasDetail}
									className="text-left w-full px-4 pt-3.5 disabled:cursor-default cursor-pointer"
								>
									<div className="flex items-center gap-2.5 flex-wrap">
										{hasDetail ? (
											open ? (
												<ChevronDown
													size={14}
													strokeWidth={1.8}
													className="text-accent-ink shrink-0"
												/>
											) : (
												<ChevronRight
													size={14}
													strokeWidth={1.8}
													className="text-text-faint shrink-0"
												/>
											)
										) : (
											<span className="w-3.5 shrink-0" />
										)}
										<span
											className={`w-[7px] h-[7px] rounded-full shrink-0 ${
												state.status === "done"
													? "bg-success"
													: state.status === "failed"
														? "bg-error"
														: "bg-warning animate-pulse"
											}`}
										/>
										<span className="text-[13px] font-semibold text-text-primary">
											{modelName(state.modelId)}
										</span>
										{differs && <Badge variant="warning">differs</Badge>}
										{state.result && (
											<div className="num ml-auto flex items-center gap-4 text-[11px] text-text-muted">
												<span>
													infer{" "}
													<span className="text-text-primary">
														{formatDuration(state.result.inference_ms)}
													</span>
												</span>
												<span>
													rtf{" "}
													<span className="text-text-primary">
														{state.result.duration_ms > 0
															? `${(state.result.inference_ms / state.result.duration_ms).toFixed(2)}×`
															: "—"}
													</span>
												</span>
												<span>
													acquire{" "}
													<span
														className={
															state.result.cold_load_ms > 0 ? "text-warning" : "text-text-primary"
														}
													>
														{formatDuration(state.result.cold_load_ms + state.result.pool_wait_ms)}
													</span>
												</span>
											</div>
										)}
									</div>

									<div className="mt-2.5 px-3 py-2.5 bg-surface-3 rounded-md text-[15px] leading-relaxed text-text-primary min-h-[44px]">
										{state.status === "running" && (
											<span className="text-text-muted text-[13px]">transcribing…</span>
										)}
										{state.status === "queued" && (
											<span className="text-text-muted text-[13px]">waiting for the pool…</span>
										)}
										{state.status === "failed" && (
											<span className="text-error text-[13px]">{state.error}</span>
										)}
										{state.result?.text}
									</div>
								</button>

								{state.result && (
									<div className="px-4 pt-2.5 pb-3.5 flex items-center gap-2 flex-wrap">
										<Button
											variant="outline"
											icon={<Check size={14} strokeWidth={1.8} />}
											onClick={() => {
												setDefault.mutate(state.modelId, {
													onSuccess: () => toast(`${modelName(state.modelId)} is now the default`),
												});
											}}
										>
											Set as default
										</Button>
										<Button
											variant="outline"
											icon={<FlaskConical size={14} strokeWidth={1.8} />}
											onClick={() => {
												if (!clip) return;
												uploadCapture.mutate(
													{
														file: new File([clip.wav], clip.name.replace(/\.[^.]+$/, ".wav"), {
															type: "audio/wav",
														}),
														captureDevice: CAPTURE_DEVICE,
													},
													{
														onSuccess: () =>
															toast("Added to labelling — type what was actually said"),
														onError: () => toast("Not added", "error"),
													},
												);
											}}
										>
											Add to evaluation
										</Button>
										<Button
											variant="ghost"
											icon={<Copy size={14} strokeWidth={1.8} />}
											onClick={() => {
												void copyToClipboard(state.result?.text ?? "");
												toast("Transcript copied");
											}}
										>
											Copy
										</Button>
									</div>
								)}

								{open && clip && state.result && (
									<div className="px-4 pb-4 pt-3.5 border-t border-border-soft">
										<p className="num text-[11px] text-text-muted">
											{state.result.segments.length}{" "}
											{state.result.segments.length === 1 ? "segment" : "segments"} ·{" "}
											{state.result.device.toUpperCase()}
											{divided && " — where this model heard speech"}
										</p>
										{divided && (
											<div className="mt-3">
												<ClipTimeline
													peaks={clip.peaks}
													durationMs={clip.durationMs}
													segments={state.result.segments}
													height={40}
												/>
											</div>
										)}
										{state.result.raw_text && (
											<RawOutput
												text={state.result.raw_text}
												className={divided ? "mt-7" : "mt-3"}
											/>
										)}
									</div>
								)}
							</div>
						);
					})}
				</div>
			</div>
		</div>
	);
}

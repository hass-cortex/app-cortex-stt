import { Download, Pause, Play } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { TranscriptionSegment } from "@/api/types";
import { Spinner } from "@/components/ui/spinner";
import { useClipPeaks } from "@/hooks/use-clip";
import { formatAudioTime } from "@/lib/format";
import { ClipTimeline } from "./clip-timeline";

interface ClipPlayerProps {
	src: string;
	/** Server-reported duration; used until the clip is decoded. */
	durationMs: number;
	segments?: TranscriptionSegment[];
	meta?: string;
	/** Supply when the caller has already decoded the clip. */
	peaks?: number[];
	/** What the saved file is called. Every clip this app plays is WAV,
	 *  so the name only has to say which clip it was. */
	downloadName?: string;
}

/** Tenths for clips short enough that whole seconds would sit still for a
 *  visible stretch of the playback. */
function formatClipTime(seconds: number, totalMs: number): string {
	if (totalMs >= 60_000) return formatAudioTime(seconds);
	return `${Math.floor(seconds / 60)}:${(seconds % 60).toFixed(1).padStart(4, "0")}`;
}

/**
 * Playback and the time axis in one block: the waveform, the segment
 * strip and the playhead are the same axis, so seeing where the playhead
 * sits tells you which segment is sounding.
 */
export function ClipPlayer({
	src,
	durationMs,
	segments = [],
	meta,
	peaks,
	downloadName = "clip.wav",
}: ClipPlayerProps) {
	const audioRef = useRef<HTMLAudioElement | null>(null);
	const frameRef = useRef<number | null>(null);
	const [playing, setPlaying] = useState(false);
	const [played, setPlayed] = useState(0);
	const { data: clip, isLoading, isError } = useClipPeaks(peaks ? null : src);

	const bars = peaks ?? clip?.peaks ?? [];
	const duration = clip?.durationMs ?? durationMs;

	const readPosition = useCallback(() => {
		const audio = audioRef.current;
		if (!audio) return;
		const total =
			Number.isFinite(audio.duration) && audio.duration > 0 ? audio.duration : duration / 1000;
		if (total > 0) setPlayed(Math.min(1, Math.max(0, audio.currentTime / total)));
	}, [duration]);

	/**
	 * `timeupdate` fires roughly four times a second — and less regularly
	 * while the clip is still buffering — which leaves the playhead up to
	 * 0.4 s behind the sound, most visibly at the end where it should land
	 * on the right edge exactly as playback stops. Drive it per frame
	 * instead, and only while something is playing.
	 */
	useEffect(() => {
		if (!playing) return;
		const step = () => {
			readPosition();
			frameRef.current = requestAnimationFrame(step);
		};
		frameRef.current = requestAnimationFrame(step);
		return () => {
			if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
			frameRef.current = null;
		};
	}, [playing, readPosition]);

	const toggle = () => {
		const audio = audioRef.current;
		if (!audio) return;
		if (audio.paused) void audio.play();
		else audio.pause();
	};

	/** Clicking a point on the axis is a request to hear that point. */
	const seekTo = useCallback(
		(ratio: number) => {
			const audio = audioRef.current;
			if (!audio) return;
			const total =
				Number.isFinite(audio.duration) && audio.duration > 0 ? audio.duration : duration / 1000;
			audio.currentTime = total * Math.min(1, Math.max(0, ratio));
			setPlayed(ratio);
			if (audio.paused) void audio.play();
		},
		[duration],
	);

	return (
		<div className="p-3.5 bg-surface-3 rounded-lg">
			<div className="flex items-center gap-3">
				<button
					type="button"
					onClick={toggle}
					className="flex items-center justify-center w-7 h-7 rounded-full bg-accent text-white shrink-0 cursor-pointer"
					aria-label={playing ? "Pause" : "Play"}
				>
					{playing ? (
						<Pause size={13} strokeWidth={2} fill="currentColor" />
					) : (
						<Play size={13} strokeWidth={2} fill="currentColor" />
					)}
				</button>
				<span className="num text-[12px] text-text-primary">
					{formatClipTime((played * duration) / 1000, duration)}
					<span className="text-text-faint"> / {formatClipTime(duration / 1000, duration)}</span>
				</span>
				<div className="ml-auto flex items-center gap-2.5">
					{meta && <span className="num text-[11px] text-text-muted">{meta}</span>}
					{/* An anchor, not a button: the clip is already a URL the browser
					    can save, and `src` carries the api_key for the elements that
					    cannot set a header — the same reason <audio> above works. */}
					<a
						href={src}
						download={downloadName}
						aria-label="Download audio"
						className="shrink-0 flex items-center justify-center w-6 h-6 rounded-md text-text-muted hover:text-accent transition-colors cursor-pointer"
					>
						<Download size={13} strokeWidth={1.8} />
					</a>
				</div>
			</div>

			<div className="mt-2.5">
				{isLoading && !peaks ? (
					<div className="flex justify-center py-4">
						<Spinner size="sm" />
					</div>
				) : (
					<ClipTimeline
						peaks={bars}
						durationMs={duration}
						segments={segments}
						playedRatio={played}
						height={44}
						onSeek={seekTo}
					/>
				)}
				{isError && !peaks && (
					<p className="num text-[11px] text-text-muted mt-2">
						Audio could not be decoded for display; playback still works.
					</p>
				)}
			</div>

			{/* biome-ignore lint/a11y/useMediaCaption: the transcript below is the caption */}
			<audio
				ref={audioRef}
				src={src}
				preload="metadata"
				className="hidden"
				onPlay={() => setPlaying(true)}
				onPause={() => {
					setPlaying(false);
					readPosition();
				}}
				onEnded={() => {
					setPlaying(false);
					setPlayed(1);
				}}
				onSeeked={readPosition}
				onLoadedMetadata={readPosition}
			/>
		</div>
	);
}

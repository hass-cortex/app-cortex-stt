import { Pause, Play } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { audioUrl } from "@/api/client";
import { formatAudioTime } from "@/lib/format";

interface AudioPlayerProps {
	recordId: string;
	durationMs: number;
}

export function AudioPlayer({ recordId, durationMs }: AudioPlayerProps) {
	const audioRef = useRef<HTMLAudioElement>(null);
	const canvasRef = useRef<HTMLCanvasElement>(null);
	const [playing, setPlaying] = useState(false);
	const [currentTime, setCurrentTime] = useState(0);
	const [waveformData, setWaveformData] = useState<number[]>([]);

	const duration = durationMs / 1000;
	const src = audioUrl(recordId);

	// Decode audio and generate waveform data
	useEffect(() => {
		let cancelled = false;

		async function loadWaveform() {
			try {
				const response = await fetch(src);
				const arrayBuffer = await response.arrayBuffer();
				const audioContext = new AudioContext();
				const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);
				const channelData = audioBuffer.getChannelData(0);

				// Downsample to ~100 bars
				const bars = 100;
				const blockSize = Math.floor(channelData.length / bars);
				const samples: number[] = [];
				for (let i = 0; i < bars; i++) {
					let sum = 0;
					for (let j = 0; j < blockSize; j++) {
						sum += Math.abs(channelData[i * blockSize + j] ?? 0);
					}
					samples.push(sum / blockSize);
				}

				// Normalize
				const max = Math.max(...samples, 0.01);
				if (!cancelled) {
					setWaveformData(samples.map((s) => s / max));
				}

				await audioContext.close();
			} catch {
				// Failed to decode — show empty waveform
			}
		}

		loadWaveform();
		return () => {
			cancelled = true;
		};
	}, [src]);

	// Draw waveform
	const drawWaveform = useCallback(() => {
		const canvas = canvasRef.current;
		if (!canvas || waveformData.length === 0) return;

		const ctx = canvas.getContext("2d");
		if (!ctx) return;

		const dpr = window.devicePixelRatio || 1;
		const rect = canvas.getBoundingClientRect();
		canvas.width = rect.width * dpr;
		canvas.height = rect.height * dpr;
		ctx.scale(dpr, dpr);

		const { width, height } = rect;
		const barWidth = width / waveformData.length;
		const progress = duration > 0 ? currentTime / duration : 0;

		ctx.clearRect(0, 0, width, height);

		for (let i = 0; i < waveformData.length; i++) {
			const sample = waveformData[i] ?? 0;
			const barHeight = Math.max(2, sample * (height - 4));
			const x = i * barWidth;
			const y = (height - barHeight) / 2;
			const isFilled = i / waveformData.length <= progress;

			ctx.fillStyle = isFilled
				? getComputedStyle(canvas).getPropertyValue("--accent").trim() || "#89b4fa"
				: getComputedStyle(canvas).getPropertyValue("--border").trim() || "#45475a";
			ctx.fillRect(x + 1, y, barWidth - 2, barHeight);
		}
	}, [waveformData, currentTime, duration]);

	useEffect(() => {
		drawWaveform();
	}, [drawWaveform]);

	// Playback tracking
	useEffect(() => {
		const audio = audioRef.current;
		if (!audio) return;

		const onTimeUpdate = () => setCurrentTime(audio.currentTime);
		const onEnded = () => {
			setPlaying(false);
			setCurrentTime(0);
		};

		audio.addEventListener("timeupdate", onTimeUpdate);
		audio.addEventListener("ended", onEnded);

		return () => {
			audio.removeEventListener("timeupdate", onTimeUpdate);
			audio.removeEventListener("ended", onEnded);
		};
	}, []);

	const togglePlay = () => {
		const audio = audioRef.current;
		if (!audio) return;

		if (playing) {
			audio.pause();
		} else {
			audio.play();
		}
		setPlaying(!playing);
	};

	const handleCanvasClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
		const canvas = canvasRef.current;
		const audio = audioRef.current;
		if (!canvas || !audio) return;

		const rect = canvas.getBoundingClientRect();
		const x = e.clientX - rect.left;
		const percent = x / rect.width;
		audio.currentTime = percent * duration;
		setCurrentTime(audio.currentTime);
	};

	return (
		<div className="space-y-2">
			{/* biome-ignore lint/a11y/useMediaCaption: audio playback for speech transcription records */}
			<audio ref={audioRef} src={src} preload="auto" />

			<div className="flex items-center gap-3">
				<button
					type="button"
					onClick={togglePlay}
					className="p-2 rounded-full bg-accent text-surface-0 hover:bg-accent-hover transition-colors cursor-pointer"
				>
					{playing ? <Pause size={16} /> : <Play size={16} />}
				</button>

				<canvas
					ref={canvasRef}
					className="flex-1 h-12 cursor-pointer rounded"
					onClick={handleCanvasClick}
				/>

				<span className="text-xs text-text-muted font-mono w-20 text-right">
					{formatAudioTime(currentTime)} / {formatAudioTime(duration)}
				</span>
			</div>
		</div>
	);
}

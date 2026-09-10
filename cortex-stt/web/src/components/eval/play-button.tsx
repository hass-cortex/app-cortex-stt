import { Pause, Play } from "lucide-react";
import { useEffect, useRef, useState } from "react";

/**
 * A play button and nothing else.
 *
 * The matrix has one of these per row. The full player decodes the
 * whole file to draw its waveform, which is right for one clip on a
 * detail screen and wrong for forty of them in a table — and hearing the
 * audio is the whole point of the reference column, so making it a
 * navigation away from the comparison defeats it.
 */
export function PlayButton({ src }: { src: string }) {
	const ref = useRef<HTMLAudioElement>(null);
	const [playing, setPlaying] = useState(false);

	useEffect(() => {
		const audio = ref.current;
		if (!audio) return;
		const stop = () => setPlaying(false);
		audio.addEventListener("ended", stop);
		audio.addEventListener("pause", stop);
		return () => {
			audio.removeEventListener("ended", stop);
			audio.removeEventListener("pause", stop);
		};
	}, []);

	return (
		<>
			{/* biome-ignore lint/a11y/useMediaCaption: speech audio under comparison */}
			<audio ref={ref} src={src} preload="none" />
			<button
				type="button"
				aria-label={playing ? "Pause" : "Play"}
				onClick={() => {
					const audio = ref.current;
					if (!audio) return;
					if (playing) {
						audio.pause();
					} else {
						audio.currentTime = 0;
						void audio.play();
					}
					setPlaying(!playing);
				}}
				className="shrink-0 p-1 rounded-full text-text-muted hover:text-accent hover:bg-surface-3 transition-colors cursor-pointer"
			>
				{playing ? <Pause size={13} /> : <Play size={13} />}
			</button>
		</>
	);
}

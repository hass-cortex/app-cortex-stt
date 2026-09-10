import { useCallback, useRef, useState } from "react";

export interface Recorder {
	recording: boolean;
	error: string | null;
	/** Unsupported browsers (or an insecure context) never get the mic. */
	supported: boolean;
	start: () => Promise<void>;
	stop: () => Promise<Blob | null>;
}

/**
 * Records whatever container the browser prefers. The bytes are decoded
 * and re-encoded as canonical WAV before they are sent, so the container
 * choice never reaches the server.
 */
export function useRecorder(): Recorder {
	const [recording, setRecording] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const recorderRef = useRef<MediaRecorder | null>(null);
	const chunksRef = useRef<Blob[]>([]);

	const supported =
		typeof navigator !== "undefined" &&
		typeof MediaRecorder !== "undefined" &&
		!!navigator.mediaDevices?.getUserMedia;

	const start = useCallback(async () => {
		setError(null);
		try {
			const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
			const recorder = new MediaRecorder(stream);
			chunksRef.current = [];
			recorder.ondataavailable = (event) => {
				if (event.data.size > 0) chunksRef.current.push(event.data);
			};
			recorder.start();
			recorderRef.current = recorder;
			setRecording(true);
		} catch (e) {
			setError(
				e instanceof Error && e.name === "NotAllowedError"
					? "Microphone permission was denied."
					: "Could not open the microphone. A page served over plain HTTP cannot record.",
			);
		}
	}, []);

	const stop = useCallback(async () => {
		const recorder = recorderRef.current;
		if (!recorder) return null;
		const blob = await new Promise<Blob>((resolve) => {
			recorder.onstop = () => resolve(new Blob(chunksRef.current, { type: recorder.mimeType }));
			recorder.stop();
		});
		for (const track of recorder.stream.getTracks()) track.stop();
		recorderRef.current = null;
		setRecording(false);
		return blob;
	}, []);

	return { recording, error, supported, start, stop };
}

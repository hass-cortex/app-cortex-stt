/**
 * Client-side audio handling for the Transcribe page.
 *
 * The server decodes WAV (PCM 16/24/32 and IEEE float) and raw PCM only,
 * so anything the browser can open — mp3, flac, ogg, a MediaRecorder
 * blob — is decoded here and re-encoded as canonical 16 kHz mono WAV
 * before it is sent. Decoding locally also yields the peaks the timeline
 * draws, so the waveform costs no extra work.
 */

/** The server's canonical rate; sending anything else just makes it resample. */
export const TARGET_RATE = 16_000;

export interface DecodedClip {
	samples: Float32Array;
	durationMs: number;
	/** Peaks at the resolution the timeline draws. */
	peaks: number[];
	rmsDb: number;
	peakDb: number;
}

function toDb(amplitude: number): number {
	return amplitude <= 0 ? Number.NEGATIVE_INFINITY : 20 * Math.log10(amplitude);
}

/** Downmix to mono at 16 kHz using an offline graph — the browser owns
 *  the resampling, so we never hand-roll one. */
async function toMono16k(buffer: AudioBuffer): Promise<Float32Array> {
	const frames = Math.max(1, Math.ceil((buffer.duration * TARGET_RATE) / 1));
	const offline = new OfflineAudioContext(1, frames, TARGET_RATE);
	const source = offline.createBufferSource();
	source.buffer = buffer;
	source.connect(offline.destination);
	source.start();
	const rendered = await offline.startRendering();
	return rendered.getChannelData(0).slice();
}

export function peaksOf(samples: Float32Array, buckets: number): number[] {
	const size = Math.max(1, Math.floor(samples.length / buckets));
	const out: number[] = [];
	for (let i = 0; i < buckets; i++) {
		let peak = 0;
		const start = i * size;
		for (let j = start; j < Math.min(start + size, samples.length); j++) {
			const v = Math.abs(samples[j] ?? 0);
			if (v > peak) peak = v;
		}
		out.push(peak);
	}
	const loudest = Math.max(...out, 0.0001);
	return out.map((p) => p / loudest);
}

export function levelsOf(samples: Float32Array): { rmsDb: number; peakDb: number } {
	let sum = 0;
	let peak = 0;
	for (const s of samples) {
		sum += s * s;
		const a = Math.abs(s);
		if (a > peak) peak = a;
	}
	return {
		rmsDb: toDb(Math.sqrt(sum / Math.max(1, samples.length))),
		peakDb: toDb(peak),
	};
}

export async function decodeClip(data: ArrayBuffer, buckets = 96): Promise<DecodedClip> {
	const ctx = new AudioContext();
	try {
		const buffer = await ctx.decodeAudioData(data.slice(0));
		const samples = await toMono16k(buffer);
		return {
			samples,
			durationMs: (samples.length / TARGET_RATE) * 1000,
			peaks: peaksOf(samples, buckets),
			...levelsOf(samples),
		};
	} finally {
		void ctx.close();
	}
}

/** 16-bit PCM WAV — the one container the server decodes without guessing. */
export function encodeWav(samples: Float32Array, sampleRate = TARGET_RATE): Blob {
	const bytes = new ArrayBuffer(44 + samples.length * 2);
	const view = new DataView(bytes);

	const ascii = (offset: number, text: string) => {
		for (let i = 0; i < text.length; i++) view.setUint8(offset + i, text.charCodeAt(i));
	};

	ascii(0, "RIFF");
	view.setUint32(4, 36 + samples.length * 2, true);
	ascii(8, "WAVE");
	ascii(12, "fmt ");
	view.setUint32(16, 16, true);
	view.setUint16(20, 1, true);
	view.setUint16(22, 1, true);
	view.setUint32(24, sampleRate, true);
	view.setUint32(28, sampleRate * 2, true);
	view.setUint16(32, 2, true);
	view.setUint16(34, 16, true);
	ascii(36, "data");
	view.setUint32(40, samples.length * 2, true);

	let offset = 44;
	for (const sample of samples) {
		const clamped = Math.max(-1, Math.min(1, sample));
		view.setInt16(offset, clamped < 0 ? clamped * 0x8000 : clamped * 0x7fff, true);
		offset += 2;
	}
	return new Blob([bytes], { type: "audio/wav" });
}

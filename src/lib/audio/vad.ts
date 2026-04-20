/**
 * On-air voice activity detection for continuous listening.
 *
 * Keeps a single `MediaStream` open for the whole session and ships one
 * `Blob` per detected utterance via the `onUtterance` callback. The caller
 * then hands the blob to `npc_ask_voice` — no changes to the backend
 * pipeline are needed, each utterance looks exactly like a push-to-talk
 * recording.
 *
 * State machine:
 *   silent → speaking     (RMS ≥ threshold, start `MediaRecorder` now;
 *                          we accept a ~50 ms attack lag equal to the
 *                          polling interval so the first syllable isn't
 *                          clipped).
 *   speaking → silent     (RMS < threshold for RELEASE_MS, stop recorder
 *                          and ship the blob if it exceeded MIN_UTTERANCE_MS).
 *
 * Echo avoidance: the caller passes `isSuppressed()`; when it returns
 * `true` (Zee is speaking), new utterances don't begin. An in-flight
 * utterance is allowed to finish so a mid-sentence TTS start doesn't
 * truncate the user.
 *
 * All thresholds are hard-coded for now — we can expose a settings UI in
 * a follow-up if users report they're wrong on their mic. Defaults tuned
 * for a typical laptop mic at ~30 cm.
 */

export interface OnAirHandle {
  stop(): Promise<void>;
  /** True when `MediaRecorder` is currently recording an utterance. */
  isRecording(): boolean;
}

export interface OnAirOptions {
  /** Called once per detected utterance with the finalized blob. */
  onUtterance: (blob: Blob, mimeType: string) => void;
  /**
   * When this returns `true` the VAD stops starting new utterances (echo
   * avoidance while Zee speaks). An already-started recording is allowed
   * to finish.
   */
  isSuppressed?: () => boolean;
  /** Optional error hook (mic denied, recorder failures, etc). */
  onError?: (err: Error) => void;
  /** Optional transition hook, useful for debug UI. */
  onStateChange?: (state: "silent" | "speaking") => void;
}

const ATTACK_MS = 0;
const RELEASE_MS = 600;
const MIN_UTTERANCE_MS = 200;
const POLL_MS = 50;
const RMS_THRESHOLD = 0.02;
const MIN_BLOB_BYTES = 1_500;

export async function startOnAir(opts: OnAirOptions): Promise<OnAirHandle> {
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: {
      echoCancellation: true,
      noiseSuppression: true,
      autoGainControl: true,
    },
  });

  const AudioCtx =
    window.AudioContext ||
    (window as unknown as { webkitAudioContext: typeof AudioContext })
      .webkitAudioContext;
  const ctx = new AudioCtx();
  const source = ctx.createMediaStreamSource(stream);
  const analyser = ctx.createAnalyser();
  analyser.fftSize = 1024;
  source.connect(analyser);
  const buf = new Float32Array(analyser.fftSize);

  const mimeType = MediaRecorder.isTypeSupported("audio/webm;codecs=opus")
    ? "audio/webm;codecs=opus"
    : MediaRecorder.isTypeSupported("audio/webm")
      ? "audio/webm"
      : "";

  let state: "silent" | "speaking" = "silent";
  let speakingSince = 0;
  let silentSince = 0;
  let utteranceStartedAt = 0;
  let recorder: MediaRecorder | null = null;
  let chunks: Blob[] = [];
  let stopped = false;
  let intervalId: number | null = null;

  function beginUtterance() {
    if (recorder) return;
    chunks = [];
    try {
      recorder = mimeType
        ? new MediaRecorder(stream, { mimeType })
        : new MediaRecorder(stream);
    } catch (e) {
      opts.onError?.(e as Error);
      recorder = null;
      return;
    }
    const rec = recorder;
    rec.ondataavailable = (ev) => {
      if (ev.data.size > 0) chunks.push(ev.data);
    };
    rec.onstop = () => {
      const duration = performance.now() - utteranceStartedAt;
      const capturedChunks = chunks;
      chunks = [];
      recorder = null;
      if (stopped) return;
      if (duration < MIN_UTTERANCE_MS) return;
      if (capturedChunks.length === 0) return;
      const type = rec.mimeType || "audio/webm";
      const blob = new Blob(capturedChunks, { type });
      if (blob.size < MIN_BLOB_BYTES) return;
      try {
        opts.onUtterance(blob, type);
      } catch (e) {
        opts.onError?.(e as Error);
      }
    };
    utteranceStartedAt = performance.now();
    try {
      rec.start();
    } catch (e) {
      opts.onError?.(e as Error);
      recorder = null;
      return;
    }
    opts.onStateChange?.("speaking");
  }

  function endUtterance() {
    if (!recorder) return;
    try {
      recorder.stop();
    } catch {
      /* ignore — recorder will be nulled in onstop */
    }
    opts.onStateChange?.("silent");
  }

  function measureRms(): number {
    analyser.getFloatTimeDomainData(buf);
    let sum = 0;
    for (let i = 0; i < buf.length; i++) {
      sum += buf[i] * buf[i];
    }
    return Math.sqrt(sum / buf.length);
  }

  function tick() {
    if (stopped) return;
    const rms = measureRms();
    const now = performance.now();
    const suppressed = opts.isSuppressed?.() ?? false;

    if (rms > RMS_THRESHOLD) {
      silentSince = 0;
      if (state === "silent") {
        if (speakingSince === 0) speakingSince = now;
        if (now - speakingSince >= ATTACK_MS && !suppressed) {
          state = "speaking";
          beginUtterance();
        }
      }
    } else {
      speakingSince = 0;
      if (state === "speaking") {
        if (silentSince === 0) silentSince = now;
        if (now - silentSince >= RELEASE_MS) {
          state = "silent";
          silentSince = 0;
          endUtterance();
        }
      }
    }
  }

  intervalId = window.setInterval(tick, POLL_MS);

  return {
    isRecording: () => recorder !== null,
    async stop() {
      stopped = true;
      if (intervalId !== null) {
        window.clearInterval(intervalId);
        intervalId = null;
      }
      if (recorder) {
        try {
          recorder.stop();
        } catch {
          /* ignore */
        }
        recorder = null;
      }
      stream.getTracks().forEach((t) => t.stop());
      try {
        await ctx.close();
      } catch {
        /* ignore */
      }
    },
  };
}

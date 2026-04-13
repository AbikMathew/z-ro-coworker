/**
 * Audio pipelines for Gemini Live.
 *
 *   MicCapture  — getUserMedia → AudioContext → AudioWorklet → 16 kHz PCM16LE → callback
 *   AudioPlayer — receives base64 PCM16LE at 24 kHz → decodes → queued AudioBuffer playback
 *
 * Why two different sample rates?  Gemini Live accepts 16 kHz from the client
 * and emits 24 kHz back. We keep two AudioContexts so neither pipeline has to
 * resample the other.
 */

import workletUrl from "./worklet/pcm-capture-processor.js?url";

// ============================================================================
//  Mic capture
// ============================================================================

export interface MicCaptureOptions {
  /** Called with each ~100 ms chunk of base64 PCM16LE @16 kHz. */
  onChunk: (base64Pcm16k: string) => void;
}

const TARGET_RATE = 16_000;

export class MicCapture {
  private stream: MediaStream | null = null;
  private audioCtx: AudioContext | null = null;
  private sourceNode: MediaStreamAudioSourceNode | null = null;
  private workletNode: AudioWorkletNode | null = null;
  private fallbackScriptNode: ScriptProcessorNode | null = null;

  constructor(private readonly opts: MicCaptureOptions) {}

  async start(): Promise<void> {
    console.log("[MicCapture] start() called");

    // Step 1: getUserMedia
    console.log("[MicCapture] requesting getUserMedia...");
    this.stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    const tracks = this.stream.getAudioTracks();
    console.log(
      "[MicCapture] getUserMedia OK — tracks:",
      tracks.length,
      "label:",
      tracks[0]?.label,
      "enabled:",
      tracks[0]?.enabled,
      "readyState:",
      tracks[0]?.readyState,
    );

    // Step 2: AudioContext
    this.audioCtx = new AudioContext();
    console.log(
      "[MicCapture] AudioContext created — state:",
      this.audioCtx.state,
      "sampleRate:",
      this.audioCtx.sampleRate,
    );
    if (this.audioCtx.state === "suspended") {
      console.log("[MicCapture] AudioContext is suspended, attempting resume...");
      await this.audioCtx.resume();
      console.log("[MicCapture] AudioContext after resume:", this.audioCtx.state);
    }

    this.sourceNode = this.audioCtx.createMediaStreamSource(this.stream);
    console.log("[MicCapture] MediaStreamSource created");

    let chunkCount = 0;
    try {
      console.log("[MicCapture] loading AudioWorklet from:", workletUrl);
      await this.audioCtx.audioWorklet.addModule(workletUrl);
      console.log("[MicCapture] AudioWorklet loaded successfully");
      this.workletNode = new AudioWorkletNode(this.audioCtx, "pcm-capture-processor");
      this.workletNode.port.onmessage = (ev) => {
        chunkCount++;
        if (chunkCount <= 3 || chunkCount % 50 === 0) {
          console.log(`[MicCapture] worklet chunk #${chunkCount}, samples:`, ev.data?.length);
        }
        const float32: Float32Array = ev.data;
        const downsampled = downsampleTo16k(float32, this.audioCtx!.sampleRate);
        const pcm16 = floatToPcm16(downsampled);
        this.opts.onChunk(bytesToBase64(new Uint8Array(pcm16.buffer)));
      };
      this.sourceNode.connect(this.workletNode);
      // AudioWorklet must be connected to destination to run in some browsers;
      // we route through a mute gain so nothing is audible.
      const muted = this.audioCtx.createGain();
      muted.gain.value = 0;
      this.workletNode.connect(muted).connect(this.audioCtx.destination);
      console.log("[MicCapture] AudioWorklet pipeline connected");
    } catch (e) {
      // Fallback: deprecated ScriptProcessorNode (still works everywhere).
      console.warn("[MicCapture] AudioWorklet failed, falling back to ScriptProcessor", e);
      const bufferSize = 4096;
      const script = this.audioCtx.createScriptProcessor(bufferSize, 1, 1);
      script.onaudioprocess = (ev) => {
        chunkCount++;
        if (chunkCount <= 3 || chunkCount % 50 === 0) {
          console.log(`[MicCapture] ScriptProcessor chunk #${chunkCount}`);
        }
        const input = ev.inputBuffer.getChannelData(0);
        const downsampled = downsampleTo16k(input, this.audioCtx!.sampleRate);
        const pcm16 = floatToPcm16(downsampled);
        this.opts.onChunk(bytesToBase64(new Uint8Array(pcm16.buffer)));
      };
      const muted = this.audioCtx.createGain();
      muted.gain.value = 0;
      this.sourceNode.connect(script);
      script.connect(muted).connect(this.audioCtx.destination);
      this.fallbackScriptNode = script;
      console.log("[MicCapture] ScriptProcessor fallback pipeline connected");
    }
  }

  stop(): void {
    this.workletNode?.disconnect();
    this.workletNode = null;
    this.fallbackScriptNode?.disconnect();
    this.fallbackScriptNode = null;
    this.sourceNode?.disconnect();
    this.sourceNode = null;
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    this.audioCtx?.close().catch(() => {});
    this.audioCtx = null;
  }
}

function downsampleTo16k(input: Float32Array, inputRate: number): Float32Array {
  if (inputRate === TARGET_RATE) return input;
  const ratio = inputRate / TARGET_RATE;
  const outLen = Math.floor(input.length / ratio);
  const out = new Float32Array(outLen);
  // Simple linear-interpolation resampler — good enough for voice.
  for (let i = 0; i < outLen; i++) {
    const srcIdx = i * ratio;
    const left = Math.floor(srcIdx);
    const right = Math.min(left + 1, input.length - 1);
    const frac = srcIdx - left;
    out[i] = input[left] * (1 - frac) + input[right] * frac;
  }
  return out;
}

function floatToPcm16(input: Float32Array): Int16Array {
  const out = new Int16Array(input.length);
  for (let i = 0; i < input.length; i++) {
    const s = Math.max(-1, Math.min(1, input[i]));
    out[i] = s < 0 ? s * 0x8000 : s * 0x7fff;
  }
  return out;
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode.apply(
      null,
      Array.from(bytes.subarray(i, i + chunk))
    );
  }
  return btoa(binary);
}

// ============================================================================
//  Audio playback (model output)
// ============================================================================

export class AudioPlayer {
  private audioCtx: AudioContext | null = null;
  private nextStartTime = 0;

  /**
   * Create + unlock the AudioContext. MUST be called from a user gesture
   * handler (click/touch) on WKWebView/Safari, otherwise the context stays
   * `suspended` forever and BufferSource.start() silently produces no sound.
   *
   * Safe to call multiple times — idempotent.
   */
  async start(): Promise<void> {
    console.log("[AudioPlayer] start() called, current ctx state:", this.audioCtx?.state ?? "null");
    if (!this.audioCtx || this.audioCtx.state === "closed") {
      // IMPORTANT: do NOT pass `sampleRate: 24000`. WKWebView/Safari will
      // refuse or silently fail to create a context at a non-native rate on
      // some macOS versions. We let the context run at the device's native
      // rate (44.1 / 48 kHz) and let the Web Audio graph resample the 24 kHz
      // PCM buffer automatically during playback.
      this.audioCtx = new AudioContext();
      this.nextStartTime = 0;
      console.log("[AudioPlayer] new AudioContext created — state:", this.audioCtx.state, "sampleRate:", this.audioCtx.sampleRate);
    }
    if (this.audioCtx.state === "suspended") {
      try {
        console.log("[AudioPlayer] resuming suspended context...");
        await this.audioCtx.resume();
        console.log("[AudioPlayer] resume() OK — state:", this.audioCtx.state);
      } catch (e) {
        console.warn("[AudioPlayer] resume() failed:", e);
      }
    }
  }

  enqueueBase64Pcm(base64: string): void {
    if (!this.audioCtx || this.audioCtx.state === "closed") {
      // No unlocked context yet — audio arrived before the user interacted.
      // Drop the chunk; caller should have called start() inside a gesture.
      console.warn(
        "[AudioPlayer] dropping audio chunk: context not started " +
          "(call audioPlayer.start() from a click handler first)",
      );
      return;
    }
    if (this.audioCtx.state === "suspended") {
      // Kick the context. In Chrome this often works outside a gesture if the
      // page has already had interaction; on WKWebView it'll just stay
      // suspended but we still try.
      void this.audioCtx.resume();
    }

    const ctx = this.audioCtx;
    const bytes = base64ToBytes(base64);
    const pcm16 = new Int16Array(bytes.buffer, bytes.byteOffset, bytes.byteLength / 2);
    const float32 = pcm16ToFloat32(pcm16);
    // Buffer is always 24 kHz (the rate the model emits). Web Audio will
    // resample on the fly to the context's native rate during playback.
    const buffer = ctx.createBuffer(1, float32.length, 24_000);
    buffer.copyToChannel(float32, 0);

    const src = ctx.createBufferSource();
    src.buffer = buffer;
    src.connect(ctx.destination);

    const startAt = Math.max(this.nextStartTime, ctx.currentTime);
    src.start(startAt);
    this.nextStartTime = startAt + buffer.duration;
  }

  /** Drop the playback queue (e.g. on `serverContent.interrupted`). */
  clear(): void {
    if (!this.audioCtx) return;
    this.nextStartTime = this.audioCtx.currentTime;
  }

  close(): void {
    this.audioCtx?.close().catch(() => {});
    this.audioCtx = null;
    this.nextStartTime = 0;
  }
}

function pcm16ToFloat32(pcm: Int16Array): Float32Array {
  const out = new Float32Array(pcm.length);
  for (let i = 0; i < pcm.length; i++) {
    out[i] = pcm[i] / (pcm[i] < 0 ? 0x8000 : 0x7fff);
  }
  return out;
}

function base64ToBytes(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

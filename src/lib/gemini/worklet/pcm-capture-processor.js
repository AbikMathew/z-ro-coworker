// @ts-nocheck
/**
 * AudioWorklet processor: posts incoming Float32 mono samples to the main
 * thread in fixed-size chunks. Main thread handles resampling + PCM16LE
 * encoding + base64 so we keep this processor dead simple.
 *
 * This file runs in the AudioWorklet global scope (not the main window), so
 * globals like `AudioWorkletProcessor` and `registerProcessor` exist at
 * runtime but aren't in the default TS lib — suppress type-checking here.
 *
 * Buffer size: ~100 ms at the AudioContext's native sample rate, which at
 * 48 kHz is 4800 samples per chunk. The exact duration depends on the
 * input sample rate — main thread handles the resample-to-16k math.
 */
class PcmCaptureProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.buffer = new Float32Array(4800);
    this.offset = 0;
  }

  process(inputs) {
    const input = inputs[0];
    if (!input || !input[0]) return true;
    const channel = input[0]; // mono (downmix done by Web Audio if needed)
    for (let i = 0; i < channel.length; i++) {
      this.buffer[this.offset++] = channel[i];
      if (this.offset >= this.buffer.length) {
        // Transfer the filled buffer — allocate a fresh one for the next batch
        this.port.postMessage(this.buffer, [this.buffer.buffer]);
        this.buffer = new Float32Array(4800);
        this.offset = 0;
      }
    }
    return true;
  }
}

registerProcessor("pcm-capture-processor", PcmCaptureProcessor);

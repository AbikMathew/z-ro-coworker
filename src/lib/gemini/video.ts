/**
 * FrameCapture — grabs frames from a MediaStream at a fixed FPS, encodes each
 * as JPEG, and pushes the base64 payload to a callback.
 *
 * Used for both screen-share (getDisplayMedia) and webcam (getUserMedia) —
 * the pipeline is identical, only the source stream differs.
 *
 * At 1 FPS + quality 0.7 a 1440×900 screen frame lands around 60–120 KB
 * base64 — well under Gemini Live's per-message limit.
 */

export interface FrameCaptureOptions {
  stream: MediaStream;
  fps?: number;
  quality?: number;
  onFrame: (base64Jpeg: string) => void;
}

export class FrameCapture {
  private video: HTMLVideoElement | null = null;
  private canvas: HTMLCanvasElement | null = null;
  private timer: number | null = null;

  constructor(private readonly opts: FrameCaptureOptions) {}

  async start(): Promise<void> {
    const video = document.createElement("video");
    video.playsInline = true;
    video.muted = true;
    video.srcObject = this.opts.stream;
    await video.play().catch(() => {});
    this.video = video;

    this.canvas = document.createElement("canvas");

    const fps = this.opts.fps ?? 1;
    const intervalMs = Math.max(16, Math.floor(1000 / fps));
    this.timer = window.setInterval(() => void this.captureFrame(), intervalMs);
  }

  stop(): void {
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
    if (this.video) {
      this.video.srcObject = null;
      this.video = null;
    }
    this.canvas = null;
  }

  private async captureFrame(): Promise<void> {
    const video = this.video;
    const canvas = this.canvas;
    if (!video || !canvas) return;
    if (video.readyState < 2 || video.videoWidth === 0) return;

    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.drawImage(video, 0, 0);

    const quality = this.opts.quality ?? 0.7;
    const blob: Blob | null = await new Promise((resolve) =>
      canvas.toBlob(resolve, "image/jpeg", quality)
    );
    if (!blob) return;

    const base64 = await blobToBase64(blob);
    this.opts.onFrame(base64);
  }
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onloadend = () => {
      const result = reader.result as string;
      // result is "data:image/jpeg;base64,XXXX" — strip the prefix
      const idx = result.indexOf(",");
      resolve(idx >= 0 ? result.slice(idx + 1) : result);
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

/**
 * GeminiClient — thin WebSocket wrapper around Gemini Live BidiGenerateContent.
 *
 * Responsibilities:
 *  - Open the WebSocket and send the `setup` message (with sessionResumption +
 *    contextWindowCompression.slidingWindow so the session has no 2-min cap).
 *  - Forward incoming `serverContent` parts to `onText` / `onAudio` callbacks.
 *  - Persist the resumption handle whenever the server emits one.
 *  - On `goAway` with timeLeft < 10s, open a new WebSocket with the saved
 *    handle BEFORE the old one drops, so audio/video frames keep flowing.
 *  - On unexpected close, reconnect with the saved handle using exponential
 *    backoff up to 5 attempts.
 *
 * The client does NOT own the mic / camera / screen-share streams — the route
 * creates those and pushes chunks/frames into `sendAudioChunk` / `sendVideoFrame`.
 */

import type {
  ClientContentMessage,
  ConnectionStatus,
  RealtimeInputAudio,
  RealtimeInputVideo,
  ServerMessage,
  SetupMessage,
} from "./types";
import { clearHandle, loadHandle, parseTimeLeftMs, saveHandle } from "./session";

const WS_BASE_URL =
  "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";

/** Wire-level logging — enabled in dev builds, silent in production. */
const DEBUG_WIRE = import.meta.env.DEV;

/** Human-readable close event summary for diagnostics. */
function closeDetail(event: CloseEvent): string {
  const reason = event.reason ? ` "${event.reason}"` : "";
  return `code=${event.code}${reason}`;
}

/**
 * Redact large base64 payloads from a message before logging so the console
 * stays readable. Walks the object shallowly and replaces `data` strings >100
 * chars with a `[base64 N bytes]` placeholder.
 */
function redactForLog(obj: unknown): unknown {
  if (!obj || typeof obj !== "object") return obj;
  const clone: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
    if (k === "data" && typeof v === "string" && v.length > 100) {
      clone[k] = `[base64 ${v.length} chars]`;
    } else if (v && typeof v === "object") {
      clone[k] = redactForLog(v);
    } else {
      clone[k] = v;
    }
  }
  return clone;
}

function logSend(obj: unknown): void {
  if (!DEBUG_WIRE) return;
  // eslint-disable-next-line no-console
  console.log("%c→ gemini", "color:#60a5fa", redactForLog(obj));
}

function logRecv(obj: unknown): void {
  if (!DEBUG_WIRE) return;
  // eslint-disable-next-line no-console
  console.log("%c← gemini", "color:#34d399", redactForLog(obj));
}

const DEFAULT_MODEL = "models/gemini-3.1-flash-live-preview";

const DEFAULT_SYSTEM_INSTRUCTION =
  "You are Zee, a friendly co-worker helping a student with workplace tasks. Keep answers short, concrete, and actionable.";

const RECONNECT_BACKOFFS_MS = [500, 1_000, 2_000, 4_000, 8_000];
const GOAWAY_THRESHOLD_MS = 10_000;

/**
 * WebSocket close codes that indicate the SESSION itself is broken — meaning
 * the saved resumption handle is almost certainly the problem. On these codes
 * we clear the handle and retry with a fresh session instead of hammering the
 * server with more handle-based setup attempts (which cause 400/409 storms).
 *
 *   1002 protocol error
 *   1007 invalid frame payload
 *   1008 policy violation (Google uses this for auth / session conflicts)
 *   1011 internal server error (often: handle invalid on server side)
 */
const SESSION_BROKEN_CLOSE_CODES = new Set([1002, 1007, 1008, 1011]);

export interface WireLogEntry {
  direction: "send" | "recv";
  ts: number;
  /** Redacted JSON-like payload safe to stringify (base64 data replaced with placeholders). */
  payload: unknown;
}

export interface GeminiClientOptions {
  apiKey: string;
  model?: string;
  systemInstruction?: string;
  onText?: (text: string) => void;
  onAudio?: (base64Pcm24k: string) => void;
  onInterrupted?: () => void;
  onStatus?: (status: ConnectionStatus, detail?: string) => void;
  onTurnComplete?: () => void;
  /** Fired for every send/recv message after redaction — used by the in-app debug panel. */
  onWireLog?: (entry: WireLogEntry) => void;
}

export class GeminiClient {
  private ws: WebSocket | null = null;
  private readonly opts: Required<
    Pick<GeminiClientOptions, "apiKey" | "model" | "systemInstruction">
  > &
    Omit<GeminiClientOptions, "apiKey" | "model" | "systemInstruction">;

  private setupAckPromise: Promise<void> | null = null;
  private setupAckResolve: (() => void) | null = null;
  private setupAckReject: ((e: Error) => void) | null = null;

  private reconnectAttempt = 0;
  private closedByUser = false;
  private isSwapping = false;
  /** Whether the most recent connect() tried to resume a saved handle. */
  private lastConnectUsedHandle = false;
  /** True once we've seen setupComplete on the current socket. */
  private setupAcked = false;

  constructor(opts: GeminiClientOptions) {
    this.opts = {
      apiKey: opts.apiKey,
      model: opts.model ?? DEFAULT_MODEL,
      systemInstruction: opts.systemInstruction ?? DEFAULT_SYSTEM_INSTRUCTION,
      onText: opts.onText,
      onAudio: opts.onAudio,
      onInterrupted: opts.onInterrupted,
      onStatus: opts.onStatus,
      onTurnComplete: opts.onTurnComplete,
      onWireLog: opts.onWireLog,
    };
  }

  /** Open a fresh WebSocket, send setup, wait for setupComplete. */
  async connect(): Promise<void> {
    this.closedByUser = false;
    this.emitStatus("connecting");
    const handle = loadHandle() ?? undefined;
    this.lastConnectUsedHandle = handle !== undefined;
    this.setupAcked = false;
    const ws = this.openSocket();
    try {
      await this.attachAndSetup(ws, handle);
      this.ws = ws;
      this.reconnectAttempt = 0;
      this.emitStatus("connected");
    } catch {
      // WebSocket closed before setupComplete — onclose already fired and
      // handleUnexpectedClose has scheduled a retry (or given up). Just return.
    }
  }

  /** Close permanently — no reconnect. */
  close(): void {
    this.closedByUser = true;
    if (this.ws) {
      try {
        this.ws.close();
      } catch {
        // ignore
      }
      this.ws = null;
    }
    this.emitStatus("closed");
  }

  /**
   * Send a user text message via `clientContent`.
   *
   * The Google GenAI JS SDK is explicit: `sendRealtimeInput` is for audio/video
   * BLOBS only (and only a subset of mimeTypes). For text — "anything that can't
   * be represented as a Blob" — the SDK always uses `sendClientContent`, which
   * maps to the `clientContent` wire key with `turns` + `turnComplete: true`.
   *
   * `realtimeInput.text` is only used on Vertex AI, not the Gemini API endpoint.
   */
  sendText(text: string): void {
    const msg: ClientContentMessage = {
      clientContent: {
        turns: [{ role: "user", parts: [{ text }] }],
        turnComplete: true,
      },
    };
    this.sendJson(msg);
  }

  sendAudioChunk(base64Pcm16k: string): void {
    const msg: RealtimeInputAudio = {
      realtimeInput: {
        audio: { data: base64Pcm16k, mimeType: "audio/pcm;rate=16000" },
      },
    };
    this.sendJson(msg);
  }

  sendVideoFrame(base64Jpeg: string): void {
    const msg: RealtimeInputVideo = {
      realtimeInput: {
        video: { data: base64Jpeg, mimeType: "image/jpeg" },
      },
    };
    this.sendJson(msg);
  }

  // ----- internal helpers -----

  private openSocket(): WebSocket {
    const url = `${WS_BASE_URL}?key=${encodeURIComponent(this.opts.apiKey)}`;
    return new WebSocket(url);
  }

  private attachAndSetup(ws: WebSocket, handle: string | undefined): Promise<void> {
    this.setupAckPromise = new Promise<void>((resolve, reject) => {
      this.setupAckResolve = resolve;
      this.setupAckReject = reject;
    });

    ws.onopen = () => {
      const setup: SetupMessage = {
        setup: {
          model: this.opts.model,
          // Live API only supports AUDIO as a responseModality.
          // Text transcription of the output is requested separately via
          // outputAudioTranscription (added after confirmed connection works).
          generationConfig: { responseModalities: ["AUDIO"] },
          systemInstruction: {
            parts: [{ text: this.opts.systemInstruction }],
          },
          // Context window compression extends sessions past the 2-min video /
          // 15-min audio-only cap. targetTokens ~128k ≈ 85 min of audio.
          contextWindowCompression: { slidingWindow: { targetTokens: 128000 } },
          ...(handle ? { sessionResumption: { handle } } : { sessionResumption: {} }),
        },
      };
      ws.send(JSON.stringify(setup));
    };

    ws.onmessage = (event) => this.handleMessage(event, ws);

    ws.onerror = (event) => {
      this.emitStatus("error", `WebSocket error (see console)`);
      console.error("[GeminiClient] WebSocket error:", event);
    };

    ws.onclose = (event) => {
      const detail = closeDetail(event);
      // Reject the pending setup promise so connect() can return.
      if (this.setupAckReject) {
        const rej = this.setupAckReject;
        this.setupAckResolve = null;
        this.setupAckReject = null;
        rej(new Error(`WS closed before setup: ${detail}`));
      }
      if (this.closedByUser || this.isSwapping) return;
      this.handleUnexpectedClose(event);
    };

    return this.setupAckPromise;
  }

  private async handleMessage(event: MessageEvent, owningSocket: WebSocket): Promise<void> {
    let text: string;
    if (typeof event.data === "string") {
      text = event.data;
    } else if (event.data instanceof Blob) {
      text = await event.data.text();
    } else if (event.data instanceof ArrayBuffer) {
      text = new TextDecoder().decode(event.data);
    } else {
      return;
    }

    let msg: ServerMessage;
    try {
      msg = JSON.parse(text) as ServerMessage;
    } catch {
      if (DEBUG_WIRE) console.warn("[GeminiClient] non-JSON server message:", text);
      return;
    }

    logRecv(msg);
    this.opts.onWireLog?.({ direction: "recv", ts: Date.now(), payload: redactForLog(msg) });

    if (msg.setupComplete) {
      this.setupAcked = true;
      this.setupAckResolve?.();
      this.setupAckResolve = null;
      this.setupAckReject = null;
      return;
    }

    if (msg.sessionResumptionUpdate?.resumable && msg.sessionResumptionUpdate.newHandle) {
      saveHandle(msg.sessionResumptionUpdate.newHandle);
    }

    if (msg.goAway) {
      const timeLeftMs = parseTimeLeftMs(msg.goAway.timeLeft);
      if (timeLeftMs < GOAWAY_THRESHOLD_MS) {
        void this.swapToNewSocket(owningSocket);
      }
    }

    if (msg.serverContent) {
      // Audio parts (inlineData) + inline text parts from modelTurn
      const parts = msg.serverContent.modelTurn?.parts ?? [];
      for (const part of parts) {
        if (part.text) {
          this.opts.onText?.(part.text);
        }
        if (part.inlineData?.data && part.inlineData.mimeType.startsWith("audio/")) {
          this.opts.onAudio?.(part.inlineData.data);
        }
      }
      // Transcription of the model's audio output (when outputAudioTranscription is enabled)
      if (msg.serverContent.outputTranscription?.text) {
        this.opts.onText?.(msg.serverContent.outputTranscription.text);
      }
      if (msg.serverContent.interrupted) {
        this.opts.onInterrupted?.();
      }
      if (msg.serverContent.turnComplete) {
        this.opts.onTurnComplete?.();
      }
    }
  }

  /**
   * Open a NEW socket with the saved handle, wait for setupComplete, then
   * atomically replace `this.ws`. The old socket will close on its own shortly
   * after (server-initiated). During the swap we briefly hold `isSwapping = true`
   * so the old socket's onclose doesn't trigger a reconnect.
   */
  private async swapToNewSocket(oldSocket: WebSocket): Promise<void> {
    if (this.isSwapping) return;
    this.isSwapping = true;
    this.emitStatus("reconnecting", "goAway — swapping sockets");
    try {
      const handle = loadHandle() ?? undefined;
      const next = this.openSocket();
      await this.attachAndSetup(next, handle);
      this.ws = next;
      this.reconnectAttempt = 0; // successful swap — reset backoff counter
      this.emitStatus("connected");
      try {
        oldSocket.close();
      } catch {
        // ignore
      }
    } catch (e) {
      // goAway handoff failed — the handle was rejected on the new socket.
      // Drop it so the old socket's impending onclose → handleUnexpectedClose
      // path tries a fresh session instead of looping on the bad handle.
      clearHandle();
      this.emitStatus(
        "reconnecting",
        `goAway swap failed (${String(e)}) — handle cleared, will retry fresh`,
      );
    } finally {
      this.isSwapping = false;
    }
  }

  private handleUnexpectedClose(event: CloseEvent): void {
    const detail = closeDetail(event);

    // Fast-path: the saved handle is poisoned. We drop it NOW (before burning
    // more retry attempts that will all fail the same way) if either:
    //   (a) The close code says "session broken" (1002/1007/1008/1011), OR
    //   (b) We tried a handle, never got setupComplete, and it closed.
    // Case (b) is the classic "ghost session" — a previous disconnect left
    // the handle in limbo on Google's side, now it's rejected on reopen.
    const handleLikelyPoisoned =
      (this.lastConnectUsedHandle && !this.setupAcked) ||
      SESSION_BROKEN_CLOSE_CODES.has(event.code);

    if (handleLikelyPoisoned && loadHandle()) {
      clearHandle();
      this.emitStatus(
        "reconnecting",
        `session handle looked poisoned (${detail}) — starting fresh`,
      );
      this.reconnectAttempt = 0;
      setTimeout(() => {
        if (this.closedByUser) return;
        void this.connect();
      }, 250);
      return;
    }

    const attempt = this.reconnectAttempt + 1;
    const backoff =
      RECONNECT_BACKOFFS_MS[Math.min(this.reconnectAttempt, RECONNECT_BACKOFFS_MS.length - 1)];
    this.reconnectAttempt += 1;

    if (this.reconnectAttempt > RECONNECT_BACKOFFS_MS.length) {
      // Out of retries. Clear the handle and kick one final fresh attempt
      // before surfacing an error — gives the user a chance to recover
      // without a manual page refresh.
      clearHandle();
      this.reconnectAttempt = 0;
      this.emitStatus(
        "reconnecting",
        `max attempts with handle reached (${detail}) — last try with fresh session`,
      );
      setTimeout(() => {
        if (this.closedByUser) return;
        void this.connect();
      }, 1_000);
      return;
    }

    this.emitStatus("reconnecting", `attempt ${attempt}/${RECONNECT_BACKOFFS_MS.length} — ${detail}`);
    setTimeout(() => {
      if (this.closedByUser) return;
      void this.connect();
    }, backoff);
  }

  private sendJson(obj: unknown): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      const redacted = redactForLog(obj);
      if (DEBUG_WIRE) {
        console.warn(
          "[GeminiClient] dropped outgoing message (ws not open, state=" +
          (this.ws?.readyState ?? "null") +
          ")",
          redacted,
        );
      }
      this.opts.onWireLog?.({ direction: "send", ts: Date.now(), payload: { dropped: true, reason: "ws-not-open", msg: redacted } });
      return;
    }
    const redacted = redactForLog(obj);
    logSend(obj);
    this.opts.onWireLog?.({ direction: "send", ts: Date.now(), payload: redacted });
    this.ws.send(JSON.stringify(obj));
  }

  private emitStatus(status: ConnectionStatus, detail?: string): void {
    this.opts.onStatus?.(status, detail);
  }
}

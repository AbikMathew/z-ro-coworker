<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { GeminiClient, type WireLogEntry } from "$lib/gemini/client";
  import { MicCapture, AudioPlayer } from "$lib/gemini/audio";
  import { FrameCapture } from "$lib/gemini/video";
  import type { ConnectionStatus } from "$lib/gemini/types";
  import { clearHandle } from "$lib/gemini/session";
  import AvatarPlaceholder from "$lib/components/coworker/AvatarPlaceholder.svelte";
  import ResponsePanel from "$lib/components/coworker/ResponsePanel.svelte";
  import BottomBar from "$lib/components/coworker/BottomBar.svelte";

  // ----- Connection state -----
  let client: GeminiClient | null = null;
  let status: ConnectionStatus = $state("idle");
  let statusDetail: string | undefined = $state(undefined);
  let errorMsg: string | null = $state(null);

  // ----- Response state -----
  let responseText = $state("");
  let speaking = $state(false);
  let audioPlayer: AudioPlayer | null = null;
  let speakingResetTimer: number | null = null;

  // ----- Mode state -----
  let micOn = $state(false);
  let cameraOn = $state(false);
  let screenOn = $state(false);

  let micCapture: MicCapture | null = null;
  let cameraCapture: FrameCapture | null = null;
  let cameraStream: MediaStream | null = null;
  let screenCapture: FrameCapture | null = null;
  let screenStream: MediaStream | null = null;

  // ----- Debug panel state -----
  // Open/close is handled by native <details> element — no Svelte state needed.
  let wireLog: WireLogEntry[] = $state([]);
  const MAX_LOG_ENTRIES = 40;

  function appendWireLog(entry: WireLogEntry) {
    // Keep only the last N entries; new ones at the top for readability.
    wireLog = [entry, ...wireLog].slice(0, MAX_LOG_ENTRIES);
  }

  function formatTs(ts: number): string {
    const d = new Date(ts);
    return `${d.getHours().toString().padStart(2, "0")}:${d
      .getMinutes()
      .toString()
      .padStart(2, "0")}:${d.getSeconds().toString().padStart(2, "0")}.${d
      .getMilliseconds()
      .toString()
      .padStart(3, "0")}`;
  }

  function summarizeEntry(payload: unknown): string {
    if (!payload || typeof payload !== "object") return String(payload);
    const obj = payload as Record<string, unknown>;
    // Pick out the dominant top-level key as a quick label.
    const topKey = Object.keys(obj)[0] ?? "";
    if (topKey === "realtimeInput") {
      const ri = obj.realtimeInput as Record<string, unknown> | undefined;
      if (ri?.audio) return "realtimeInput.audio";
      if (ri?.video) return "realtimeInput.video";
      if (typeof ri?.text === "string") return `realtimeInput.text: "${ri.text}"`;
      return "realtimeInput";
    }
    if (topKey === "serverContent") {
      const sc = obj.serverContent as Record<string, unknown> | undefined;
      const flags: string[] = [];
      if (sc?.modelTurn) flags.push("modelTurn");
      if (sc?.outputTranscription) flags.push("outputTranscription");
      if (sc?.interrupted) flags.push("interrupted");
      if (sc?.turnComplete) flags.push("turnComplete");
      if (sc?.generationComplete) flags.push("generationComplete");
      return `serverContent [${flags.join(", ") || "empty"}]`;
    }
    if (topKey === "setupComplete") return "setupComplete";
    if (topKey === "setup") return "setup";
    if (topKey === "sessionResumptionUpdate") return "sessionResumptionUpdate";
    if (topKey === "goAway") return `goAway ${JSON.stringify(obj.goAway)}`;
    return topKey;
  }

  function clearWireLog() {
    wireLog = [];
  }

  function clearSessionHandle() {
    clearHandle();
    appendWireLog({
      direction: "recv",
      ts: Date.now(),
      payload: { info: "session handle cleared — reload page to start fresh" },
    });
  }

  // ----- Lifecycle -----

  onMount(async () => {
    // Step 1: fetch the API key from Rust (can fail if .env is missing).
    let apiKey: string;
    try {
      if ((window as any).__TAURI_INTERNALS__) {
        apiKey = await invoke<string>("get_gemini_api_key");
      } else {
        apiKey = import.meta.env.VITE_GEMINI_API_KEY;
        if (!apiKey) throw new Error("VITE_GEMINI_API_KEY is not defined in .env");
      }
    } catch (e) {
      errorMsg = `API key error: ${e}`;
      status = "error";
      return;
    }

    // Step 2: create client + connect. Connection drops are handled internally
    // by the client's reconnect loop — onStatus is the authoritative status source.
    audioPlayer = new AudioPlayer();
    client = new GeminiClient({
      apiKey,
      onText: (t) => {
        responseText += t;
      },
      onAudio: (pcm) => {
        audioPlayer?.enqueueBase64Pcm(pcm);
        speaking = true;
        if (speakingResetTimer !== null) window.clearTimeout(speakingResetTimer);
        speakingResetTimer = window.setTimeout(() => (speaking = false), 800);
      },
      onInterrupted: () => {
        audioPlayer?.clear();
        speaking = false;
      },
      onTurnComplete: () => {
        responseText += "\n\n";
      },
      onStatus: (s, detail) => {
        status = s;
        statusDetail = detail;
        if (s === "error") errorMsg = detail ?? "unknown error";
        else if (s === "connected") errorMsg = null; // clear on successful reconnect
      },
      onWireLog: appendWireLog,
    });

    // connect() never throws — failures are surfaced via onStatus("error", ...)
    void client.connect();
  });

  onDestroy(() => {
    void stopAllStreams();
    client?.close();
    audioPlayer?.close();
    if (speakingResetTimer !== null) window.clearTimeout(speakingResetTimer);
  });

  async function stopAllStreams() {
    if (micOn) await toggleMic();
    if (cameraOn) await toggleCamera();
    if (screenOn) await toggleScreen();
  }

  // ----- Toggles -----

  async function toggleMic() {
    if (!client) return;
    // Unlock the output AudioContext inside a user gesture. WKWebView/Safari
    // refuses to play audio from a context created outside a click handler.
    await audioPlayer?.start();
    if (micOn) {
      micCapture?.stop();
      micCapture = null;
      micOn = false;
      return;
    }
    try {
      micCapture = new MicCapture({
        onChunk: (b64) => client?.sendAudioChunk(b64),
      });
      await micCapture.start();
      micOn = true;
    } catch (e) {
      errorMsg = `Mic failed: ${e}`;
      micCapture = null;
    }
  }

  async function toggleCamera() {
    if (!client) return;
    // Unlock playback context from inside the user gesture (WKWebView).
    await audioPlayer?.start();
    if (cameraOn) {
      cameraCapture?.stop();
      cameraCapture = null;
      cameraStream?.getTracks().forEach((t) => t.stop());
      cameraStream = null;
      cameraOn = false;
      return;
    }
    try {
      cameraStream = await navigator.mediaDevices.getUserMedia({
        video: { width: 640 },
        audio: false,
      });
      cameraCapture = new FrameCapture({
        stream: cameraStream,
        fps: 1,
        quality: 0.7,
        onFrame: (b64) => client?.sendVideoFrame(b64),
      });
      await cameraCapture.start();
      cameraOn = true;
    } catch (e) {
      errorMsg = `Camera failed: ${e}`;
      cameraStream?.getTracks().forEach((t) => t.stop());
      cameraStream = null;
      cameraCapture = null;
    }
  }

  async function toggleScreen() {
    if (!client) return;
    // Unlock playback context from inside the user gesture (WKWebView).
    await audioPlayer?.start();
    if (screenOn) {
      screenCapture?.stop();
      screenCapture = null;
      screenStream?.getTracks().forEach((t) => t.stop());
      screenStream = null;
      screenOn = false;
      return;
    }
    try {
      screenStream = await navigator.mediaDevices.getDisplayMedia({
        video: true,
        audio: false,
      });
      // If the user clicks the browser's "Stop sharing" button, mirror it.
      screenStream.getVideoTracks()[0]?.addEventListener("ended", () => {
        if (screenOn) void toggleScreen();
      });
      screenCapture = new FrameCapture({
        stream: screenStream,
        fps: 1,
        quality: 0.7,
        onFrame: (b64) => client?.sendVideoFrame(b64),
      });
      await screenCapture.start();
      screenOn = true;
    } catch (e) {
      errorMsg = `Screen share failed: ${e}`;
      screenStream?.getTracks().forEach((t) => t.stop());
      screenStream = null;
      screenCapture = null;
    }
  }

  // ----- Text send -----

  function sendText(text: string) {
    if (!client) return;
    // Unlock playback context from inside the user gesture (WKWebView).
    // Fire-and-forget — start() is idempotent and resolves fast.
    void audioPlayer?.start();
    responseText += `\n> ${text}\n`;
    client.sendText(text);
  }
</script>

<main class="min-h-screen bg-gray-950 text-white pb-28">
  <div class="max-w-3xl mx-auto px-4 pt-6">
    <!-- Header -->
    <div class="flex items-center justify-between mb-6">
      <div>
        <h1 class="text-2xl font-bold">Zee — Coworker</h1>
        <p class="text-xs text-gray-500">
          Gemini Live playground · model: gemini-3.1-flash-live-preview
        </p>
      </div>
      <a
        href="/"
        class="text-xs text-gray-500 hover:text-gray-300 transition-colors"
      >
        ← back to dashboard
      </a>
    </div>

    {#if errorMsg}
      <div
        class="mb-4 text-xs text-red-400 bg-red-950/30 border border-red-900 rounded-lg px-3 py-2"
      >
        {errorMsg}
      </div>
    {/if}

    <!-- Centered stage -->
    <div class="flex flex-col items-center gap-6 py-4">
      <AvatarPlaceholder {speaking} />

      <!-- Mode toggle buttons -->
      <div class="flex items-center gap-3">
        <button
          type="button"
          onclick={toggleMic}
          class="px-4 py-2 rounded-full text-sm transition-colors {micOn
            ? 'bg-emerald-600 hover:bg-emerald-500 text-white'
            : 'bg-gray-900 hover:bg-gray-800 text-gray-300 border border-gray-800'}"
        >
          {micOn ? "Talking" : "Talk"}
        </button>
        <button
          type="button"
          onclick={toggleCamera}
          class="px-4 py-2 rounded-full text-sm transition-colors {cameraOn
            ? 'bg-emerald-600 hover:bg-emerald-500 text-white'
            : 'bg-gray-900 hover:bg-gray-800 text-gray-300 border border-gray-800'}"
        >
          {cameraOn ? "Webcam on" : "Webcam"}
        </button>
        <button
          type="button"
          onclick={toggleScreen}
          class="px-4 py-2 rounded-full text-sm transition-colors {screenOn
            ? 'bg-emerald-600 hover:bg-emerald-500 text-white'
            : 'bg-gray-900 hover:bg-gray-800 text-gray-300 border border-gray-800'}"
        >
          {screenOn ? "Sharing" : "Share Screen"}
        </button>
      </div>

      <ResponsePanel text={responseText} {status} {statusDetail} />
    </div>

    <!-- Debug panel: wire-level log of messages sent to / received from Gemini.
         Uses native <details>/<summary> so open/close works without any Svelte
         reactivity. -->
    <details class="mt-6 mb-24 group">
      <summary
        class="w-full flex items-center justify-between px-3 py-2 rounded-lg bg-gray-900 border border-gray-800 text-xs text-gray-400 hover:text-gray-200 transition-colors cursor-pointer list-none"
      >
        <span>Debug wire log ({wireLog.length})</span>
        <span class="group-open:hidden">▸</span>
        <span class="hidden group-open:inline">▾</span>
      </summary>
      <div class="mt-2 bg-gray-950 border border-gray-800 rounded-lg">
        <div class="flex items-center gap-2 px-3 py-2 border-b border-gray-800">
          <button
            type="button"
            onclick={clearWireLog}
            class="text-xs text-gray-400 hover:text-gray-200 px-2 py-1 rounded bg-gray-900 hover:bg-gray-800"
          >
            Clear log
          </button>
          <button
            type="button"
            onclick={clearSessionHandle}
            class="text-xs text-gray-400 hover:text-gray-200 px-2 py-1 rounded bg-gray-900 hover:bg-gray-800"
          >
            Clear session handle
          </button>
          <span class="text-xs text-gray-600 ml-auto"
            >newest first · max {MAX_LOG_ENTRIES}</span
          >
        </div>
        <div class="max-h-96 overflow-y-auto font-mono text-[11px]">
          {#each wireLog as entry, i (i)}
            <details class="border-b border-gray-900">
              <summary
                class="px-3 py-1.5 cursor-pointer hover:bg-gray-900 flex items-center gap-2 list-none"
              >
                <span class="text-gray-600">{formatTs(entry.ts)}</span>
                <span
                  class={entry.direction === "send"
                    ? "text-blue-400"
                    : "text-emerald-400"}
                >
                  {entry.direction === "send" ? "→" : "←"}
                </span>
                <span class="text-gray-300 truncate"
                  >{summarizeEntry(entry.payload)}</span
                >
              </summary>
              <pre
                class="px-3 py-2 text-gray-400 bg-gray-900/50 overflow-x-auto whitespace-pre-wrap">{JSON.stringify(
                  entry.payload,
                  null,
                  2,
                )}</pre>
            </details>
          {:else}
            <p class="px-3 py-4 text-gray-600 text-xs">
              No wire messages yet. Try speaking or sending a text.
            </p>
          {/each}
        </div>
      </div>
    </details>
  </div>

  <BottomBar
    {micOn}
    {cameraOn}
    {screenOn}
    onToggleMic={toggleMic}
    onToggleCamera={toggleCamera}
    onToggleScreen={toggleScreen}
    onSendText={sendText}
  />
</main>

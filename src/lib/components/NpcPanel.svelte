<script lang="ts">
  import { npc } from "$lib/npc/npcStore.svelte";
  import type { NpcStatus, ChatMessage } from "$lib/npc/types";
  import { listen } from "@tauri-apps/api/event";

  // ── Local UI state ────────────────────────────────────────────────
  let messages: ChatMessage[] = $state([
    {
      role: "npc",
      content:
        "Hey! I'm Zee, your co-worker. I can see your screen now — start a task and I'll help you through it. Ask me anything!",
    },
  ]);
  let input = $state("");
  let isThinking = $state(false);
  let chatContainer: HTMLElement | null = $state(null);

  // Voice recording state
  let mediaRecorder: MediaRecorder | null = $state(null);
  let recordedChunks: Blob[] = [];
  let isRecording = $state(false);
  let micError: string | null = $state(null);
  let currentAudio: HTMLAudioElement | null = null;

  // Derived: track the store's reactive status directly (no subscribe()).
  // Using a getter keeps the reactivity proxy alive across re-renders.
  const status = $derived<NpcStatus>(npc.status);

  // Activate NPC once on mount + subscribe to the backend's barge-in event.
  // When the user (or some other source) calls `npc_interrupt`, the backend
  // cancels the in-flight turn AND emits `npc-interrupt` so the frontend
  // can stop any HTML5 audio that's already playing client-side.
  $effect(() => {
    npc.activate().catch((e) => console.warn("[npc] Activate failed:", e));

    const unlistenPromise = listen("npc-interrupt", () => {
      stopPlayback();
      isThinking = false;
    });

    return () => {
      stopPlayback();
      stopRecordingSilently();
      unlistenPromise.then((u) => u()).catch(() => {});
    };
  });

  // ── Helpers ───────────────────────────────────────────────────────
  function stateLabel(state: NpcStatus["state"]): string {
    if (typeof state === "string") return state;
    if ("Error" in state) return `Error: ${state.Error}`;
    return "Unknown";
  }

  function stateColor(state: NpcStatus["state"]): string {
    if (state === "Idle") return "bg-gray-500";
    if (state === "Listening") return "bg-green-500 animate-pulse";
    if (state === "Thinking") return "bg-yellow-500 animate-pulse";
    if (state === "Speaking") return "bg-blue-500 animate-pulse";
    return "bg-red-500";
  }

  function scrollToBottom() {
    requestAnimationFrame(() => {
      if (chatContainer) {
        chatContainer.scrollTop = chatContainer.scrollHeight;
      }
    });
  }

  /** Convert a Blob to a base64 string (no data: prefix). */
  async function blobToBase64(blob: Blob): Promise<string> {
    const buf = await blob.arrayBuffer();
    // btoa chokes on large strings; chunk to avoid call-stack overflow.
    const bytes = new Uint8Array(buf);
    const CHUNK = 0x8000;
    let binary = "";
    for (let i = 0; i < bytes.length; i += CHUNK) {
      binary += String.fromCharCode.apply(
        null,
        Array.from(bytes.subarray(i, i + CHUNK)),
      );
    }
    return btoa(binary);
  }

  // ── Text send ─────────────────────────────────────────────────────
  async function sendMessage() {
    if (!input.trim() || isThinking) return;

    const question = input.trim();
    input = "";
    messages = [...messages, { role: "user", content: question }];
    isThinking = true;
    scrollToBottom();

    try {
      const response = await npc.askText(question);
      messages = [...messages, { role: "npc", content: response }];
    } catch (e) {
      messages = [
        ...messages,
        { role: "npc", content: `Sorry, I hit a snag: ${e}. Try again?` },
      ];
    } finally {
      isThinking = false;
      scrollToBottom();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }

  // ── Voice: push-to-talk ───────────────────────────────────────────

  /** Kick off mic capture. Called on mousedown / touchstart of the mic btn. */
  async function startRecording() {
    if (isRecording || isThinking) return;
    micError = null;

    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      // Prefer webm/opus (supported on Chromium/WebKit) — Groq accepts it.
      const mimeType = MediaRecorder.isTypeSupported("audio/webm;codecs=opus")
        ? "audio/webm;codecs=opus"
        : MediaRecorder.isTypeSupported("audio/webm")
          ? "audio/webm"
          : "";
      const recorder = mimeType
        ? new MediaRecorder(stream, { mimeType })
        : new MediaRecorder(stream);

      recordedChunks = [];
      recorder.ondataavailable = (ev) => {
        if (ev.data.size > 0) recordedChunks.push(ev.data);
      };
      recorder.onstop = async () => {
        // Release the mic track immediately
        stream.getTracks().forEach((t) => t.stop());
        await finalizeRecording(recorder.mimeType);
      };

      recorder.start();
      mediaRecorder = recorder;
      isRecording = true;

      // Tell Rust we're listening (updates NpcState for the indicator)
      npc.startListening().catch((e) =>
        console.warn("[npc] startListening failed:", e),
      );
    } catch (e) {
      micError = `Mic unavailable: ${e}`;
      console.error("[npc] getUserMedia failed:", e);
    }
  }

  /** Stop mic capture. Called on mouseup / touchend / mouseleave. */
  function stopRecording() {
    if (!isRecording || !mediaRecorder) return;
    try {
      mediaRecorder.stop();
    } catch (e) {
      console.warn("[npc] recorder.stop failed:", e);
    }
    isRecording = false;
  }

  /** Stop recorder without processing (used on unmount). */
  function stopRecordingSilently() {
    if (!mediaRecorder) return;
    try {
      mediaRecorder.ondataavailable = null;
      mediaRecorder.onstop = null;
      if (mediaRecorder.state !== "inactive") mediaRecorder.stop();
    } catch {
      /* ignore */
    }
    mediaRecorder = null;
    recordedChunks = [];
    isRecording = false;
  }

  /** Package up the recorded audio, send to Rust, play the response. */
  async function finalizeRecording(mimeType: string) {
    mediaRecorder = null;

    if (recordedChunks.length === 0) {
      await npc.stopListening().catch(() => {});
      return;
    }

    const blob = new Blob(recordedChunks, { type: mimeType || "audio/webm" });
    recordedChunks = [];

    // Very short recordings (<300ms of webm header-only data) can't be
    // transcribed; skip them.
    if (blob.size < 2_000) {
      await npc.stopListening().catch(() => {});
      return;
    }

    isThinking = true;
    scrollToBottom();

    try {
      const audioB64 = await blobToBase64(blob);
      const filename = mimeType.includes("ogg") ? "audio.ogg" : "audio.webm";

      const result = await npc.askVoice(audioB64, filename);

      if (result.user_transcript.trim()) {
        messages = [
          ...messages,
          { role: "user", content: result.user_transcript },
        ];
      }
      if (result.assistant_text.trim()) {
        messages = [
          ...messages,
          { role: "npc", content: result.assistant_text },
        ];
      }
      scrollToBottom();

      if (result.audio_b64 && result.audio_mime) {
        playAudio(result.audio_b64, result.audio_mime);
      }
    } catch (e) {
      messages = [
        ...messages,
        { role: "npc", content: `Voice turn failed: ${e}` },
      ];
    } finally {
      isThinking = false;
      scrollToBottom();
    }
  }

  function playAudio(b64: string, mime: string) {
    stopPlayback();
    const audio = new Audio(`data:${mime};base64,${b64}`);
    currentAudio = audio;
    audio.play().catch((e) => console.warn("[npc] audio play failed:", e));
  }

  function stopPlayback() {
    if (currentAudio) {
      try {
        currentAudio.pause();
        currentAudio.src = "";
      } catch {
        /* ignore */
      }
      currentAudio = null;
    }
  }

  function onMicPointerDown(e: PointerEvent) {
    e.preventDefault();
    startRecording();
  }
  function onMicPointerUp(e: PointerEvent) {
    e.preventDefault();
    stopRecording();
  }

  /** Barge-in: cancels any in-flight turn (LLM stream, TTS synthesis, audio
   *  playback) and returns the NPC to Idle. Wired to a small "Stop" button
   *  that only appears while Zee is Thinking or Speaking. */
  async function interrupt() {
    try {
      await npc.interrupt();
    } catch (e) {
      console.warn("[npc] interrupt failed:", e);
    }
    stopPlayback();
    isThinking = false;
  }

  const isBusy = $derived(
    status.state === "Thinking" || status.state === "Speaking" || isThinking,
  );
</script>

<div class="bg-gray-900 rounded-2xl border border-gray-800 flex flex-col h-96">
  <!-- Header -->
  <div class="p-4 border-b border-gray-800 flex items-center gap-3">
    <div
      class="w-8 h-8 rounded-full bg-blue-600 flex items-center justify-center text-xs font-bold"
    >
      Z
    </div>
    <div class="flex-1">
      <p class="font-medium text-sm">Zee</p>
      <p class="text-gray-500 text-xs">
        {status.model_name} &middot; {status.conversation_turns} turns
      </p>
    </div>
    <!-- Status indicator -->
    <div class="flex items-center gap-1.5">
      <div class="w-2 h-2 rounded-full {stateColor(status.state)}"></div>
      <span class="text-xs text-gray-400">{stateLabel(status.state)}</span>
    </div>
  </div>

  <!-- Context bar: what Zee can see -->
  {#if status.active_task || status.current_step}
    <div
      class="px-4 py-2 border-b border-gray-800 bg-gray-800/50 text-xs text-gray-400"
    >
      {#if status.active_task}
        <span class="text-blue-400">Task:</span> {status.active_task}
      {/if}
      {#if status.current_step}
        <span class="ml-2 text-green-400">Step:</span>
        {status.current_step.length > 60
          ? status.current_step.slice(0, 60) + "..."
          : status.current_step}
      {/if}
    </div>
  {/if}

  <!-- Mic error banner -->
  {#if micError}
    <div class="px-4 py-2 border-b border-gray-800 bg-red-900/40 text-xs text-red-300">
      {micError}
    </div>
  {/if}

  <!-- Chat messages -->
  <div
    class="flex-1 overflow-y-auto p-4 space-y-3"
    bind:this={chatContainer}
  >
    {#each messages as msg}
      <div class="flex {msg.role === 'user' ? 'justify-end' : 'justify-start'}">
        <div
          class="max-w-[80%] px-3 py-2 rounded-xl text-sm whitespace-pre-wrap {msg.role === 'user'
            ? 'bg-blue-600 text-white'
            : 'bg-gray-800 text-gray-300'}"
        >
          {msg.content}
        </div>
      </div>
    {/each}
    {#if isThinking}
      <div class="flex justify-start">
        <div class="bg-gray-800 text-gray-400 px-3 py-2 rounded-xl text-sm animate-pulse">
          Zee is thinking...
        </div>
      </div>
    {/if}
  </div>

  <!-- Input bar -->
  <div class="p-3 border-t border-gray-800">
    <div class="flex gap-2">
      <input
        type="text"
        bind:value={input}
        onkeydown={handleKeydown}
        placeholder="Ask Zee for help..."
        disabled={isRecording}
        class="flex-1 bg-gray-800 text-white rounded-lg px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-blue-600 placeholder-gray-500 disabled:opacity-50"
      />
      <!-- Push-to-talk mic: hold to speak, release to send -->
      <button
        type="button"
        onpointerdown={onMicPointerDown}
        onpointerup={onMicPointerUp}
        onpointercancel={onMicPointerUp}
        onpointerleave={isRecording ? onMicPointerUp : null}
        disabled={isThinking && !isRecording}
        aria-label="Hold to talk"
        title="Hold to talk"
        class="select-none px-3 py-2 rounded-lg text-sm transition-colors {isRecording
          ? 'bg-red-600 hover:bg-red-700 animate-pulse'
          : 'bg-gray-700 hover:bg-gray-600'} disabled:opacity-50 text-white"
      >
        {isRecording ? "● Rec" : "🎙"}
      </button>
      {#if isBusy}
        <button
          type="button"
          onclick={interrupt}
          aria-label="Stop Zee"
          title="Interrupt — stop talking / cancel the turn"
          class="bg-red-600 hover:bg-red-700 text-white px-4 py-2 rounded-lg text-sm transition-colors"
        >
          Stop
        </button>
      {:else}
        <button
          onclick={sendMessage}
          disabled={isThinking || isRecording}
          class="bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white px-4 py-2 rounded-lg text-sm transition-colors"
        >
          Send
        </button>
      {/if}
    </div>
    <p class="text-[11px] text-gray-500 mt-1.5">
      Hold the mic button to speak · release to send · click Stop to cut Zee off
    </p>
  </div>
</div>

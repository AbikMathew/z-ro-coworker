<script lang="ts">
  interface Props {
    micOn: boolean;
    cameraOn: boolean;
    screenOn: boolean;
    onToggleMic: () => void;
    onToggleCamera: () => void;
    onToggleScreen: () => void;
    onSendText: (text: string) => void;
  }

  let {
    micOn,
    cameraOn,
    screenOn,
    onToggleMic,
    onToggleCamera,
    onToggleScreen,
    onSendText,
  }: Props = $props();

  let inputText = $state("");

  function handleSend() {
    const trimmed = inputText.trim();
    if (!trimmed) return;
    onSendText(trimmed);
    inputText = "";
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }
</script>

<div
  class="fixed bottom-0 left-0 right-0 bg-gray-950/95 backdrop-blur border-t border-gray-800 px-4 py-3"
>
  <div class="max-w-3xl mx-auto flex items-center gap-2">
    <!-- Mic toggle -->
    <button
      type="button"
      onclick={onToggleMic}
      aria-label="Toggle microphone"
      aria-pressed={micOn}
      class="w-10 h-10 rounded-full flex items-center justify-center transition-colors {micOn
        ? 'bg-emerald-600 hover:bg-emerald-500 text-white'
        : 'bg-gray-800 hover:bg-gray-700 text-gray-400'}"
    >
      {#if micOn}
        <svg viewBox="0 0 24 24" class="w-5 h-5" fill="currentColor"
          ><path
            d="M12 14a3 3 0 0 0 3-3V5a3 3 0 1 0-6 0v6a3 3 0 0 0 3 3Zm5-3a5 5 0 0 1-10 0H5a7 7 0 0 0 6 6.92V21h2v-3.08A7 7 0 0 0 19 11Z"
          /></svg
        >
      {:else}
        <svg viewBox="0 0 24 24" class="w-5 h-5" fill="currentColor"
          ><path
            d="m4.3 3.3 16.4 16.4-1.4 1.4-3.9-3.9A7 7 0 0 1 13 17.92V21h-2v-3.08A7 7 0 0 1 5 11h2a5 5 0 0 0 5 5 5 5 0 0 0 2.6-.72L13.6 14.3A3 3 0 0 1 9 11v-.1L2.9 4.7Zm7.7-1.3a3 3 0 0 1 3 3v5.17l-6-6A3 3 0 0 1 12 2Z"
          /></svg
        >
      {/if}
    </button>

    <!-- Camera toggle -->
    <button
      type="button"
      onclick={onToggleCamera}
      aria-label="Toggle camera"
      aria-pressed={cameraOn}
      class="w-10 h-10 rounded-full flex items-center justify-center transition-colors {cameraOn
        ? 'bg-emerald-600 hover:bg-emerald-500 text-white'
        : 'bg-gray-800 hover:bg-gray-700 text-gray-400'}"
    >
      {#if cameraOn}
        <svg viewBox="0 0 24 24" class="w-5 h-5" fill="currentColor"
          ><path d="M17 10.5V7a1 1 0 0 0-1-1H4a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-3.5l4 4v-11Z" /></svg
        >
      {:else}
        <svg viewBox="0 0 24 24" class="w-5 h-5" fill="currentColor"
          ><path
            d="m3.3 2.3 18.4 18.4-1.4 1.4L17 18.4V17a1 1 0 0 1-1 1H6.4l-4.5-4.5 1.4-1.4ZM17 7a1 1 0 0 1 1 1v2.5l4-4v11l-4-4V13L9 4h7Z"
          /></svg
        >
      {/if}
    </button>

    <!-- Screen share toggle -->
    <button
      type="button"
      onclick={onToggleScreen}
      aria-label="Toggle screen share"
      aria-pressed={screenOn}
      class="w-10 h-10 rounded-full flex items-center justify-center transition-colors {screenOn
        ? 'bg-emerald-600 hover:bg-emerald-500 text-white'
        : 'bg-gray-800 hover:bg-gray-700 text-gray-400'}"
    >
      <svg viewBox="0 0 24 24" class="w-5 h-5" fill="currentColor"
        ><path
          d="M3 4h18a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1h-7v2h3v2H7v-2h3v-2H3a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Zm1 2v9h16V6H4Zm8 1 4 3-4 3V7Z"
        /></svg
      >
    </button>

    <!-- Text input -->
    <input
      type="text"
      bind:value={inputText}
      onkeydown={handleKeydown}
      placeholder="Type a message…"
      class="flex-1 bg-gray-900 border border-gray-800 rounded-full px-4 py-2 text-sm text-gray-100 placeholder-gray-600 outline-none focus:border-gray-600"
    />

    <!-- Send -->
    <button
      type="button"
      onclick={handleSend}
      class="px-4 py-2 rounded-full bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium transition-colors"
    >
      Send
    </button>
  </div>
</div>

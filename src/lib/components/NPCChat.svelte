<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let messages: { role: string; content: string }[] = $state([
    {
      role: "npc",
      content: "Hey! I'm Zee, your co-worker. Start a task and I'll help you through it. Ask me anything!",
    },
  ]);
  let input = $state("");
  let isThinking = $state(false);

  async function sendMessage() {
    if (!input.trim() || isThinking) return;

    const question = input.trim();
    input = "";
    messages = [...messages, { role: "user", content: question }];
    isThinking = true;

    try {
      const response = await invoke<string>("request_help", {
        question,
        taskContext: "User is asking a general question.",
      });
      messages = [...messages, { role: "npc", content: response }];
    } catch (e) {
      messages = [
        ...messages,
        { role: "npc", content: "Hmm, I'm having trouble thinking right now. Try again?" },
      ];
    } finally {
      isThinking = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }
</script>

<div class="bg-gray-900 rounded-2xl border border-gray-800 flex flex-col h-80">
  <div class="p-4 border-b border-gray-800 flex items-center gap-3">
    <div class="w-8 h-8 rounded-full bg-blue-600 flex items-center justify-center text-xs font-bold">Z</div>
    <div>
      <p class="font-medium text-sm">Zee</p>
      <p class="text-gray-500 text-xs">Your co-worker</p>
    </div>
  </div>

  <div class="flex-1 overflow-y-auto p-4 space-y-3">
    {#each messages as msg}
      <div class="flex {msg.role === 'user' ? 'justify-end' : 'justify-start'}">
        <div
          class="max-w-[80%] px-3 py-2 rounded-xl text-sm {msg.role === 'user'
            ? 'bg-blue-600 text-white'
            : 'bg-gray-800 text-gray-300'}"
        >
          {msg.content}
        </div>
      </div>
    {/each}
    {#if isThinking}
      <div class="flex justify-start">
        <div class="bg-gray-800 text-gray-400 px-3 py-2 rounded-xl text-sm">Thinking...</div>
      </div>
    {/if}
  </div>

  <div class="p-3 border-t border-gray-800">
    <div class="flex gap-2">
      <input
        type="text"
        bind:value={input}
        onkeydown={handleKeydown}
        placeholder="Ask Zee for help..."
        class="flex-1 bg-gray-800 text-white rounded-lg px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-blue-600"
      />
      <button
        onclick={sendMessage}
        disabled={isThinking}
        class="bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white px-4 py-2 rounded-lg text-sm transition-colors"
      >
        Send
      </button>
    </div>
  </div>
</div>

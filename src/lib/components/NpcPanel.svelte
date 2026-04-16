<script lang="ts">
  import { askText, refreshStatus, npcStatus, activateNpc } from "$lib/npc/npcStore";
  import type { NpcStatus, ChatMessage } from "$lib/npc/types";

  let messages: ChatMessage[] = $state([
    {
      role: "npc",
      content:
        "Hey! I'm Zee, your co-worker. I can see your screen now — start a task and I'll help you through it. Ask me anything!",
    },
  ]);
  let input = $state("");
  let isThinking = $state(false);
  let status: NpcStatus | null = $state(null);
  let chatContainer: HTMLElement | null = $state(null);

  // Subscribe to NPC status
  npcStatus.subscribe((s) => (status = s));

  // Activate NPC and fetch initial status on mount
  $effect(() => {
    activateNpc().catch((e) => console.warn("[npc] Activate failed:", e));
  });

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

  async function sendMessage() {
    if (!input.trim() || isThinking) return;

    const question = input.trim();
    input = "";
    messages = [...messages, { role: "user", content: question }];
    isThinking = true;
    scrollToBottom();

    try {
      const response = await askText(question);
      messages = [...messages, { role: "npc", content: response }];
    } catch (e) {
      messages = [
        ...messages,
        {
          role: "npc",
          content: `Sorry, I hit a snag: ${e}. Try again?`,
        },
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

  function scrollToBottom() {
    requestAnimationFrame(() => {
      if (chatContainer) {
        chatContainer.scrollTop = chatContainer.scrollHeight;
      }
    });
  }
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
        {#if status}
          {status.model_name} &middot; {status.conversation_turns} turns
        {:else}
          Initializing...
        {/if}
      </p>
    </div>
    <!-- Status indicator -->
    {#if status}
      <div class="flex items-center gap-1.5">
        <div class="w-2 h-2 rounded-full {stateColor(status.state)}"></div>
        <span class="text-xs text-gray-400">{stateLabel(status.state)}</span>
      </div>
    {/if}
  </div>

  <!-- Context bar: what Zee can see -->
  {#if status?.active_task || status?.current_step}
    <div class="px-4 py-2 border-b border-gray-800 bg-gray-800/50 text-xs text-gray-400">
      {#if status?.active_task}
        <span class="text-blue-400">Task:</span> {status.active_task}
      {/if}
      {#if status?.current_step}
        <span class="ml-2 text-green-400">Step:</span>
        {status.current_step.length > 60
          ? status.current_step.slice(0, 60) + "..."
          : status.current_step}
      {/if}
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
        class="flex-1 bg-gray-800 text-white rounded-lg px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-blue-600 placeholder-gray-500"
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

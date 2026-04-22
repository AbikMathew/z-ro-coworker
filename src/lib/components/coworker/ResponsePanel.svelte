<script lang="ts">
  import type { ConnectionStatus } from "$lib/gemini/types";

  interface Props {
    text: string;
    status: ConnectionStatus;
    statusDetail?: string;
  }
  let { text, status, statusDetail }: Props = $props();

  let statusLabel = $derived.by(() => {
    switch (status) {
      case "idle":
        return "idle";
      case "connecting":
        return "connecting…";
      case "connected":
        return "connected";
      case "reconnecting":
        return statusDetail ? `reconnecting — ${statusDetail}` : "reconnecting…";
      case "closed":
        return "closed";
      case "error":
        return statusDetail ? `error — ${statusDetail}` : "error";
    }
  });

  let statusColor = $derived.by(() => {
    switch (status) {
      case "connected":
        return "text-emerald-400";
      case "connecting":
      case "reconnecting":
        return "text-amber-400";
      case "error":
        return "text-red-400";
      default:
        return "text-gray-500";
    }
  });
</script>

<div class="w-full max-w-2xl bg-gray-900/80 border border-gray-800 rounded-2xl p-5 min-h-[120px]">
  <div class="flex items-center justify-between mb-3">
    <span class="text-xs uppercase tracking-widest text-gray-500">Zee's response</span>
    <span class="text-xs font-mono {statusColor}">{statusLabel}</span>
  </div>
  {#if text}
    <p class="text-gray-100 text-sm leading-relaxed whitespace-pre-wrap">{text}</p>
  {:else}
    <p class="text-gray-600 text-sm italic">
      Turn on a mode or type a message to start a conversation.
    </p>
  {/if}
</div>

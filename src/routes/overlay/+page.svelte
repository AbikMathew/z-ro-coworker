<script lang="ts">
  // Overlay window — renders transparent visual guides on top of other apps
  // This page runs in a separate Tauri webview window
</script>

<div class="fixed inset-0 pointer-events-none">
  <!-- Overlay elements (highlights, tooltips, arrows) will be rendered here -->
  <!-- They are driven by state from the Rust backend via events -->
  <p class="text-white text-xs opacity-50 absolute bottom-4 right-4">Z-RO Overlay Active</p>
</div>

<style>
  :global(body) {
    background: transparent !important;
    margin: 0;
    padding: 0;
    overflow: hidden;
  }
</style>

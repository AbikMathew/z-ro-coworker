<script>
  // Overlay layout - does NOT import app.css/Tailwind
  // This breaks the layout chain from root
  let { children } = $props();
</script>

{@render children()}

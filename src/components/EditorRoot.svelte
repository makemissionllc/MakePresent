<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Editor from "./Editor.svelte";
  import BrandLockup from "./BrandLockup.svelte";

  function reportError(error: unknown): void {
    const message = error instanceof Error ? error.stack ?? error.message : String(error);
    void invoke("report_frontend_status", { message: `editor render failed: ${message}` }).catch(() => {});
  }
</script>

<svelte:boundary onerror={reportError}>
  <Editor />
  {#snippet failed(error, reset)}
    <main class="startup-error">
      <BrandLockup large />
      <h1>The workspace could not be displayed</h1>
      <p>Your saved service is still on this device. Try opening the workspace again.</p>
      <pre>{error instanceof Error ? error.message : String(error)}</pre>
      <button onclick={reset}>Try again</button>
      <button onclick={() => window.location.reload()}>Reload app</button>
    </main>
  {/snippet}
</svelte:boundary>

<style>
  .startup-error { max-width: 660px; margin: 12vh auto; padding: 32px; }
  h1 { font-size: 22px; margin-top: 28px; }
  p { color: var(--text-dim); }
  pre { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 220px; overflow: auto; }
  button { margin: 12px 8px 0 0; }
</style>

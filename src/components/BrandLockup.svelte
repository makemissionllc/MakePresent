<script lang="ts">
  let { large = false }: { large?: boolean } = $props();
</script>

<span class="brand-lockup" class:large aria-label="MakrStudio by MakeMission">
  <svg class="mark" viewBox="0 0 40 40" fill="none" aria-hidden="true">
    <rect width="40" height="40" rx="11" fill="currentColor" fill-opacity=".12" />
    <path d="M10 28V12l10 11 10-11v16" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" />
  </svg>
  <span class="wordmark"><strong>MakrStudio</strong><span class="byline">by <b>MakeMission</b></span></span>
</span>

<style>
  .brand-lockup { display: inline-flex; align-items: center; gap: 9px; flex-shrink: 0; text-align: left; }
  .mark { width: 32px; height: 32px; color: var(--accent); flex: none; }
  .wordmark { display: flex; flex-direction: column; gap: 3px; }
  strong { font-family: var(--font-body); font-size: 14px; font-weight: 700; line-height: 1.1; letter-spacing: -.025em; color: var(--text); }
  .byline { font-size: 10px; font-weight: 400; line-height: 1.2; letter-spacing: .015em; color: var(--text-dim); }
  b { font-weight: 600; }
  .large { gap: 14px; }
  .large .mark { width: 48px; height: 48px; }
  .large strong { font-size: 24px; }
  .large .byline { font-size: 12px; }
</style>

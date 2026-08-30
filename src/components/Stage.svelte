<script lang="ts">
  import { onMount } from "svelte";
  import { api, subscribeState } from "../lib/sync";
  import type { ClientState } from "../lib/types";

  let appState = $state<ClientState | null>(null);
  let currentTime = $state("--:--:--");

  const current = $derived(appState?.current ?? null);
  const next = $derived(appState?.next ?? null);

  onMount(() => {
    let un: () => void = () => {};
    let clock: number | undefined;

    const tick = () => {
      currentTime = new Date().toLocaleTimeString([], { hour12: false });
    };

    void (async () => {
      un = await subscribeState((s) => {
        appState = s;
      });
      try {
        appState = await api.getState();
      } catch (e) {
        console.error("Failed to fetch initial appState", e);
      }
    })();
    tick();
    clock = window.setInterval(tick, 1000);

    return () => {
      un();
      if (clock !== undefined) window.clearInterval(clock);
    };
  });
</script>

<div class="stage">
  <section class="current">
    {#if current}
      <p class="current-body">{current.body || current.title}</p>
    {:else}
      <p class="placeholder">No live slide</p>
    {/if}
  </section>

  <aside class="side">
    <div class="next">
      <span class="next-label">NEXT</span>
      {#if next}
        <p class="next-body">{next.body || next.title}</p>
      {:else}
        <p class="placeholder">Nothing queued</p>
      {/if}
    </div>
    <div class="clock">{currentTime}</div>
  </aside>
</div>

<style>
  :global(html),
  :global(body),
  :global(#app) {
    height: 100%;
    margin: 0;
    overflow: hidden;
  }

  .stage {
    position: relative;
    display: flex;
    width: 100vw;
    height: 100vh;
    background: #0b0b0e;
    color: #f4f4f7;
    overflow: hidden;
  }

  .current {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 6vh 6vw;
    min-width: 0;
  }

  .current-body {
    font-family: system-ui, -apple-system, "Segoe UI", Ubuntu, Cantarell,
      sans-serif;
    font-size: clamp(1.5rem, 4.2vmin, 4rem);
    font-weight: 500;
    line-height: 1.45;
    margin: 0;
    max-width: 95%;
    white-space: pre-wrap;
  }

  .side {
    width: 30%;
    min-width: 240px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    border-left: 1px solid #26262e;
    padding: 3vh 2vw;
    background: #101014;
  }

  .next-label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.18em;
    color: #7f8494;
  }

  .next-body {
    font-family: system-ui, -apple-system, "Segoe UI", Ubuntu, Cantarell,
      sans-serif;
    font-size: clamp(1rem, 2.2vmin, 2rem);
    line-height: 1.4;
    color: #d6d9e2;
    white-space: pre-wrap;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 8;
    line-clamp: 8;
    -webkit-box-orient: vertical;
  }

  .clock {
    font-family: system-ui, -apple-system, "Segoe UI", Ubuntu, Cantarell,
      sans-serif;
    font-size: clamp(2rem, 6vmin, 5rem);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    text-align: left;
    color: #ffffff;
  }

  .placeholder {
    color: #555a68;
    font-size: clamp(1rem, 2vmin, 1.8rem);
    margin: 0;
  }
</style>
<script lang="ts">
  import { onMount } from "svelte";
  import { api, subscribeState } from "../lib/sync";
  import type { ClientState, Look } from "../lib/types";
  import { fitText } from "../lib/fitText";
  import SlideRender from "./SlideRender.svelte";

  let appState = $state<ClientState | null>(null);
  let currentTime = $state("--:--:--");

  const current = $derived(appState?.current ?? null);
  const next = $derived(appState?.next ?? null);

  // Resolve the Look assigned to this Stage window. Falls back to the look
  // named "Stage", then the first look, when unmapped.
  const look = $derived.by<Look | null>(() => {
    const looks = appState?.looks ?? [];
    if (looks.length === 0) return null;
    const mapped = looks.find((l) => l.id === appState?.stageLookId);
    if (mapped) return mapped;
    return looks.find((l) => l.name === "Stage") ?? looks[0]!;
  });

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
    {#if current && look}
      <SlideRender {look} slide={current} />
    {:else if current}
      <p class="placeholder">{current.body || current.title}</p>
    {:else}
      <p class="placeholder">No live slide</p>
    {/if}
  </section>

  <aside class="side">
    <div class="next">
      <span class="next-label">NEXT</span>
      {#if next}
        <div class="next-body-wrap" use:fitText>
          <p class="next-body" data-role="body">{next.body || next.title}</p>
        </div>
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
    width: 100vw;
    height: 100vh;
    margin: 0;
    padding: 0;
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
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .side {
    position: relative;
    z-index: 2;
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

  .next-body-wrap {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    align-items: center;
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
    margin: 0;
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
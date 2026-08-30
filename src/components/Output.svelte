<script lang="ts">
  import { onMount } from "svelte";
  import { api, subscribeState } from "../lib/sync";
  import type { ClientState, Slide } from "../lib/types";

  const FADE_MS = 400;

  // The slide currently occupying the Output (or null = black screen).
  let shown = $state<Slide | null>(null);
  // A copy of the outgoing slide that fades away to reveal the incoming one.
  let leaving = $state<Slide | null>(null);
  // "in" hides the incoming slide (opacity 0) until the crossfade ends.
  let dim = $state(false);
  // "out" fades the outgoing slide from opaque to transparent.
  let out = $state(false);
  let appState = $state<ClientState | null>(null);
  let timer: number | undefined;

  const project = $derived(appState?.project ?? null);
  const live = $derived(
    project
      ? (project.slides.find((s) => s.id === project.live) ?? null)
      : null,
  );
  const transition = $derived(project?.transition ?? "cut");

  function solidColor(slide: Slide | null): string {
    return slide && slide.background.type === "solid"
      ? slide.background.color
      : "#000000";
  }

  // The Output is a dumb renderer: it only choreographs the fade. The backend
  // decides which slide is live; here we layer the old slide on top of the new
  // one and crossfade between them when the project's transition is "fade".
  $effect(() => {
    const next = live;
    const prev = shown;
    if (next?.id === prev?.id && (next === null) === (prev === null)) return;

    if (transition === "fade") {
      leaving = prev;
      out = false;
      shown = next;
      dim = true;
      // Force style recompute so both transitions start from a clean state.
      void document.body.offsetWidth;
      requestAnimationFrame(() => {
        out = true;
        dim = false;
      });
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        leaving = null;
        out = false;
        dim = false;
      }, FADE_MS + 40);
    } else {
      leaving = null;
      out = false;
      shown = next;
      dim = false;
      window.clearTimeout(timer);
    }
  });

  onMount(() => {
    let un: () => void = () => {};
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
    return () => {
      un();
      window.clearTimeout(timer);
    };
  });
</script>

{#snippet slideMarkup(slide: Slide)}
  {#if slide.title}
    <h1 class="title">{slide.title}</h1>
  {/if}
  {#if slide.body}
    <p class="body">{slide.body}</p>
  {/if}
{/snippet}

<main class="stage">
  {#if shown}
    <div class="slide" class:in={dim} style:background-color={solidColor(shown)}>
      {@render slideMarkup(shown)}
    </div>
  {:else if !leaving}
    <div class="offline"></div>
  {/if}

  {#if leaving}
    <div class="leaving" class:out={out} style:background-color={solidColor(leaving)}>
      {@render slideMarkup(leaving)}
    </div>
  {/if}
</main>

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
    width: 100vw;
    height: 100vh;
    background: #000;
  }

  .slide,
  .leaving,
  .offline {
    position: absolute;
    inset: 0;
  }

  .slide,
  .leaving {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2.5vh;
    padding: 8vh 10vw;
    text-align: center;
    color: #ffffff;
    opacity: 1;
    transition: opacity 400ms ease;
  }

  .slide.in {
    opacity: 0;
  }

  .leaving.out {
    opacity: 0;
  }

  .title {
    font-family: system-ui, -apple-system, "Segoe UI", Ubuntu, Cantarell,
      sans-serif;
    font-size: clamp(2.5rem, 8vmin, 9rem);
    font-weight: 700;
    margin: 0;
    line-height: 1.1;
    text-shadow: 0 2px 24px rgba(0, 0, 0, 0.45);
  }

  .body {
    font-family: system-ui, -apple-system, "Segoe UI", Ubuntu, Cantarell,
      sans-serif;
    font-size: clamp(1.25rem, 4.5vmin, 5rem);
    font-weight: 400;
    margin: 0;
    max-width: 80%;
    line-height: 1.4;
    white-space: pre-wrap;
    text-shadow: 0 2px 20px rgba(0, 0, 0, 0.4);
  }
</style>
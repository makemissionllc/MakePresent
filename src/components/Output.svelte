<script lang="ts">
  import { onMount } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { api, subscribeState } from "../lib/sync";
  import type { ClientState, Look, Slide } from "../lib/types";
  import SlideRender from "./SlideRender.svelte";

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

  // Resolve the Look assigned to this Output window (mapping lives in per-machine
  // settings, not hardcoded). Falls back to the look named "Main", then the
  // first look, when unmapped.
  const look = $derived.by<Look | null>(() => {
    const looks = appState?.looks ?? [];
    if (looks.length === 0) return null;
    const mapped = looks.find((l) => l.id === appState?.outputLookId);
    if (mapped) return mapped;
    return looks.find((l) => l.name === "Main") ?? looks[0]!;
  });

  // The on-deck slide comes straight from state (the backend decides who is
  // "likely next"). Its media is preloaded here — in the window that will
  // actually play it — so a cut to it starts instantly instead of decoding
  // on demand mid-service. Exactly ONE hidden element is kept, never a pile.
  const onDeck = $derived(appState?.onDeck ?? null);

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

<main class="stage">
  {#if shown}
    {#if look}
      <div class="frame" class:in={dim}>
        <SlideRender slide={shown} {look} />
      </div>
    {/if}
  {:else if !leaving}
    <div class="offline"></div>
  {/if}

  {#if leaving}
    {#if look}
      <div class="frame" class:out={out}>
        <SlideRender slide={leaving} {look} />
      </div>
    {/if}
  {/if}

  {#if onDeck && onDeck.background.type === "video"}
    <video
      class="preloader"
      src={convertFileSrc(onDeck.background.path)}
      preload="auto"
      muted
      tabindex="-1"
      aria-hidden="true"
    ></video>
  {:else if onDeck && onDeck.background.type === "image"}
    <img
      class="preloader"
      src={convertFileSrc(onDeck.background.path)}
      alt=""
      tabindex="-1"
      aria-hidden="true"
    />
  {/if}
</main>

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
    width: 100vw;
    height: 100vh;
    background: #000;
  }

  .frame,
  .offline {
    position: absolute;
    inset: 0;
  }

  .frame {
    opacity: 1;
    transition: opacity 400ms ease;
  }

  .preloader {
    position: absolute;
    bottom: 0;
    right: 0;
    width: 1px;
    height: 1px;
    opacity: 0.01;
    pointer-events: none;
    /* keep it reachable by the layout engine so WebKit actually fetches it */
  }

  .frame.in {
    opacity: 0;
  }

  .frame.out {
    opacity: 0;
  }
</style>

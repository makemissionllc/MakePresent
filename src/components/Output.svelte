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
  // Opacity of the incoming (shown) slide during crossfade.
  let inOpacity = $state(1);
  // Opacity of the outgoing (leaving) slide during crossfade.
  let outOpacity = $state(1);
  // True while a crossfade is active — drives GPU layer hints.
  let crossfading = $state(false);
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

  const showText = $derived(project?.showText ?? true);
  const showBackground = $derived(project?.showBackground ?? true);
  const overlay = $derived(appState?.overlay ?? null);

  // The on-deck slide comes straight from state (the backend decides who is
  // "likely next"). Its media is preloaded here — in the window that will
  // actually play it — so a cut to it starts instantly instead of decoding
  // on demand mid-service. Exactly ONE hidden element is kept, never a pile.
  // Camera streams open only on the live + brief fade-overlap leaving frames
  // (plus the Editor live preview) — the on-deck preloader stays file-only,
  // since warming a hidden camera stream would double device contention for
  // nothing visible. Unmount stops all tracks (CameraFeed).
  const onDeck = $derived(appState?.onDeck ?? null);

  // The Output is a dumb renderer: it only choreographs the fade. The backend
  // decides which slide is live; here we layer the old slide on top of the new
  // one and crossfade between them when the project's transition is "fade".
  //
  // GPU compositing: during the crossfade both .frame elements are promoted to
  // their own GPU layers via `will-change: transform, opacity` and a
  // `translate3d(0,0,0)` transform. This avoids full-window repaints on every
  // frame and keeps the crossfade buttery-smooth even with video backgrounds.
  // The hints are added at the start of the transition and removed on cleanup
  // so idle frames never waste GPU memory.
  $effect(() => {
    const next = live;
    const prev = shown;
    if (next?.id === prev?.id && (next === null) === (prev === null)) return;

    if (transition === "fade") {
      leaving = prev;
      outOpacity = 1;
      shown = next;
      inOpacity = 0;
      crossfading = true;
      // Force style recompute so both transitions start from a clean state.
      void document.body.offsetWidth;
      requestAnimationFrame(() => {
        outOpacity = 0;
        inOpacity = 1;
      });
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        leaving = null;
        outOpacity = 1;
        inOpacity = 1;
        crossfading = false;
      }, FADE_MS + 40);
    } else {
      leaving = null;
      outOpacity = 1;
      shown = next;
      inOpacity = 1;
      crossfading = false;
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
      leaving = null;
      inOpacity = 1;
      outOpacity = 1;
      crossfading = false;
    };
  });
</script>

<main class="stage">
  {#if shown}
    {#if look}
      <div
        class="frame"
        class:gpu={crossfading}
        style:opacity={inOpacity}
      >
        <SlideRender slide={shown} {look} {showText} {showBackground} enableCamera={true} />
      </div>
    {/if}
  {:else if !leaving}
    <div class="offline"></div>
  {/if}

  {#if leaving}
    {#if look}
      <div
        class="frame"
        class:gpu={crossfading}
        style:opacity={outOpacity}
      >
        <SlideRender slide={leaving} {look} {showText} {showBackground} enableCamera={true} />
      </div>
    {/if}
  {/if}

  {#if overlay?.visible}
    <div class="overlay-layer" style:z-index={2}>
      {#if overlay.background?.type === "image"}
        <img
          class="overlay-media"
          src={convertFileSrc(overlay.background.path)}
          alt=""
          draggable="false"
          onerror={(e) => {
            (e.currentTarget as HTMLImageElement).style.display = "none";
          }}
        />
      {:else if overlay.background?.type === "video"}
        <video
          class="overlay-media"
          src={convertFileSrc(overlay.background.path)}
          autoplay
          loop
          muted
          playsinline
          preload="auto"
        ></video>
      {/if}
      {#if overlay.text}
        <div class="overlay-text">{overlay.text}</div>
      {/if}
    </div>
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
    margin: 0;
    padding: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    background: #000;
    isolation: isolate;
    contain: layout style;
  }

  .frame,
  .offline {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .frame {
    opacity: 1;
    transition: opacity 400ms ease;
    /* was `contain: size layout style paint` — `size` freezes the box to the
       stale window size during the OS fullscreen swap-chain recreation, so the
       incoming/outgoing frames clip to the old size instead of blending. Keep
       layout/style/paint isolation for perf but allow the box to resize. */
    contain: layout style paint;
    /* Keep a cheap composited layer alive at all times so the fullscreen
       surface switch does not discard the layer mid-fade. The heavy
       `will-change: transform, opacity` promotion for the crossfade itself
       stays on `.gpu` below. */
    will-change: opacity;
    transform: translateZ(0);
    backface-visibility: hidden;
    isolation: isolate;
  }

  /* GPU compositing hints: applied only during a crossfade so idle frames
      never waste GPU memory. translate3d forces the browser to promote the
      element to its own composited layer; will-change tells the compositor
      to expect transform+opacity changes so it can prepare the layer up
      front instead of discovering the animation on the first frame. */
  .frame.gpu {
    will-change: transform, opacity;
    transform: translate3d(0, 0, 0);
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

  .overlay-layer {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: center;
    pointer-events: none;
    z-index: 2;
  }
  .overlay-media {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    width: 100%;
    height: 18vh;
    object-fit: cover;
    z-index: 0;
  }
  .overlay-text {
    position: relative;
    z-index: 1;
    background: rgba(0, 0, 0, 0.72);
    color: white;
    padding: 1.2vh 3vw;
    font-family: var(--font-body);
    font-size: clamp(1rem, 2.8vmin, 1.8rem);
    font-weight: 600;
    text-align: center;
    max-width: 90%;
    border-radius: 6px;
    margin-bottom: 3vh;
    backdrop-filter: blur(4px);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.5);
    white-space: pre-wrap;
  }
</style>

<script lang="ts">
  import { onMount } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { api, emitOutroDone, emitRenderAck, subscribeCountdown, subscribeExitOutro, subscribeState } from "../lib/sync";
  import type { ClientState, CountdownView, Look, Slide } from "../lib/types";
  import SlideRender from "./SlideRender.svelte";

  const FADE_MS = 400;
  /** Render-ack heartbeat cadence — slow enough to be noise-free, fast enough
      that the Editor's stale threshold (12s) trips promptly on a freeze. */
  const ACK_MS = 5000;

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
  let countdown = $state<CountdownView>({ active: false, running: false, remainingSeconds: 0, outputVisible: false, mode: null, targetTime: null });
  let timer: number | undefined;
  // Exit/outro animation (real Quit only): full-bleed video URL once the
  // backend emits `exit-outro`. Null = normal rendering. The backend's hard
  // cap guarantees shutdown, so this overlay can never stall the exit — and
  // the ack heartbeat below keeps firing while it plays, so the outro is
  // never misread as a frozen Output.
  let outroUrl = $state<string | null>(null);

  function formatCountdown(seconds: number): string {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}` : `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }

  function playOutro(path: string): void {
    try {
      outroUrl = convertFileSrc(path);
    } catch {
      // Unresolvable path: fail silent — tell the backend we're done.
      outroUrl = null;
      emitOutroDone();
    }
  }

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
  const aspectRatio = $derived(project?.aspectRatio ?? "16:9");
  const overlays = $derived.by(() => {
    const saved = appState?.overlays ?? [];
    if (saved.length > 0) return saved.filter((item) => item.visible);
    const legacy = appState?.overlay;
    return legacy?.visible ? [legacy] : [];
  });

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
    if (next?.id === prev?.id && (next === null) === (prev === null)) {
      // Edits to the live slide must reach Output (and its NDI mirror) without
      // replaying the transition or remounting its media.
      if (next !== prev) shown = next;
      return;
    }

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
    let unCountdown: () => void = () => {};
    let unOutro: () => void = () => {};
    let ackTimer: number | undefined;
    // One-way ack: confirm each applied state + a slow heartbeat so a
    // frozen-but-not-updating window is still detectable. Fire-and-forget —
    // never awaits anything, so a stalled backend can't hang this window.
    const ack = () => emitRenderAck("output", live?.id ?? null);
    void (async () => {
      un = await subscribeState((s) => {
        appState = s;
        ack();
      });
      unCountdown = await subscribeCountdown((value) => (countdown = value));
      unOutro = await subscribeExitOutro((path) => playOutro(path));
      try {
        appState = await api.getState();
        ack();
      } catch (e) {
        console.error("Failed to fetch initial appState", e);
      }
      try {
        countdown = await api.getCountdown();
      } catch (e) {
        console.error("Failed to fetch initial countdown", e);
      }
    })();
    ackTimer = window.setInterval(ack, ACK_MS);
    return () => {
      un();
      unCountdown();
      unOutro();
      window.clearTimeout(timer);
      if (ackTimer !== undefined) window.clearInterval(ackTimer);
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
        <SlideRender slide={shown} effectiveBackground={appState?.effectiveBackgrounds?.[shown.id] ?? shown.background} {look} {showText} {showBackground} {aspectRatio} enableCamera={true} />
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
        <SlideRender slide={leaving} effectiveBackground={appState?.effectiveBackgrounds?.[leaving.id] ?? leaving.background} {look} {showText} {showBackground} {aspectRatio} enableCamera={true} />
      </div>
    {/if}
  {/if}

  {#each overlays as overlay (overlay.id)}
    <div class="overlay-layer" class:logo={overlay.placement === "logo"} style:z-index={2}>
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
  {/each}

  {#if appState?.liveCreditLine}
    <div class="live-credit-line">{appState.liveCreditLine}</div>
  {/if}

  {#if countdown.active && countdown.outputVisible}
    <div class="countdown-overlay" aria-label={`Service countdown ${formatCountdown(countdown.remainingSeconds)}`}>
      <span>{countdown.mode === "clock" && countdown.targetTime ? `UNTIL ${countdown.targetTime}` : "COUNTDOWN"}</span>
      <strong>{formatCountdown(countdown.remainingSeconds)}</strong>
    </div>
  {/if}

  {#if onDeck}
    {@const onDeckBackground = appState?.effectiveBackgrounds?.[onDeck.id] ?? onDeck.background}
    {#if onDeckBackground.type === "video"}
      <video
        class="preloader"
        src={convertFileSrc(onDeckBackground.path)}
        preload="auto"
        muted
        tabindex="-1"
        aria-hidden="true"
      ></video>
    {:else if onDeckBackground.type === "image"}
      <img
        class="preloader"
        src={convertFileSrc(onDeckBackground.path)}
        alt=""
        tabindex="-1"
        aria-hidden="true"
      />
    {/if}
  {/if}

  {#if outroUrl}
    <!-- Exit/outro: covers the whole Output (above slide + overlay layers) so
         the congregation sees the video, not a cut to desktop. Muted for
         reliable autoplay; `ended`/`error` both release the backend to exit. -->
    <video
      class="outro"
      src={outroUrl}
      autoplay
      muted
      playsinline
      preload="auto"
      onended={() => emitOutroDone()}
      onerror={() => emitOutroDone()}
    ></video>
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

  .countdown-overlay {
    position: absolute;
    right: clamp(18px, 3vw, 56px);
    bottom: clamp(18px, 3vh, 48px);
    z-index: 3;
    display: grid;
    gap: 0.3em;
    min-width: 150px;
    padding: clamp(10px, 1.2vw, 22px) clamp(14px, 1.6vw, 28px);
    border: 1px solid rgba(255, 255, 255, 0.28);
    border-radius: 12px;
    background: rgba(15, 18, 20, 0.76);
    color: #fff;
    font-family: var(--font-display);
    font-variant-numeric: tabular-nums;
    text-align: center;
    pointer-events: none;
    backdrop-filter: blur(8px);
  }
  .countdown-overlay span { color: rgba(255, 255, 255, 0.78); font: 700 clamp(9px, 1vmin, 14px)/1.2 var(--font-body); letter-spacing: 0.14em; }
  .countdown-overlay strong { font-size: clamp(1.8rem, 4.5vmin, 4rem); line-height: 1; }

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
    container-type: size;
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

  .outro {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    background: #000;
    z-index: 50;
    pointer-events: none;
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
  .overlay-layer.logo {
    inset: auto 4vw 4vh auto;
    width: min(22vw, 320px);
    height: min(22vh, 220px);
    justify-content: flex-end;
    align-items: flex-end;
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
  .overlay-layer.logo .overlay-media {
    position: relative;
    inset: auto;
    width: 100%;
    height: 100%;
    object-fit: contain;
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
  .overlay-layer.logo .overlay-text {
    max-width: 100%;
    margin: 0;
    padding: 0.55em 0.8em;
    font-size: clamp(0.8rem, 1.4vmin, 1.2rem);
  }
  .live-credit-line {
    position: absolute;
    z-index: 4;
    right: 3vw;
    bottom: 1.4vh;
    left: 3vw;
    color: rgba(255, 255, 255, 0.88);
    font: 500 clamp(10px, 1.35vmin, 17px)/1.3 var(--font-body);
    text-align: center;
    text-shadow: 0 1px 5px #000, 0 0 10px #000;
    pointer-events: none;
  }
</style>

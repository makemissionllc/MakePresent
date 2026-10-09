<script lang="ts">
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { api, emitOutroDone, emitRenderAck, subscribeCountdown, subscribeExitOutro, subscribeState } from "../lib/sync";
  import type { ClientState, CountdownView, Look, Slide, Transition } from "../lib/types";
  import { fitText } from "../lib/fitText";
  import { prefersReducedMotion } from "../lib/motion";
  import SlideRender from "./SlideRender.svelte";
  import { stripChords } from "../lib/chords";

  let appState = $state<ClientState | null>(null);
  let currentTime = $state("--:--:--");
  let countdown = $state<CountdownView>({ active: false, running: false, remainingSeconds: 0, outputVisible: false, mode: null, targetTime: null });
  // Exit/outro animation (real Quit only): same contract as Output — full
  // bleed here covers the current/next/clock layout for the stage team.
  let outroUrl = $state<string | null>(null);

  function playOutro(path: string): void {
    try {
      outroUrl = convertFileSrc(path);
    } catch {
      outroUrl = null;
      emitOutroDone();
    }
  }

  /** Same render-ack heartbeat as Output (see Output.svelte) — confirms the
      Stage renderer is alive and applied state, so the Editor can warn on a
      silent freeze instead of only noticing at the screen. */
  const ACK_MS = 5000;
  const FADE_MS = 400;

  function formatCountdown(seconds: number): string {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}` : `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }

  const current = $derived(appState?.current ?? null);
  const next = $derived(appState?.next ?? null);
  // Stage follows the backend's live slide and transition setting. It only
  // choreographs the visual crossfade between the old and new frames.
  let shownCurrent = $state<Slide | null>(null);
  let leavingCurrent = $state<Slide | null>(null);
  let currentInOpacity = $state(1);
  let currentOutOpacity = $state(1);
  let currentTransition = $state<Transition>("cut");
  let currentTransitionStarted = $state(false);
  let currentCrossfading = $state(false);
  let currentFadeTimer: number | undefined;

  $effect(() => {
    const incoming = current;
    const previous = shownCurrent;
    if (incoming?.id === previous?.id && (incoming === null) === (previous === null)) {
      // Live slide edits update in place without restarting the transition.
      if (incoming !== previous) shownCurrent = incoming;
      return;
    }

    const selectedTransition = appState?.project?.transition ?? "cut";
    if (selectedTransition !== "cut" && !prefersReducedMotion()) {
      currentTransition = selectedTransition;
      currentTransitionStarted = false;
      leavingCurrent = previous;
      currentOutOpacity = 1;
      shownCurrent = incoming;
      currentInOpacity = selectedTransition === "fade" ? 0 : 1;
      currentCrossfading = true;
      void document.body.offsetWidth;
      requestAnimationFrame(() => {
        currentTransitionStarted = true;
        if (currentTransition === "fade") {
          currentOutOpacity = 0;
          currentInOpacity = 1;
        }
      });
      window.clearTimeout(currentFadeTimer);
      currentFadeTimer = window.setTimeout(() => {
        leavingCurrent = null;
        currentOutOpacity = 1;
        currentInOpacity = 1;
        currentCrossfading = false;
        currentTransitionStarted = false;
        currentTransition = "cut";
      }, FADE_MS + 40);
    } else {
      leavingCurrent = null;
      shownCurrent = incoming;
      currentOutOpacity = 1;
      currentInOpacity = 1;
      currentCrossfading = false;
      currentTransitionStarted = false;
      currentTransition = "cut";
      window.clearTimeout(currentFadeTimer);
    }
  });

  // Resolve the Look assigned to this Stage window. Falls back to the look
  // named "Stage", then the first look, when unmapped.
  const look = $derived.by<Look | null>(() => {
    const looks = appState?.looks ?? [];
    if (looks.length === 0) return null;
    const mapped = looks.find((l) => l.id === appState?.stageLookId);
    if (mapped) return mapped;
    return looks.find((l) => l.name === "Stage") ?? looks[0]!;
  });

  const showText = $derived(appState?.project?.showText ?? true);
  const showBackground = $derived(appState?.project?.showBackground ?? true);
  const aspectRatio = $derived(appState?.project?.aspectRatio ?? "16:9");
  const stageMessage = $derived(appState?.stageMessage ?? null);

  onMount(() => {
    let un: () => void = () => {};
    let unCountdown: () => void = () => {};
    let unOutro: () => void = () => {};
    let clock: number | undefined;
    let ackTimer: number | undefined;

    const tick = () => {
      currentTime = new Date().toLocaleTimeString([], { hour12: false });
    };
    const ack = () => emitRenderAck("stage", current?.id ?? null);

    void (async () => {
      un = await subscribeState((s) => {
        appState = s;
        ack();
      });
      unCountdown = await subscribeCountdown((timer) => (countdown = timer));
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
    tick();
    clock = window.setInterval(tick, 1000);
    ackTimer = window.setInterval(ack, ACK_MS);

    return () => {
      un();
      unCountdown();
      unOutro();
      window.clearTimeout(currentFadeTimer);
      if (clock !== undefined) window.clearInterval(clock);
      if (ackTimer !== undefined) window.clearInterval(ackTimer);
    };
  });
</script>

<div class="stage">
  {#if stageMessage}
    <div class="stage-banner" role="alert" aria-live="assertive">
      <span class="banner-text">{stageMessage}</span>
    </div>
  {/if}
  <section class="current">
    {#if shownCurrent && look}
      <div class="stage-frame" class:crossfading={currentCrossfading} class:push-in={currentTransition === "push" && !currentTransitionStarted} style:opacity={currentInOpacity} style:clip-path={currentTransition === "wipe" ? (currentTransitionStarted ? "inset(0)" : "inset(0 100% 0 0)") : undefined}>
        <SlideRender {look} slide={shownCurrent} effectiveBackground={appState?.effectiveBackgrounds?.[shownCurrent.id] ?? shownCurrent.background} {showText} {showBackground} {aspectRatio} isStage={true} presentSongTitle={true} />
      </div>
    {:else if shownCurrent}
      <p class="placeholder">{shownCurrent.body || shownCurrent.title}</p>
    {:else if !leavingCurrent}
      <p class="placeholder">No live slide</p>
    {/if}
    {#if leavingCurrent && look}
      <div class="stage-frame stage-frame-leaving" class:crossfading={currentCrossfading} class:push-out={currentTransition === "push" && currentTransitionStarted} style:opacity={currentOutOpacity}>
        <SlideRender {look} slide={leavingCurrent} effectiveBackground={appState?.effectiveBackgrounds?.[leavingCurrent.id] ?? leavingCurrent.background} {showText} {showBackground} {aspectRatio} isStage={true} presentSongTitle={true} />
      </div>
    {/if}
  </section>

  <aside class="side">
    <div class="next">
      <span class="next-label">NEXT</span>
      {#if next}
        {#key next.id}
          <div class="next-body-wrap" use:fitText transition:fade={{ duration: prefersReducedMotion() ? 0 : 180 }}>
            <p class="next-body" data-role="body">{stripChords(next.body) || stripChords(next.title)}</p>
          </div>
        {/key}
      {:else}
        <p class="placeholder">Nothing queued</p>
      {/if}
    </div>
    {#if countdown.active}
      <div class="service-countdown" aria-label={`Service countdown ${formatCountdown(countdown.remainingSeconds)}`}>
        <span>{countdown.mode === "clock" && countdown.targetTime ? `UNTIL ${countdown.targetTime}` : "COUNTDOWN"}</span>
        <strong>{formatCountdown(countdown.remainingSeconds)}</strong>
        {#if !countdown.running}<small>{countdown.remainingSeconds === 0 ? "Time reached" : "Paused"}</small>{/if}
      </div>
    {/if}
    <div class="clock">{currentTime}</div>
  </aside>

  {#if outroUrl}
    <!-- Exit/outro: full-bleed over the whole Stage window (current + next +
         clock) so the stage team sees the video, not a cut to desktop. -->
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

  .stage-banner {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 10;
    background: #e11d48;
    color: white;
    text-align: center;
    padding: 1.2vh 2vw;
    font-family: var(--font-display);
    font-size: clamp(1.4rem, 3.5vmin, 2.8rem);
    font-weight: 800;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    box-shadow: 0 4px 20px rgba(225, 29, 72, 0.6);
    animation: banner-pulse 1s ease-in-out infinite alternate;
    pointer-events: none;
  }
  @keyframes banner-pulse {
    from { background: #e11d48; box-shadow: 0 4px 20px rgba(225, 29, 72, 0.6); }
    to { background: #be123c; box-shadow: 0 6px 28px rgba(225, 29, 72, 0.85); }
  }
  .banner-text {
    display: inline-block;
    animation: text-flash 0.8s step-end infinite alternate;
  }
  @keyframes text-flash {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.92; }
  }

  .current {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    container-type: size;
  }

  .stage-frame {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 1;
    transition: opacity 400ms ease, transform 400ms cubic-bezier(0.22, 1, 0.36, 1), clip-path 400ms cubic-bezier(0.22, 1, 0.36, 1);
    contain: layout style paint;
    will-change: opacity;
    transform: translateZ(0);
    backface-visibility: hidden;
    isolation: isolate;
  }
  .stage-frame-leaving { z-index: 1; }
  .stage-frame.crossfading { will-change: transform, opacity; }
  .stage-frame.push-in { transform: translate3d(100%, 0, 0); }
  .stage-frame.push-out { transform: translate3d(-100%, 0, 0); }

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
    text-transform: uppercase;
    color: var(--text-dim);
  }

  .next-body-wrap {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    align-items: center;
  }

  .next-body {
    font-family: var(--font-body);
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
    font-family: var(--font-display);
    font-size: clamp(2rem, 6vmin, 5rem);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    text-align: left;
    color: #ffffff;
  }

  .service-countdown {
    display: grid;
    gap: 0.4vh;
    padding: 1.2vh 1.2vw;
    border: 1px solid rgba(129, 170, 149, 0.35);
    border-radius: 10px;
    background: rgba(129, 170, 149, 0.1);
    color: #e8edeb;
    font-variant-numeric: tabular-nums;
  }
  .service-countdown span { color: #9aa6a2; font-size: clamp(9px, 1.1vmin, 13px); font-weight: 700; letter-spacing: 0.14em; }
  .service-countdown strong { font-family: var(--font-display); font-size: clamp(1.6rem, 4.2vmin, 3.2rem); line-height: 1.1; }
  .service-countdown small { color: #9aa6a2; font-size: clamp(10px, 1.2vmin, 14px); }

  .placeholder {
    color: #555a68;
    font-size: clamp(1rem, 2vmin, 1.8rem);
    margin: 0;
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
</style>

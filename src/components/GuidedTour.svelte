<script lang="ts">
  import { surface } from "../lib/motion";
  import type { TourStep } from "../lib/tour";

  interface Props {
    step: number;
    steps: TourStep[];
    onNext: () => void;
    onBack: () => void;
    onDone: () => void;
    onSkip: () => void;
    onGoTo: (step: number) => void;
    onAction: () => void;
    busy?: boolean;
  }

  let { step, steps, onNext, onBack, onDone, onSkip, onGoTo, onAction, busy = false }: Props = $props();

  const current = $derived(steps[Math.min(step, steps.length - 1)]!);
  const isFirst = $derived(step <= 0);
  const isLast = $derived(step >= steps.length - 1);
</script>

<!-- Non-blocking: the wrapper ignores pointer events so a volunteer under
     time pressure can click straight through everywhere except the card. -->
<div class="tour-layer" class:source-step={["songs", "scripture", "reader", "bookmarks", "media"].includes(current.action ?? "")} role="dialog" aria-label="MakrStudio guided tour">
  <div class="tour-card" in:surface={{ duration: 240, distance: 10 }}>
    <div class="tour-top">
      <span class="tour-eyebrow">{current.eyebrow} · {step + 1} of {steps.length}</span>
      <button class="tour-x" title="End tour" aria-label="End tour" onclick={onSkip}>×</button>
    </div>
    <div class="tour-progress" role="progressbar" aria-label="Tour progress" aria-valuemin="1" aria-valuemax={steps.length} aria-valuenow={step + 1}>
      <span style={`transform:scaleX(${(step + 1) / steps.length})`}></span>
    </div>
    <label class="topic-picker">Jump to a topic<select value={step} onchange={(e) => onGoTo(Number(e.currentTarget.value))}>{#each steps as topic, i}<option value={i}>{i + 1}. {topic.eyebrow}</option>{/each}</select></label>
    {#key step}
      <div class="tour-copy" in:surface={{ duration: 190, distance: 5 }}>
        <strong class="tour-title">{current.title}</strong>
        <p class="tour-body" aria-live="polite">{current.body}</p>
      </div>
    {/key}
    {#if current.actionLabel}<button class="try-action" disabled={busy} aria-busy={busy} onclick={onAction}>{busy ? "Opening…" : current.actionLabel}<span aria-hidden="true"> ↗</span></button>{/if}
    <div class="tour-dots" aria-hidden="true">
      {#each steps as _, i}
        <span class="tour-dot" class:active={i === step}></span>
      {/each}
    </div>
    <div class="tour-actions">
      {#if !isFirst}
        <button class="ghost" onclick={onBack}>Back</button>
      {/if}
      <span class="spacer"></span>
      <button class="ghost" onclick={onSkip}>Skip</button>
      {#if isLast}
        <button class="primary" onclick={onDone}>Finish tour</button>
      {:else}
        <button class="primary" onclick={onNext}>Next</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .tour-layer {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 18px;
    display: flex;
    justify-content: center;
    z-index: 60;
    pointer-events: none;
  }
  .tour-layer.source-step { bottom: auto; top: 72px; }
  .tour-card {
    pointer-events: auto;
    width: min(400px, 92vw);
    background: var(--panel);
    border: 1px solid var(--accent, #4f8cff);
    border-radius: 12px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: calc(100dvh - 100px);
    overflow-y: auto;
  }
  .tour-progress { height: 3px; overflow: hidden; border-radius: 99px; background: var(--panel-2); }
  .tour-progress span { display: block; width: 100%; height: 100%; background: var(--accent, #4f8cff); transform-origin: left center; transition: transform var(--motion-normal) var(--ease-emphasized); }
  .tour-copy { display: grid; gap: 8px; }
  .topic-picker { display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: 10px; color: var(--text-dim); font-size: 10px; }
  .topic-picker select { min-width: 0; padding: 5px; border: 1px solid var(--border); background: var(--panel-2); color: var(--text); font-size: 11px; }
  .try-action { padding: 9px 12px; border: 1px solid var(--accent); border-radius: 7px; background: color-mix(in srgb, var(--accent) 12%, var(--panel)); color: var(--text); font-size: 12px; font-weight: 700; text-align: left; }
  .try-action span { float: right; color: var(--accent); }
  .tour-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tour-eyebrow {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
    flex: 1;
  }
  .tour-x {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: transparent;
    color: var(--text-dim);
    width: 26px;
    height: 26px;
    line-height: 1;
    cursor: pointer;
  }
  .tour-title {
    font-size: 14px;
  }
  .tour-body {
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-dim);
    margin: 0;
  }
  .tour-dots {
    display: flex;
    gap: 6px;
  }
  .tour-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--border);
    transition: background var(--motion-fast) var(--ease-out), transform var(--motion-normal) var(--ease-out);
  }
  .tour-dot.active {
    background: var(--accent, #4f8cff);
    transform: scale(1.25);
  }
  .tour-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tour-actions .spacer {
    flex: 1;
  }
  .tour-actions button {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel-2);
    color: var(--text);
    padding: 6px 12px;
    cursor: pointer;
  }
  .tour-actions button.ghost {
    background: transparent;
  }
  .tour-actions button.primary {
    background: var(--accent, #4f8cff);
    border-color: var(--accent, #4f8cff);
    color: #fff;
  }
  @media (prefers-reduced-motion: reduce) {
    .tour-progress span { transition: none; }
    .tour-dot { transition: none; }
  }
</style>

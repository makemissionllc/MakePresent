<script lang="ts">
  export interface TourStep {
    eyebrow: string;
    title: string;
    body: string;
  }

  interface Props {
    step: number;
    steps: TourStep[];
    onNext: () => void;
    onBack: () => void;
    onDone: () => void;
    onSkip: () => void;
  }

  let { step, steps, onNext, onBack, onDone, onSkip }: Props = $props();

  const current = $derived(steps[Math.min(step, steps.length - 1)]!);
  const isFirst = $derived(step <= 0);
  const isLast = $derived(step >= steps.length - 1);
</script>

<!-- Non-blocking: the wrapper ignores pointer events so a volunteer under
     time pressure can click straight through everywhere except the card. -->
<div class="tour-layer" role="dialog" aria-label="MakrStudio guided tour">
  <div class="tour-card">
    <div class="tour-top">
      <span class="tour-eyebrow">{current.eyebrow} · {step + 1} of {steps.length}</span>
      <button class="tour-x" title="End tour" aria-label="End tour" onclick={onSkip}>×</button>
    </div>
    <strong class="tour-title">{current.title}</strong>
    <p class="tour-body">{current.body}</p>
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
        <button class="primary" onclick={onDone}>Start</button>
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
  }
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
  }
  .tour-dot.active {
    background: var(--accent, #4f8cff);
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
</style>

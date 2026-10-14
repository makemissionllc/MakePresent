<script lang="ts">
  import { surface } from "../lib/motion";

  export interface ServiceCheckItem {
    title: string;
    detail: string;
    ready: boolean;
    action: "playlist" | "output";
    actionLabel: string;
  }

  interface Props {
    checks: ServiceCheckItem[];
    stageVisible: boolean;
    onClose: () => void;
    onAction: (action: "playlist" | "output" | "stage") => void;
  }

  let { checks, stageVisible, onClose, onAction }: Props = $props();
  const readyCount = $derived(checks.filter((item) => item.ready).length);
  const allReady = $derived(readyCount === checks.length);
</script>

<div class="service-check-layer" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) onClose(); }} onkeydown={(event) => event.stopPropagation()}>
  <div class="service-check" role="dialog" aria-modal="true" aria-labelledby="service-check-title" tabindex="-1" in:surface={{ duration: 220, distance: 10 }}>
    <header>
      <div>
        <span class="eyebrow">BEFORE YOU PRESENT</span>
        <h2 id="service-check-title">Service check</h2>
      </div>
      <button class="close" aria-label="Close service check" onclick={onClose}>×</button>
    </header>

    <div class="progress-copy" aria-live="polite">
      <strong>{allReady ? "Ready when you are" : `${readyCount} of ${checks.length} essentials ready`}</strong>
      <span>{allReady ? "Your service is set up. Review the live preview before presenting." : "A quick check keeps setup clear and predictable."}</span>
    </div>
    <div class="progress-track" role="progressbar" aria-label="Service readiness" aria-valuemin="0" aria-valuemax={checks.length} aria-valuenow={readyCount}>
      <span style={`transform:scaleX(${checks.length ? readyCount / checks.length : 1})`}></span>
    </div>

    <div class="check-list">
      {#each checks as item, index (item.title)}
        <article class="check-item" class:ready={item.ready}>
          <span class="check-mark" aria-hidden="true">{item.ready ? "✓" : index + 1}</span>
          <div class="check-copy">
            <strong>{item.title}</strong>
            <p>{item.detail}</p>
          </div>
          <button class="check-action" onclick={() => onAction(item.action)}>{item.actionLabel}<span aria-hidden="true"> ↗</span></button>
        </article>
      {/each}
    </div>

    <div class="stage-note">
      <span class="stage-mark" aria-hidden="true">{stageVisible ? "✓" : "＋"}</span>
      <div><strong>Stage display <span>{stageVisible ? "Connected" : "Optional"}</span></strong><p>{stageVisible ? "Your presenter screen is open." : "Add a separate presenter screen for notes and upcoming slides."}</p></div>
      <button class="text-action" onclick={() => onAction("stage")}>Review Stage ↗</button>
    </div>

    <footer>
      <span>Checking readiness never puts a slide live.</span>
      <button class="done" onclick={onClose}>Done</button>
    </footer>
  </div>
</div>

<style>
  .service-check-layer { position: fixed; inset: 0; z-index: 80; display: grid; place-items: center; padding: 20px; background: #080d10aa; backdrop-filter: blur(4px); animation: veil-in 160ms ease-out both; }
  .service-check { width: min(560px, 100%); max-height: min(760px, calc(100dvh - 40px)); overflow-y: auto; padding: 22px; color: var(--text); background: var(--panel); border: 1px solid var(--border); border-radius: 16px; box-shadow: 0 24px 80px #0008; }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  .eyebrow { color: var(--accent); font-size: 9px; font-weight: 800; letter-spacing: .13em; }
  h2 { margin: 5px 0 0; font-family: var(--font-display); font-size: 22px; letter-spacing: -.03em; }
  button { font: inherit; cursor: pointer; }
  .close { width: 30px; height: 30px; border: 1px solid var(--border); border-radius: 8px; color: var(--text-dim); background: transparent; font-size: 19px; line-height: 1; }
  .progress-copy { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 9px; margin-top: 22px; }
  .progress-copy strong { font-size: 13px; }
  .progress-copy span { color: var(--text-dim); font-size: 11px; }
  .progress-track { height: 5px; margin: 11px 0 16px; overflow: hidden; border-radius: 99px; background: var(--panel-2); }
  .progress-track span { display: block; width: 100%; height: 100%; transform-origin: left; background: var(--accent); transition: transform 260ms cubic-bezier(.2,.8,.2,1); }
  .check-list { display: grid; gap: 9px; }
  .check-item { display: grid; grid-template-columns: 28px minmax(0,1fr) auto; align-items: center; gap: 11px; padding: 13px; border: 1px solid var(--border); border-radius: 11px; background: var(--bg); }
  .check-mark { display: grid; place-items: center; width: 27px; height: 27px; border: 1px solid var(--border); border-radius: 50%; color: var(--text-dim); font-size: 11px; font-weight: 700; }
  .check-item.ready .check-mark { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 45%, var(--border)); background: color-mix(in srgb, var(--accent) 12%, var(--panel)); }
  .check-copy { min-width: 0; }
  .check-copy strong, .stage-note strong { font-size: 12px; }
  .check-copy p, .stage-note p { margin: 4px 0 0; color: var(--text-dim); font-size: 10px; line-height: 1.5; }
  .check-action, .done { padding: 8px 10px; border: 1px solid var(--border); border-radius: 7px; color: var(--text); background: var(--panel-2); font-size: 10px; font-weight: 700; white-space: nowrap; }
  .check-action span { color: var(--accent); }
  .stage-note { display: grid; grid-template-columns: 28px minmax(0,1fr) auto; align-items: center; gap: 11px; margin-top: 12px; padding: 12px 4px; }
  .stage-mark { display: grid; place-items: center; width: 25px; height: 25px; border: 1px solid var(--border); border-radius: 8px; color: var(--text-dim); font-size: 12px; }
  .stage-note strong span { margin-left: 5px; color: var(--text-dim); font-size: 9px; font-weight: 500; }
  .text-action { border: 0; color: var(--accent); background: transparent; font-size: 10px; font-weight: 700; white-space: nowrap; }
  footer { display: flex; justify-content: space-between; align-items: center; gap: 12px; margin-top: 9px; padding-top: 15px; border-top: 1px solid var(--border); }
  footer > span { color: var(--text-dim); font-size: 10px; }
  .done { padding: 8px 16px; border-color: var(--accent); color: var(--bg); background: var(--accent); }
  @keyframes veil-in { from { opacity: 0; } to { opacity: 1; } }
  @media (max-width: 480px) { .service-check { padding: 17px; border-radius: 13px; } .check-item { grid-template-columns: 28px minmax(0,1fr); } .check-action { grid-column: 2; justify-self: start; } .stage-note { grid-template-columns: 28px minmax(0,1fr); } .text-action { grid-column: 2; justify-self: start; padding-left: 0; } footer > span { max-width: 190px; } }
  @media (prefers-reduced-motion: reduce) { .service-check-layer { animation: none; } .progress-track span { transition: none; } }
</style>

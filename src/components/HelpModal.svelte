<script lang="ts">
  interface Props {
    open: boolean;
    onClose: () => void;
    onReplayTour: () => void;
  }

  let { open, onClose, onReplayTour }: Props = $props();

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    }
  }
</script>

{#if open}
  <div
    class="backdrop"
    role="presentation"
    onclick={onClose}
    onkeydown={(e) => e.stopPropagation()}
  >
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-label="Help and keyboard shortcuts"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="help-head">
        <strong>Help</strong>
        <button class="x" title="Close" aria-label="Close help" onclick={onClose}>×</button>
      </div>
      <p class="muted">
        New here? Take the 4-step tour — playlist, Output, songs &amp; Scripture.
      </p>
      <button
        class="replay"
        onclick={() => {
          onReplayTour();
          onClose();
        }}
      >
        Replay guided tour
      </button>
      <div class="section-title">Keyboard shortcuts</div>
      <ul class="shortcuts">
        <li><span class="keys"><kbd>←</kbd> <kbd>→</kbd></span> Previous / next slide (when not typing)</li>
        <li><span class="keys"><kbd>Ctrl</kbd>+<kbd>K</kbd></span> Search songs, Bibles &amp; media <span class="muted">(⌘K on Mac)</span></li>
        <li><span class="keys"><kbd>Esc</kbd></span> Close search, tour, or dialog</li>
        <li><span class="keys"><kbd>↑</kbd> <kbd>↓</kbd> <kbd>Enter</kbd></span> Pick a Scripture suggestion</li>
        <li><span class="keys"><kbd>Enter</kbd> / <kbd>Esc</kbd></span> Confirm / cancel a dialog</li>
      </ul>
      <p class="muted">
        Prefer dragging? Songs, verses, and images can all be dragged straight
        onto the playlist — clicking works too.
      </p>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: grid;
    place-items: center;
    z-index: 70;
  }
  .dialog {
    width: min(420px, 92vw);
    max-height: 84vh;
    overflow-y: auto;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .help-head {
    display: flex;
    align-items: center;
    font-size: 14px;
  }
  .help-head strong {
    flex: 1;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 12px;
  }
  .x {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: transparent;
    color: var(--text-dim);
    width: 26px;
    height: 26px;
    line-height: 1;
    cursor: pointer;
  }
  .muted {
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.5;
    margin: 0;
  }
  .replay {
    align-self: flex-start;
    border: 1px solid var(--accent, #4f8cff);
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    padding: 6px 12px;
    cursor: pointer;
  }
  .section-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
    margin-top: 4px;
  }
  .shortcuts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 13px;
  }
  .keys {
    display: inline-flex;
    gap: 2px;
    margin-right: 6px;
  }
  kbd {
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 6px;
    background: var(--panel-2);
    white-space: nowrap;
  }
</style>

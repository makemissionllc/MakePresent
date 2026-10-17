<script lang="ts">
  import { surface, veil } from "../lib/motion";
  interface Props {
    open: boolean;
    title?: string;
    label?: string;
    placeholder?: string;
    initialValue?: string;
    message?: string | null;
    busy?: boolean;
    busyLabel?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    onConfirm: (value: string) => void;
    onCancel: () => void;
  }

  let {
    open,
    title = "Enter value",
    label = "Name",
    placeholder = "",
    initialValue = "",
    message = null,
    busy = false,
    busyLabel = "Working…",
    confirmLabel = "OK",
    cancelLabel = "Cancel",
    onConfirm,
    onCancel,
  }: Props = $props();

  let value = $state("");
  let visibleMessage = $state<string | null>(null);
  let dialogEl = $state<HTMLDivElement | null>(null);
  let inputEl = $state<HTMLInputElement | null>(null);

  // Sync when opened with new initialValue; focus input
  $effect(() => {
    if (open) visibleMessage = message;
  });

  $effect(() => {
    if (open) {
      value = initialValue;
      // Focus after render
      const frame = requestAnimationFrame(() => {
        inputEl?.focus();
        inputEl?.select();
      });
      return () => cancelAnimationFrame(frame);
    }
  });

  // Keep focus inside while the request is busy; move focus back to the field
  // when an error returns control to the user.
  $effect(() => {
    if (!open) return;
    const isBusy = busy;
    const frame = requestAnimationFrame(() => {
      if (isBusy) dialogEl?.focus();
      else inputEl?.focus();
    });
    return () => cancelAnimationFrame(frame);
  });

  function handleConfirm(): void {
    if (busy) return;
    const trimmed = value.trim();
    // Allow empty? For song title we require non-empty, but let caller validate.
    // If empty and label is song title, we still close and let caller decide to ignore.
    onConfirm(value);
  }

  function handleCancel(): void {
    if (busy) return;
    onCancel();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      e.preventDefault();
      handleConfirm();
    } else if (e.key === "Escape") {
      e.preventDefault();
      handleCancel();
    }
  }

  function onDialogKeydown(e: KeyboardEvent): void {
    e.stopPropagation();
    if (e.key === "Escape" && !e.defaultPrevented && !busy) {
      e.preventDefault();
      handleCancel();
      return;
    }
    if (e.key !== "Tab") return;

    const focusable = Array.from(dialogEl?.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex]:not([tabindex="-1"])',
    ) ?? []).filter((element) => element.getClientRects().length > 0);
    if (focusable.length === 0) {
      e.preventDefault();
      dialogEl?.focus();
      return;
    }
    const first = focusable[0]!;
    const last = focusable[focusable.length - 1]!;
    const active = document.activeElement;
    if (e.shiftKey && (active === first || !dialogEl?.contains(active))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (active === last || !dialogEl?.contains(active))) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

{#if open}
  <div class="overlay" role="presentation" transition:veil={{ duration: 170 }}>
    <button class="backdrop" aria-label="Close dialog" tabindex="-1" disabled={busy} onclick={handleCancel}></button>
    <div
      bind:this={dialogEl}
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-busy={busy}
      aria-label={title}
      tabindex="-1"
      in:surface={{ duration: 250, distance: 12 }}
      onclick={(e) => e.stopPropagation()}
      onkeydown={onDialogKeydown}
    >
      <header class="dialog-head">
        <h2>{title}</h2>
        <button class="close" title="Close" aria-label="Close" disabled={busy} onclick={handleCancel}>&times;</button>
      </header>
      <div class="content">
        {#if visibleMessage}<p class="dialog-message" role="alert">{visibleMessage}</p>{/if}
        <label>
          {label}
          <input
            bind:this={inputEl}
            type="text"
            value={value}
            disabled={busy}
            placeholder={placeholder}
            oninput={(e) => { value = (e.target as HTMLInputElement).value; visibleMessage = null; }}
            onkeydown={onKeydown}
          />
        </label>
        <div class="actions">
          <button class="ghost" disabled={busy} onclick={handleCancel}>{cancelLabel}</button>
          <button class="primary" disabled={busy} aria-live="polite" onclick={handleConfirm}>
            {#if busy}<span class="busy-spinner" aria-hidden="true"></span>{/if}
            {busy ? busyLabel : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    border: none;
    border-radius: 0;
    cursor: pointer;
  }

  .dialog {
    position: relative;
    width: min(420px, 92vw);
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 18px 60px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.04);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .dialog-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
    background: var(--panel-2);
  }

  .dialog-head h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--text);
  }

  .close {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 6px;
    width: 28px;
    height: 28px;
    line-height: 1;
    color: var(--text-dim);
  }

  .close:hover {
    background: var(--panel);
    color: var(--text);
  }

  .content {
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .dialog-message {
    margin: -4px 0 0;
    padding: 9px 10px;
    border: 1px solid color-mix(in srgb, var(--danger-text, #f87171) 35%, var(--border));
    border-radius: 7px;
    color: var(--danger-text, #f87171);
    background: var(--danger-bg, color-mix(in srgb, #f87171 8%, var(--panel)));
    font-size: 11px;
    line-height: 1.45;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    color: var(--text-dim);
  }

  input[type="text"] {
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px 12px;
    color: var(--text);
    width: 100%;
    font-size: 14px;
    transition:
      border-color var(--motion-fast, 150ms) var(--ease-standard, ease),
      box-shadow var(--motion-fast, 150ms) var(--ease-standard, ease);
  }

  input[type="text"]:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px rgba(79, 140, 255, 0.15);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 4px;
  }

  button {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel-2);
    color: var(--text);
    padding: 7px 14px;
    font-size: 13px;
    font-weight: 500;
  }

  button.ghost {
    background: transparent;
  }

  button.ghost:hover {
    background: var(--panel-2);
    border-color: var(--brand-slate-400, #94a3b8);
  }

  button:disabled {
    cursor: wait;
    opacity: 0.72;
  }

  .busy-spinner {
    display: inline-block;
    width: 11px;
    height: 11px;
    margin-right: 6px;
    vertical-align: -1px;
    border: 2px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: modal-spin 700ms linear infinite;
  }

  @keyframes modal-spin { to { transform: rotate(360deg); } }

  @media (prefers-reduced-motion: reduce) {
    .busy-spinner { animation: none; border-right-color: currentColor; }
  }

  button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
    box-shadow: 0 2px 8px rgba(79, 140, 255, 0.25);
  }

  button.primary:hover {
    background: #3a6fd6;
    border-color: #3a6fd6;
    transform: translateY(-1px);
  }

  button.primary:active {
    transform: translateY(0);
  }
</style>

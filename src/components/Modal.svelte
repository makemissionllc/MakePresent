<script lang="ts">
  interface Props {
    open: boolean;
    title?: string;
    label?: string;
    placeholder?: string;
    initialValue?: string;
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
    confirmLabel = "OK",
    cancelLabel = "Cancel",
    onConfirm,
    onCancel,
  }: Props = $props();

  let value = $state("");
  let inputEl = $state<HTMLInputElement | null>(null);

  // Sync when opened with new initialValue; focus input
  $effect(() => {
    if (open) {
      value = initialValue;
      // Focus after render
      requestAnimationFrame(() => {
        inputEl?.focus();
        inputEl?.select();
      });
    }
  });

  function handleConfirm(): void {
    const trimmed = value.trim();
    // Allow empty? For song title we require non-empty, but let caller validate.
    // If empty and label is song title, we still close and let caller decide to ignore.
    onConfirm(value);
  }

  function handleCancel(): void {
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
</script>

{#if open}
  <div class="overlay" role="presentation">
    <button class="backdrop" aria-label="Close dialog" tabindex="-1" onclick={handleCancel}></button>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <header class="dialog-head">
        <h2>{title}</h2>
        <button class="close" title="Close" aria-label="Close" onclick={handleCancel}>&times;</button>
      </header>
      <div class="content">
        <label>
          {label}
          <input
            bind:this={inputEl}
            type="text"
            value={value}
            placeholder={placeholder}
            oninput={(e) => (value = (e.target as HTMLInputElement).value)}
            onkeydown={onKeydown}
          />
        </label>
        <div class="actions">
          <button class="ghost" onclick={handleCancel}>{cancelLabel}</button>
          <button class="primary" onclick={handleConfirm}>{confirmLabel}</button>
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

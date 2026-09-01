<script lang="ts">
  import { parseSong } from "../lib/songParser";

  interface Props {
    open: boolean;
    initialTitle?: string;
    initialBody?: string;
    onConfirm: (title: string, slides: { title: string; body: string }[], raw: string) => void;
    onCancel: () => void;
    onBack?: () => void;
  }

  let { open, initialTitle = "", initialBody = "", onConfirm, onCancel, onBack }: Props = $props();

  const TAGS = [
    "Verse",
    "Verse 1",
    "Verse 2",
    "Chorus",
    "Chorus 1",
    "Chorus 2",
    "Pre-Chorus",
    "Bridge",
    "Tag",
    "Outro",
    "Intro",
  ];

  let songTitle = $state("");
  let rawText = $state("");
  let textareaEl = $state<HTMLTextAreaElement | null>(null);
  let titleInputEl = $state<HTMLInputElement | null>(null);

  // Sync when opened
  $effect(() => {
    if (open) {
      songTitle = initialTitle;
      rawText = initialBody;
      // focus after render
      requestAnimationFrame(() => {
        if (rawText) textareaEl?.focus();
        else titleInputEl?.focus();
      });
    }
  });

  const parsed = $derived(parseSong(rawText, songTitle));
  const previewSlides = $derived(parsed.slides);
  const effectiveTitle = $derived(parsed.metadata.title || songTitle || "Untitled");

  function insertTag(tag: string): void {
    const header = `### ${tag}`;
    const el = textareaEl;
    if (!el) {
      rawText += (rawText ? "\n\n" : "") + header + "\n";
      return;
    }
    const start = el.selectionStart ?? rawText.length;
    const end = el.selectionEnd ?? rawText.length;
    const before = rawText.slice(0, start);
    const after = rawText.slice(end);
    const needsNewlineBefore = before && !before.endsWith("\n");
    const needsNewlineAfter = after && !after.startsWith("\n");
    const insertion = (needsNewlineBefore ? "\n\n" : rawText ? "\n\n" : "") + header + "\n" + (needsNewlineAfter ? "" : "");
    rawText = before + insertion + after;
    // restore cursor after insertion
    requestAnimationFrame(() => {
      el.focus();
      const pos = before.length + insertion.length;
      el.setSelectionRange(pos, pos);
    });
  }

  function handleConfirm(): void {
    const title = effectiveTitle.trim() || "Untitled";
    if (previewSlides.length === 0) return;
    onConfirm(title, previewSlides.map((s) => ({ title: s.title, body: s.body })), rawText);
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    }
  }
</script>

{#if open}
  <div class="overlay" role="presentation">
    <button class="backdrop" aria-label="Close dialog" tabindex="-1" onclick={onCancel}></button>
    <div
      class="dialog wide"
      role="dialog"
      aria-modal="true"
      aria-label="Song Editor"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <header class="dialog-head">
        <h2>Song Editor</h2>
        <button class="close" title="Close" aria-label="Close" onclick={onCancel}>&times;</button>
      </header>

      <div class="body">
        <div class="editor-pane">
          <label class="field-label">
            Song Title
            <input
              bind:this={titleInputEl}
              type="text"
              class="title-input"
              placeholder="e.g. Amazing Grace"
              value={songTitle}
              oninput={(e) => (songTitle = (e.target as HTMLInputElement).value)}
              onkeydown={onKeydown}
            />
          </label>
          {#if parsed.metadata.title && parsed.metadata.title !== songTitle}
            <span class="meta-hint">Detected title: <strong>{parsed.metadata.title}</strong></span>
          {/if}
          {#if parsed.metadata.style}
            <span class="meta-hint">Style: {parsed.metadata.style}</span>
          {/if}

          <div class="chips">
            {#each TAGS as tag}
              <button class="chip" onclick={() => insertTag(tag)} title="Insert {tag}">{tag}</button>
            {/each}
          </div>

          <textarea
            bind:this={textareaEl}
            class="lyrics"
            placeholder={`Paste full lyrics here — e.g.\n\n### Verse 1\nAmazing grace how sweet the sound\n...\n\n### Chorus\nMy chains are gone...\n\nSupports Markdown headers (### Verse 1), [Bridge], Verse 1: — auto-splits into slides.`}
            value={rawText}
            oninput={(e) => (rawText = (e.target as HTMLTextAreaElement).value)}
            onkeydown={onKeydown}
            rows={14}
          ></textarea>
          <span class="hint">{rawText.length} characters · {previewSlides.length} slide{previewSlides.length === 1 ? "" : "s"}</span>
        </div>

        <div class="preview-pane">
          <div class="preview-head">
            <span class="preview-title">Live Preview</span>
            <span class="preview-count">{previewSlides.length} slide{previewSlides.length === 1 ? "" : "s"}</span>
          </div>
          {#if previewSlides.length === 0}
            <p class="empty">No slides yet — add a section tag like <code>### Verse 1</code> or type lyrics to generate slides.</p>
          {:else}
            <ul class="preview-list">
              {#each previewSlides as slide, i}
                <li class="preview-card">
                  <span class="card-num">{i + 1}</span>
                  <div class="card-body">
                    <span class="card-title">{slide.title}</span>
                    <span class="card-text">{slide.body.split("\n").slice(0,3).join(" / ")}{slide.body.split("\n").length > 3 ? " …" : ""}</span>
                  </div>
                </li>
              {/each}
            </ul>
          {/if}
          {#if parsed.metadata.author}
            <span class="meta-hint">Author: {parsed.metadata.author}</span>
          {/if}
        </div>
      </div>

      <div class="actions">
        {#if onBack}
          <button class="ghost" onclick={onBack}>Back</button>
        {:else}
          <button class="ghost" onclick={onCancel}>Cancel</button>
        {/if}
        <span class="spacer"></span>
        <button class="ghost" onclick={onCancel}>Cancel</button>
        <button class="primary" onclick={handleConfirm} disabled={previewSlides.length === 0}>
          Add Song · {previewSlides.length} slide{previewSlides.length === 1 ? "" : "s"}
        </button>
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
    width: min(920px, 96vw);
    max-height: 88vh;
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
    flex-shrink: 0;
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
  .close:hover { background: var(--panel); color: var(--text); }

  .body {
    display: grid;
    grid-template-columns: 1fr 320px;
    gap: 0;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }
  @media (max-width: 760px) {
    .body { grid-template-columns: 1fr; }
    .preview-pane { border-left: none; border-top: 1px solid var(--border); max-height: 28vh; }
  }
  .editor-pane {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow: auto;
    min-height: 0;
  }
  .field-label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .title-input {
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px 12px;
    color: var(--text);
    font-size: 14px;
    width: 100%;
  }
  .title-input:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 3px rgba(79,140,255,0.15); }
  .meta-hint { font-size: 11px; color: var(--text-dim); }
  .meta-hint strong { color: var(--text); }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    padding: 5px 9px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--panel-2);
    color: var(--text-dim);
    cursor: pointer;
  }
  .chip:hover { background: var(--panel); color: var(--text); border-color: var(--accent); }
  .lyrics {
    width: 100%;
    min-height: 220px;
    flex: 1;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    color: var(--text);
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 13px;
    line-height: 1.5;
    resize: vertical;
  }
  .lyrics:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 3px rgba(79,140,255,0.15); }
  .hint { font-size: 11px; color: var(--text-dim); }
  .preview-pane {
    background: var(--panel-2);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  .preview-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-dim);
  }
  .preview-count { color: var(--accent); }
  .empty {
    padding: 18px;
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.5;
  }
  .empty code { background: var(--panel); padding: 1px 4px; border-radius: 4px; font-family: var(--font-mono, monospace); }
  .preview-list {
    list-style: none;
    margin: 0;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  .preview-card {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 8px 10px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .card-num {
    font-size: 11px;
    font-weight: 700;
    color: var(--accent);
    background: rgba(79,140,255,0.12);
    border-radius: 999px;
    min-width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    flex-shrink: 0;
  }
  .card-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .card-title { font-size: 12px; font-weight: 600; color: var(--text); }
  .card-text { font-size: 11px; color: var(--text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: block; max-width: 220px; }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
    background: var(--panel);
    flex-shrink: 0;
  }
  .spacer { flex: 1; }
  button {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel-2);
    color: var(--text);
    padding: 7px 14px;
    font-size: 13px;
    font-weight: 500;
  }
  button.ghost { background: transparent; }
  button.ghost:hover { background: var(--panel-2); border-color: var(--brand-slate-400, #94a3b8); }
  button.primary { background: var(--accent); border-color: var(--accent); color: white; box-shadow: 0 2px 8px rgba(79,140,255,0.25); }
  button.primary:hover { background: #3a6fd6; border-color: #3a6fd6; transform: translateY(-1px); }
  button.primary:disabled { opacity: 0.45; cursor: not-allowed; transform: none; }
</style>

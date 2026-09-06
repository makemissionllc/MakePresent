<script lang="ts">
  import type { HAlign, TextStyle, TextStylePatch } from "../lib/types";
  import { DEFAULT_BODY_STYLE, DEFAULT_TITLE_STYLE } from "../lib/types";

  /**
   * Per-text-element style controls for one Look role (Title or Body),
   * FreeShow-textbox-inspired: horizontal alignment, line height, text shadow
   * (blur + X/Y offset), outline (width + color), and an optional
   * semi-transparent readability bar behind the text. Shared by the central
   * Look editor and the Settings → Looks tab so the two never drift.
   *
   * Every change calls `onChange` with a minimal patch; the parent merges it
   * into its optimistic draft (instant preview) and debounces the IPC commit.
   */

  interface Props {
    /** Which text role this section edits. */
    role: "title" | "body";
    /** Current style (falls back to role defaults when the Look predates it). */
    style: TextStyle | null | undefined;
    /** Unique prefix so radio groups stay distinct when two editors mount. */
    uid: string;
    onChange: (patch: TextStylePatch) => void;
  }

  let { role, style, uid, onChange }: Props = $props();

  const s: TextStyle = $derived(
    style ?? (role === "title" ? DEFAULT_TITLE_STYLE : DEFAULT_BODY_STYLE),
  );
  const heading = $derived(role === "title" ? "Title style" : "Body style");
  const sub = $derived(
    role === "title"
      ? "Slide title — also the scripture reference line"
      : "Verse / lyric text",
  );
  const group = $derived(`${uid}-${role}-align`);

  const ALIGNS: { value: HAlign; label: string }[] = [
    { value: "left", label: "Left" },
    { value: "center", label: "Center" },
    { value: "right", label: "Right" },
  ];

  function num(e: Event): number {
    return Number((e.target as HTMLInputElement).value);
  }
</script>

<div class="ts-section">
  <span class="ts-head">{heading} <span class="ts-sub">— {sub}</span></span>

  <div class="ts-row" role="radiogroup" aria-label="{heading} horizontal alignment">
    <span class="ts-label">Align</span>
    <span class="ts-seg-group">
      {#each ALIGNS as a (a.value)}
        <label class="ts-seg" class:on={s.align === a.value}>
          <input
            type="radio"
            name={group}
            checked={s.align === a.value}
            onchange={() => onChange({ align: a.value })}
          />
          {a.label}
        </label>
      {/each}
    </span>
  </div>

  <div class="ts-row">
    <label class="ts-field">
      <span class="ts-label">Line height</span>
      <input
        type="number"
        min="0.8"
        max="3"
        step="0.05"
        value={s.lineHeight}
        oninput={(e) => onChange({ lineHeight: num(e) })}
      />
    </label>
    <label class="ts-field">
      <span class="ts-label">Shadow blur</span>
      <input
        type="number"
        min="0"
        max="60"
        step="1"
        value={s.shadowBlur}
        oninput={(e) => onChange({ shadowBlur: num(e) })}
      />
    </label>
  </div>

  <div class="ts-row">
    <label class="ts-field">
      <span class="ts-label">Shadow X</span>
      <input
        type="number"
        min="-50"
        max="50"
        step="1"
        value={s.shadowX}
        oninput={(e) => onChange({ shadowX: num(e) })}
      />
    </label>
    <label class="ts-field">
      <span class="ts-label">Shadow Y</span>
      <input
        type="number"
        min="-50"
        max="50"
        step="1"
        value={s.shadowY}
        oninput={(e) => onChange({ shadowY: num(e) })}
      />
    </label>
  </div>

  <div class="ts-row">
    <label class="ts-field">
      <span class="ts-label">Outline width</span>
      <input
        type="number"
        min="0"
        max="8"
        step="0.5"
        value={s.outlineWidth}
        oninput={(e) => onChange({ outlineWidth: num(e) })}
      />
    </label>
    <label class="ts-field">
      <span class="ts-label">Outline colour</span>
      <span class="ts-color-line">
        <input
          type="color"
          value={s.outlineColor}
          oninput={(e) => onChange({ outlineColor: (e.target as HTMLInputElement).value })}
        />
        <code>{s.outlineColor}</code>
      </span>
    </label>
  </div>

  <div class="ts-row">
    <label class="ts-field">
      <span class="ts-label">Bar colour</span>
      <span class="ts-color-line">
        <input
          type="color"
          value={s.bgColor}
          oninput={(e) => onChange({ bgColor: (e.target as HTMLInputElement).value })}
        />
        <code>{s.bgColor}</code>
      </span>
    </label>
    <label class="ts-field">
      <span class="ts-label">Bar opacity %</span>
      <input
        type="number"
        min="0"
        max="100"
        step="5"
        value={Math.round(s.bgOpacity * 100)}
        oninput={(e) => onChange({ bgOpacity: num(e) / 100 })}
        title="0 = off. A semi-transparent dark bar (e.g. black at 50%) keeps text readable over busy video/image backgrounds."
      />
    </label>
  </div>
  <span class="ts-hint">Bar at 0% = off. Try black at ~50% for text over video.</span>
</div>

<style>
  .ts-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    border-top: 1px solid var(--border);
    padding-top: 12px;
  }
  .ts-head {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }
  .ts-sub {
    font-weight: 400;
    text-transform: none;
    letter-spacing: 0;
  }
  .ts-row {
    display: flex;
    gap: 12px;
    align-items: flex-end;
  }
  .ts-field {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ts-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-dim);
  }
  .ts-seg-group {
    display: flex;
    gap: 6px;
    flex: 1;
  }
  .ts-seg {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--panel-2);
    font-size: 12px;
    cursor: pointer;
  }
  .ts-seg.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px rgba(79, 140, 255, 0.12);
  }
  .ts-color-line {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ts-hint {
    font-size: 11px;
    color: var(--text-dim);
  }
</style>

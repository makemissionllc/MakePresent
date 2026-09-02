<script lang="ts">
  import { api } from "../lib/sync";
  import type { BoxGeometry, ClientState, Look, LookPatch, Positioning, TextPosition } from "../lib/types";
  import SlideRender from "./SlideRender.svelte";
  import type { Slide } from "../lib/types";

  interface Props {
    appState: ClientState | null;
    onUpdate: (s: ClientState) => void;
    onError: (msg: string) => void;
  }

  let { appState, onUpdate, onError }: Props = $props();

  const looks = $derived(appState?.looks ?? []);
  let activeLookId = $state<string | null>(null);
  const activeLook = $derived.by(() => {
    if (!activeLookId) return looks[0] ?? null;
    return looks.find((l) => l.id === activeLookId) ?? looks[0] ?? null;
  });

  let draft: Look | null = $state(null);
  let lookErr = $state<string | null>(null);

  $effect(() => {
    // Sync draft when active look changes
    draft = activeLook ? { ...activeLook } : null;
  });

  let commitTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleCommit(updated: Look): void {
    const patch: LookPatch = {
      name: updated.name,
      titleSize: updated.titleSize,
      bodySize: updated.bodySize,
      titleFont: updated.titleFont,
      bodyFont: updated.bodyFont,
      textColor: updated.textColor,
      showBackground: updated.showBackground,
      textPosition: updated.textPosition,
      positioning: updated.positioning,
      titleBox: updated.titleBox,
      bodyBox: updated.bodyBox,
    };
    if (commitTimer) clearTimeout(commitTimer);
    commitTimer = setTimeout(() => {
      void api.upsertLook(updated.id, patch).then(onUpdate).catch((e: unknown) => (lookErr = String(e)));
    }, 200);
  }

  function setDraft(field: keyof Look, value: unknown): void {
    lookErr = null;
    if (!draft) return;
    const updated = { ...draft, [field]: value } as Look;
    draft = updated;
    if (appState) {
      onUpdate({ ...appState, looks: appState.looks.map((l) => (l.id === updated.id ? updated : l)) } as ClientState);
    }
    scheduleCommit(updated);
  }

  function selectLook(id: string): void {
    activeLookId = id;
    if (commitTimer) clearTimeout(commitTimer);
  }

  function addLook(): void {
    lookErr = null;
    void api
      .upsertLook(null, {
        name: `Look ${looks.length + 1}`,
        titleSize: 60,
        bodySize: 40,
        titleFont: "sans-serif",
        bodyFont: "sans-serif",
        textColor: "#ffffff",
        showBackground: true,
        textPosition: "center",
        positioning: "auto",
        titleBox: { x: 5, y: 10, width: 90, height: 20, zIndex: 1 },
        bodyBox: { x: 5, y: 35, width: 90, height: 45, zIndex: 1 },
      })
      .then((s) => {
        onUpdate(s);
        const created = s.looks.at(-1);
        if (created) activeLookId = created.id;
      })
      .catch((e: unknown) => (lookErr = String(e)));
  }

  function deleteLook(): void {
    if (!activeLook) return;
    lookErr = null;
    void api
      .deleteLook(activeLook.id)
      .then((s) => {
        onUpdate(s);
        activeLookId = null;
      })
      .catch((e: unknown) => (lookErr = String(e)));
  }

  function assignTo(target: "output" | "stage" | "ndi", id: string | null): void {
    lookErr = null;
    const fn = target === "output" ? api.setOutputLook : target === "stage" ? api.setStageLook : api.setNdiLook;
    void fn(id).then(onUpdate).catch((e: unknown) => (lookErr = String(e)));
  }

  // Bounding box editor
  let boxDrag: { role: "title" | "body"; mode: "move" | "resize" } | null = $state(null);
  let canvasRef = $state<HTMLDivElement | null>(null);

  function boxOf(role: "title" | "body"): BoxGeometry {
    if (!draft) return { x: 5, y: 10, width: 90, height: 20, zIndex: 1 };
    return role === "title" ? draft.titleBox : draft.bodyBox;
  }
  function updateBox(role: "title" | "body", next: BoxGeometry): void {
    if (!draft) return;
    setDraft(role === "title" ? "titleBox" : "bodyBox", next);
  }
  function setPositioning(p: Positioning): void {
    if (!draft) return;
    setDraft("positioning", p);
  }
  function onBoxPointerDown(e: PointerEvent, role: "title" | "body", mode: "move" | "resize") {
    e.preventDefault();
    e.stopPropagation();
    boxDrag = { role, mode };
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
  }
  function onCanvasPointerMove(e: PointerEvent): void {
    if (!boxDrag || !draft) return;
    const el = canvasRef;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const w = Math.max(1, rect.width);
    const h = Math.max(1, rect.height);
    const px = (e.clientX - rect.left) / w;
    const py = (e.clientY - rect.top) / h;
    let cur = boxOf(boxDrag.role);
    let next: BoxGeometry;
    if (boxDrag.mode === "move") {
      next = { ...cur, x: clamp(px * 100 - cur.width / 2), y: clamp(py * 100 - cur.height / 2) };
    } else {
      next = { ...cur, width: clamp(px * 100 - cur.x, 5, 100 - cur.x), height: clamp(py * 100 - cur.y, 5, 100 - cur.y) };
    }
    updateBox(boxDrag.role, next);
  }
  function endBoxDrag(e: PointerEvent): void {
    if (!boxDrag) return;
    const el = e.currentTarget as HTMLElement;
    if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
    boxDrag = null;
  }
  function clamp(v: number, lo = 0, hi = 100): number {
    return Math.min(hi, Math.max(lo, v));
  }
  function n0(v: number): string {
    return Math.round(v).toString();
  }

  // Sample slide for preview — use first project slide or a synthetic one
  const sampleSlide: Slide = $derived.by(() => {
    const s = appState?.project.slides[0];
    if (s) return s;
    return {
      id: "sample",
      libraryId: null,
      librarySlideId: null,
      name: "Sample",
      title: "Welcome to MakrStudio",
      body: "Great is Thy faithfulness\nMorning by morning new mercies I see",
      background: { type: "solid", color: "#123a5c" },
      autoAdvanceSecs: null,
    };
  });
</script>

<div class="look-editor-view">
  <div class="looks-sidebar">
    <div class="looks-sidebar-head">
      <span class="section-title" style="margin:0">Looks — {looks.length}</span>
      <button class="ghost" onclick={addLook}>+ New</button>
    </div>
    <div class="looks-list">
      {#each looks as lk (lk.id)}
        <button class="look-pill" class:active={lk.id === activeLook?.id} onclick={() => selectLook(lk.id)}>
          <span class="look-swatch" style:background-color={lk.textColor}></span>
          <span class="look-pill-name">{lk.name}</span>
          {#if appState?.outputLookId === lk.id}<span class="badge">Output</span>{/if}
          {#if appState?.stageLookId === lk.id}<span class="badge stage">Stage</span>{/if}
          {#if appState?.ndiLookId === lk.id}<span class="badge ndi">NDI</span>{/if}
        </button>
      {/each}
    </div>
    {#if lookErr}<p class="status err">{lookErr}</p>{/if}
  </div>

  <div class="look-main">
    {#if draft}
      <div class="look-preview-wrap">
        <div class="look-preview-box">
          <SlideRender slide={sampleSlide} look={draft} showText={true} showBackground={draft.showBackground} />
        </div>
        <span class="field-hint">Preview — live sample rendered with this Look (updates instantly)</span>
      </div>

      <div class="look-form">
        <label>
          Name
          <input type="text" value={draft.name} oninput={(e) => setDraft("name", (e.target as HTMLInputElement).value)} />
        </label>
        <div class="field-row">
          <label>
            Title size
            <input type="number" min="16" max="300" value={draft.titleSize} oninput={(e) => setDraft("titleSize", Number((e.target as HTMLInputElement).value))} />
          </label>
          <label>
            Body size
            <input type="number" min="16" max="300" value={draft.bodySize} oninput={(e) => setDraft("bodySize", Number((e.target as HTMLInputElement).value))} />
          </label>
        </div>
        <div class="field-row">
          <label>
            Title font
            <select value={draft.titleFont} onchange={(e) => setDraft("titleFont", (e.target as HTMLSelectElement).value)}>
              <option value="sans-serif">Sans-serif</option>
              <option value="Archivo Black">Archivo Black</option>
              <option value="Inter">Inter</option>
              <option value="serif">Serif</option>
              <option value="monospace">Monospace</option>
            </select>
          </label>
          <label>
            Body font
            <select value={draft.bodyFont} onchange={(e) => setDraft("bodyFont", (e.target as HTMLSelectElement).value)}>
              <option value="sans-serif">Sans-serif</option>
              <option value="Archivo Black">Archivo Black</option>
              <option value="Inter">Inter</option>
              <option value="serif">Serif</option>
              <option value="monospace">Monospace</option>
            </select>
          </label>
        </div>
        <label>
          Text colour
          <span class="color-line">
            <input type="color" value={draft.textColor} oninput={(e) => setDraft("textColor", (e.target as HTMLInputElement).value)} />
            <code>{draft.textColor}</code>
          </span>
        </label>
        <label class="check">
          <input type="checkbox" checked={draft.showBackground} onchange={(e) => setDraft("showBackground", (e.target as HTMLInputElement).checked)} />
          Show background
        </label>
        <label>
          Text position
          <select value={draft.textPosition} onchange={(e) => setDraft("textPosition", (e.target as HTMLSelectElement).value as TextPosition)}>
            <option value="top">Top</option>
            <option value="center">Center</option>
            <option value="bottom">Bottom</option>
          </select>
        </label>

        <div class="positioning-row">
          <span class="assign-title">Layout</span>
          <label class="check">
            <input type="radio" name="positioning" checked={draft.positioning === "auto"} onchange={() => setPositioning("auto")} />
            Auto flow
          </label>
          <label class="check">
            <input type="radio" name="positioning" checked={draft.positioning === "absolute"} onchange={() => setPositioning("absolute")} />
            Bounding boxes
          </label>
        </div>

        {#if draft.positioning === "absolute"}
          <div class="box-editor">
            <div class="box-canvas" role="presentation" bind:this={canvasRef} onpointermove={onCanvasPointerMove} onpointerup={endBoxDrag} onpointercancel={endBoxDrag}>
              <div class="box title" role="button" tabindex="0" style:left={`${draft.titleBox.x}%`} style:top={`${draft.titleBox.y}%`} style:width={`${draft.titleBox.width}%`} style:height={`${draft.titleBox.height}%`} style:z-index={draft.titleBox.zIndex} onpointerdown={(e) => onBoxPointerDown(e, "title", "move")}>
                <span class="box-label">Title</span>
                <span class="handle" role="button" tabindex="0" aria-label="Resize title box" onpointerdown={(e) => onBoxPointerDown(e, "title", "resize")}></span>
              </div>
              <div class="box body" role="button" tabindex="0" style:left={`${draft.bodyBox.x}%`} style:top={`${draft.bodyBox.y}%`} style:width={`${draft.bodyBox.width}%`} style:height={`${draft.bodyBox.height}%`} style:z-index={draft.bodyBox.zIndex} onpointerdown={(e) => onBoxPointerDown(e, "body", "move")}>
                <span class="box-label">Body</span>
                <span class="handle" role="button" tabindex="0" aria-label="Resize body box" onpointerdown={(e) => onBoxPointerDown(e, "body", "resize")}></span>
              </div>
            </div>
            <div class="box-fields">
              <div class="box-field"><span class="assign-title">Title</span><code>X {n0(draft.titleBox.x)} · Y {n0(draft.titleBox.y)} · W {n0(draft.titleBox.width)} · H {n0(draft.titleBox.height)}</code></div>
              <div class="box-field"><span class="assign-title">Body</span><code>X {n0(draft.bodyBox.x)} · Y {n0(draft.bodyBox.y)} · W {n0(draft.bodyBox.width)} · H {n0(draft.bodyBox.height)}</code></div>
            </div>
          </div>
        {/if}

        <div class="assign-block">
          <span class="assign-title">Assign to</span>
          <label>
            Main Output
            <select value={appState?.outputLookId ?? ""} onchange={(e) => assignTo("output", (e.target as HTMLSelectElement).value || null)}>
              <option value="">Auto (Main)</option>
              {#each looks as lk (lk.id)}<option value={lk.id}>{lk.name}</option>{/each}
            </select>
          </label>
          <label>
            Stage Display
            <select value={appState?.stageLookId ?? ""} onchange={(e) => assignTo("stage", (e.target as HTMLSelectElement).value || null)}>
              <option value="">Auto (Stage)</option>
              {#each looks as lk (lk.id)}<option value={lk.id}>{lk.name}</option>{/each}
            </select>
          </label>
          <label>
            NDI Feed
            <select value={appState?.ndiLookId ?? ""} onchange={(e) => assignTo("ndi", (e.target as HTMLSelectElement).value || null)}>
              <option value="">Auto (first Look)</option>
              {#each looks as lk (lk.id)}<option value={lk.id}>{lk.name}</option>{/each}
            </select>
          </label>
        </div>

        <button class="danger" onclick={deleteLook}>Delete this look</button>
      </div>
    {:else}
      <p class="looks-empty">Select or create a Look to edit its style.</p>
    {/if}
  </div>
</div>

<style>
  .look-editor-view {
    display: flex;
    gap: 16px;
    height: 100%;
    min-height: 0;
  }
  .looks-sidebar {
    width: 220px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-right: 1px solid var(--border);
    padding-right: 12px;
  }
  .looks-sidebar-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .looks-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
  }
  .look-pill {
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    padding: 8px 10px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--panel-2);
  }
  .look-pill.active {
    border-color: var(--accent);
    background: var(--panel);
    box-shadow: 0 0 0 3px rgba(79,140,255,0.12);
  }
  .look-swatch {
    width: 12px;
    height: 12px;
    border-radius: 3px;
    border: 1px solid var(--border);
    flex: none;
  }
  .look-pill-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }
  .badge {
    font-size: 9px;
    font-weight: 700;
    padding: 2px 5px;
    border-radius: 4px;
    background: var(--accent);
    color: white;
  }
  .badge.stage { background: #64748b; }
  .badge.ndi { background: #7c3aed; }
  .look-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
    overflow-y: auto;
    padding-right: 4px;
  }
  .look-preview-wrap {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .look-preview-box {
    aspect-ratio: 16 / 9;
    background: #000;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    position: relative;
  }
  .look-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 520px;
  }
  .field-row {
    display: flex;
    gap: 12px;
  }
  .field-row label { flex: 1; }
  .color-line {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .check {
    flex-direction: row;
    align-items: center;
  }
  .positioning-row {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .assign-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }
  .assign-block {
    display: flex;
    flex-direction: column;
    gap: 8px;
    border-top: 1px solid var(--border);
    padding-top: 12px;
  }
  .box-editor {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .box-canvas {
    position: relative;
    aspect-ratio: 16 / 9;
    background: #0a0a0f;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }
  .box {
    position: absolute;
    border: 2px dashed var(--accent);
    background: rgba(79,140,255,0.08);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: move;
  }
  .box.body { border-color: #1f9d6a; background: rgba(31,157,106,0.08); }
  .box-label {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
    pointer-events: none;
  }
  .handle {
    position: absolute;
    right: -6px;
    bottom: -6px;
    width: 12px;
    height: 12px;
    background: var(--accent);
    border: 2px solid white;
    border-radius: 3px;
    cursor: nwse-resize;
  }
  .box.body .handle { background: #1f9d6a; }
  .box-fields {
    display: flex;
    gap: 12px;
    font-size: 11px;
  }
  .box-field { flex: 1; display: flex; flex-direction: column; gap: 4px; }
  .looks-empty {
    color: var(--text-dim);
    padding: 20px;
    text-align: center;
  }
  .danger {
    background: var(--danger-bg);
    border-color: var(--danger);
    color: var(--danger-text);
  }
  .status.err {
    color: var(--danger-text);
    background: var(--danger-bg);
    padding: 8px;
    border-radius: 6px;
  }
</style>

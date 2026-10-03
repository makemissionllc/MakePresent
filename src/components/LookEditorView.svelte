<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/sync";
  import type { Background, BoxGeometry, ClientState, Look, LookPatch, Positioning, TextPosition, TextStyle, TextStylePatch } from "../lib/types";
  import { DEFAULT_BODY_STYLE, DEFAULT_TITLE_STYLE } from "../lib/types";
  import { isMedia } from "../lib/types";
  import SlideThumbnail from "./SlideThumbnail.svelte";
  import LookStyleFields from "./LookStyleFields.svelte";
  import { surface } from "../lib/motion";
  import type { Slide } from "../lib/types";

  const PALETTE = ["#1a1a24", "#0f2b4a", "#123a5c", "#1f3a2f", "#3a2b1f", "#3d1f1f", "#2b2b3d", "#000000"];

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
  let layoutEditMode = $state(false);

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
      titleStyle: updated.titleStyle ?? { ...DEFAULT_TITLE_STYLE },
      bodyStyle: updated.bodyStyle ?? { ...DEFAULT_BODY_STYLE },
      positioning: updated.positioning,
      titleBox: updated.titleBox,
      bodyBox: updated.bodyBox,
      background: updated.background ?? null,
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

  /** Merge one per-element style patch into the optimistic draft (preview
      re-renders instantly via the same path as every other Look edit). */
  function setStyle(role: "title" | "body", patch: TextStylePatch): void {
    if (!draft) return;
    const key = role === "title" ? "titleStyle" : "bodyStyle";
    const current: TextStyle =
      draft[key] ?? (role === "title" ? DEFAULT_TITLE_STYLE : DEFAULT_BODY_STYLE);
    setDraft(key, { ...current, ...patch });
  }

  function selectLook(id: string): void {
    activeLookId = id;
    layoutEditMode = false;
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
        titleStyle: { ...DEFAULT_TITLE_STYLE },
        bodyStyle: { ...DEFAULT_BODY_STYLE },
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

  function assignTo(target: "output" | "stage", id: string | null): void {
    lookErr = null;
    const fn = target === "output" ? api.setOutputLook : api.setStageLook;
    void fn(id).then(onUpdate).catch((e: unknown) => (lookErr = String(e)));
  }

  // Bounding box editor
  let boxDrag: {
    role: "title" | "body";
    mode: "move" | "resize";
    startX: number;
    startY: number;
    initial: BoxGeometry;
  } | null = $state(null);
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
  function applyLayoutPreset(preset: "centered" | "lower-third" | "split"): void {
    if (!draft) return;
    const boxes = {
      centered: {
        title: { x: 8, y: 20, width: 84, height: 22, zIndex: 1 },
        body: { x: 10, y: 46, width: 80, height: 34, zIndex: 1 },
        align: "center" as const,
      },
      "lower-third": {
        title: { x: 8, y: 58, width: 84, height: 13, zIndex: 1 },
        body: { x: 8, y: 72, width: 84, height: 20, zIndex: 1 },
        align: "left" as const,
      },
      split: {
        title: { x: 7, y: 18, width: 39, height: 64, zIndex: 1 },
        body: { x: 52, y: 18, width: 41, height: 64, zIndex: 1 },
        align: "left" as const,
      },
    }[preset];
    setPositioning("absolute");
    updateBox("title", boxes.title);
    updateBox("body", boxes.body);
    setStyle("title", { align: boxes.align });
    setStyle("body", { align: boxes.align });
    layoutEditMode = true;
  }
  function startCanvasEdit(): void {
    if (draft?.positioning !== "absolute") setPositioning("absolute");
    layoutEditMode = true;
  }
  function onBoxPointerDown(e: PointerEvent, role: "title" | "body", mode: "move" | "resize") {
    e.preventDefault();
    e.stopPropagation();
    boxDrag = { role, mode, startX: e.clientX, startY: e.clientY, initial: { ...boxOf(role) } };
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
    const initial = boxDrag.initial;
    const dx = ((e.clientX - boxDrag.startX) / w) * 100;
    const dy = ((e.clientY - boxDrag.startY) / h) * 100;
    let next: BoxGeometry;
    if (boxDrag.mode === "move") {
      next = {
        ...initial,
        x: clamp(initial.x + dx, 0, 100 - initial.width),
        y: clamp(initial.y + dy, 0, 100 - initial.height),
      };
    } else {
      next = {
        ...initial,
        width: clamp(initial.width + dx, 5, 100 - initial.x),
        height: clamp(initial.height + dy, 5, 100 - initial.y),
      };
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
      kind: "generic",
      title: "Welcome to MakrStudio",
      body: "Great is Thy faithfulness\nMorning by morning new mercies I see",
      background: { type: "solid", color: "#123a5c" },
      autoAdvanceSecs: null,
    };
  });

  function fileUrl(path: string): string {
    try { return convertFileSrc(path); } catch { return ""; }
  }

  function setLookBackgroundColor(color: string): void {
    if (!draft) return;
    setDraft("background", { type: "solid", color } as Background);
  }

  function clearLookBackground(): void {
    if (!draft) return;
    setDraft("background", null);
  }

  async function pickLookBackgroundMedia(): Promise<void> {
    try {
      const picked = await open({ multiple: false, filters: [{ name: "Images", extensions: ["png","jpg","jpeg","gif","webp","bmp","tiff","svg","avif"] }, { name: "Videos", extensions: ["mp4","m4v","mov","webm","mkv","avi","ogv"] }] });
      if (typeof picked !== "string") return;
      const asset = await api.importMedia(picked);
      setDraft("background", asset.background);
    } catch (e) { lookErr = String(e); }
  }
</script>

<div class="look-editor-view">
  <div class="looks-sidebar">
    <div class="looks-sidebar-head">
      <span class="section-title" style="margin:0">Looks — {looks.length}</span>
      <button class="ghost" onclick={addLook}>+ New</button>
    </div>
    <div class="looks-list">
      {#each looks as lk (lk.id)}
        <button class="look-pill" class:active={lk.id === activeLook?.id} aria-pressed={lk.id === activeLook?.id} onclick={() => selectLook(lk.id)}>
          <span class="look-swatch" style:background-color={lk.textColor}></span>
          <span class="look-pill-name">{lk.name}</span>
          {#if appState?.outputLookId === lk.id}<span class="badge">Output</span>{/if}
          {#if appState?.stageLookId === lk.id}<span class="badge stage">Stage</span>{/if}
        </button>
      {/each}
    </div>
    {#if lookErr}<p class="status err">{lookErr}</p>{/if}
  </div>

  <div class="look-main">
    {#if draft}
      <div class="look-preview-wrap">
        <div class="look-preview-stage">
          {#key draft.id}
            <div class="look-preview-box" in:surface={{ duration: 220, distance: 6, scale: 0.006 }}>
              <SlideThumbnail slide={sampleSlide} look={draft} showText={true} showBackground={draft.showBackground} aspectRatio={appState?.project.aspectRatio ?? "16:9"} />
            </div>
          {/key}
          {#if draft.positioning === "absolute" && layoutEditMode}
            <div class="box-canvas" role="presentation" bind:this={canvasRef} onpointermove={onCanvasPointerMove} onpointerup={endBoxDrag} onpointercancel={endBoxDrag}>
              <div class="box title" role="button" tabindex="0" aria-label="Move or resize title" style:left={`${draft.titleBox.x}%`} style:top={`${draft.titleBox.y}%`} style:width={`${draft.titleBox.width}%`} style:height={`${draft.titleBox.height}%`} style:z-index={draft.titleBox.zIndex} onpointerdown={(e) => onBoxPointerDown(e, "title", "move")}>
                <span class="box-label">Title <small>drag to move</small></span>
                <span class="handle" role="button" tabindex="0" aria-label="Resize title box" onpointerdown={(e) => onBoxPointerDown(e, "title", "resize")}></span>
              </div>
              <div class="box body" role="button" tabindex="0" aria-label="Move or resize body" style:left={`${draft.bodyBox.x}%`} style:top={`${draft.bodyBox.y}%`} style:width={`${draft.bodyBox.width}%`} style:height={`${draft.bodyBox.height}%`} style:z-index={draft.bodyBox.zIndex} onpointerdown={(e) => onBoxPointerDown(e, "body", "move")}>
                <span class="box-label">Body <small>drag to move</small></span>
                <span class="handle" role="button" tabindex="0" aria-label="Resize body box" onpointerdown={(e) => onBoxPointerDown(e, "body", "resize")}></span>
              </div>
            </div>
          {/if}
        </div>
        <div class="layout-presets" aria-label="Starting layout">
          <div class="preset-heading"><span>Start with a layout</span><small>Drag blocks on the preview to fine-tune</small></div>
          <div class="preset-options">
            <button class="preset-card" onclick={() => applyLayoutPreset("centered")} title="Title above centered body">
              <span class="preset-diagram centered"><i></i><i></i></span><span>Centered</span>
            </button>
            <button class="preset-card" onclick={() => applyLayoutPreset("lower-third")} title="Text aligned along the lower third">
              <span class="preset-diagram lower"><i></i><i></i></span><span>Lower third</span>
            </button>
            <button class="preset-card" onclick={() => applyLayoutPreset("split")} title="Title and body side by side">
              <span class="preset-diagram split"><i></i><i></i></span><span>Side by side</span>
            </button>
          </div>
          <div class="canvas-tools">
            {#if draft.positioning === "absolute" && layoutEditMode}
              <span class="canvas-edit-status"><span></span> Canvas editing</span>
              <button class="canvas-edit-toggle" onclick={() => (layoutEditMode = false)}>Done</button>
            {:else}
              <span class="canvas-edit-status muted">Position blocks freely on the slide</span>
              <button class="canvas-edit-toggle" onclick={startCanvasEdit}>Edit on canvas</button>
            {/if}
          </div>
        </div>
        <span class="field-hint">Live preview updates as you change this Look.</span>
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
        <div class="field">
          <span class="field-label">Default background for new slides using this Look</span>
          <div class="swatches">
            {#each PALETTE as color}
              <button class="swatch" style:background-color={color} class:selected={draft.background?.type === "solid" && draft.background.color.toLowerCase() === color} onclick={() => setLookBackgroundColor(color)} title={color}></button>
            {/each}
            <label class="custom-color">
              <input type="color" value={draft.background?.type === "solid" ? draft.background.color : "#123a5c"} oninput={(e) => setLookBackgroundColor((e.target as HTMLInputElement).value)} />
              <span>Custom</span>
            </label>
            {#if draft.background && isMedia(draft.background)}
              <span class="media-swatch-wrap">
                <button class="swatch media selected" style:background-color="#000" title={draft.background.type === "video" ? "Video" : "Image"}>
                  <img src={fileUrl(draft.background.thumb)} alt="" draggable="false" onerror={(e) => { const t = e.currentTarget as unknown as HTMLImageElement; t.style.display = "none"; }} />
                </button>
                <button class="media-remove" title="Remove" onclick={() => clearLookBackground()}>×</button>
              </span>
            {/if}
            <button class="media-add" title="Pick image/video for Look background" onclick={() => pickLookBackgroundMedia()}>+</button>
            {#if draft.background}
              <button class="ghost" onclick={() => clearLookBackground()} title="Clear Look background">Clear</button>
            {/if}
          </div>
          <span class="field-hint">When this Look is set as default for a kind (Scripture/Song/Generic), new slides of that kind will start with this background (one-time copy).</span>
        </div>
        <label>
          Text position
          <select value={draft.textPosition} onchange={(e) => setDraft("textPosition", (e.target as HTMLSelectElement).value as TextPosition)}>
            <option value="top">Top</option>
            <option value="center">Center</option>
            <option value="bottom">Bottom</option>
          </select>
        </label>

        <LookStyleFields role="title" style={draft.titleStyle} uid="lev" onChange={(p) => setStyle("title", p)} />
        <LookStyleFields role="body" style={draft.bodyStyle} uid="lev" onChange={(p) => setStyle("body", p)} />

        <div class="positioning-row">
          <span class="assign-title">Layout</span>
          <label class="check">
            <input type="radio" name="positioning" checked={draft.positioning === "auto"} onchange={() => { setPositioning("auto"); layoutEditMode = false; }} />
            Auto flow
          </label>
          <label class="check">
            <input type="radio" name="positioning" checked={draft.positioning === "absolute"} onchange={() => { setPositioning("absolute"); layoutEditMode = true; }} />
            Bounding boxes
          </label>
        </div>

        {#if draft.positioning === "absolute"}
          <div class="box-editor">
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
          <p class="field-hint ndi-look-note">NDI mirrors the native Output window, including its assigned Look.</p>
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

  /* Narrow center column (small windows / high zoom): the fixed 220px sidebar
     would leave the preview postage-stamp sized, so stack it as a top strip
     with wrapping pills instead. Same content, no logic change. */
  @media (max-width: 960px) {
    .look-editor-view {
      flex-direction: column;
    }
    .looks-sidebar {
      width: 100%;
      flex: none;
      border-right: none;
      border-bottom: 1px solid var(--border);
      padding-right: 0;
      padding-bottom: 10px;
    }
    .looks-list {
      flex-direction: row;
      flex-wrap: wrap;
    }
    .look-pill {
      flex: 1 1 140px;
    }
  }

  /* Canvas-first editor: keep the slide visible while controls stay within a
     compact inspector, like arranging elements on the screen they affect. */
  .look-editor-view {
    gap: 18px;
    height: calc(100vh - 220px);
    min-height: 480px;
  }
  .looks-sidebar {
    width: clamp(165px, 18vw, 220px);
    padding: 4px 12px 4px 0;
    border-color: rgba(255,255,255,0.07);
  }
  .looks-sidebar-head .section-title {
    color: var(--text);
    font-size: 12px;
    font-weight: 700;
  }
  .look-pill { background: rgba(255,255,255,0.025); border-color: rgba(255,255,255,0.06); }
  .look-pill.active { background: rgba(129,170,149,0.11); border-color: rgba(129,170,149,0.42); box-shadow: inset 2px 0 var(--accent); }
  .look-main {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(260px, 31%);
    grid-template-rows: minmax(0, 1fr);
    align-items: start;
    gap: 18px;
    overflow: hidden;
    padding: 0 2px 0 0;
  }
  .look-preview-wrap {
    position: sticky;
    top: 0;
    min-width: 0;
    gap: 10px;
  }
  .look-preview-stage {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    border: 1px solid rgba(255,255,255,0.15);
    border-radius: 12px;
    background: #090d0c;
    box-shadow: 0 14px 32px rgba(0,0,0,0.3);
  }
  .look-preview-box {
    position: absolute;
    inset: 0;
    aspect-ratio: auto;
    border: 0;
    border-radius: 0;
  }
  .box-canvas {
    position: absolute;
    inset: 0;
    aspect-ratio: auto;
    border: 0;
    border-radius: 0;
    background: transparent;
    pointer-events: none;
    touch-action: none;
  }
  .box {
    pointer-events: auto;
    border: 1px dashed rgba(255,255,255,0.88);
    border-radius: 5px;
    background: rgba(8,13,12,0.25);
    box-shadow: 0 0 0 1px rgba(0,0,0,0.35);
    touch-action: none;
  }
  .box.body { border-color: #72d4a5; background: rgba(23,100,66,0.26); }
  .box:active { cursor: grabbing; }
  .box-label {
    display: grid;
    gap: 3px;
    color: #fff;
    text-shadow: 0 1px 3px #000;
    font-size: 10px;
    letter-spacing: .08em;
  }
  .box-label small { color: rgba(255,255,255,.75); font-size: 8px; font-weight: 500; letter-spacing: 0; }
  .layout-presets {
    display: grid;
    gap: 9px;
    padding: 12px;
    border: 1px solid rgba(255,255,255,0.07);
    border-radius: 10px;
    background: rgba(255,255,255,0.025);
  }
  .preset-heading { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; }
  .preset-heading span { color: var(--text); font-size: 11px; font-weight: 700; }
  .preset-heading small { color: var(--text-dim); font-size: 9px; }
  .preset-options { display: grid; grid-template-columns: repeat(3, minmax(0,1fr)); gap: 8px; }
  .preset-card {
    display: grid;
    justify-items: center;
    gap: 6px;
    padding: 8px 6px 7px;
    border: 1px solid rgba(255,255,255,0.08);
    border-radius: 8px;
    background: rgba(0,0,0,0.16);
    color: var(--text-dim);
    font-size: 9px;
  }
  .preset-card:hover { border-color: rgba(129,170,149,0.48); background: rgba(129,170,149,0.08); color: var(--text); }
  .look-editor-view { animation: looks-arrive var(--motion-normal) var(--ease-emphasized) both; }
  .look-pill, .preset-card {
    transition: background var(--motion-normal) var(--ease-out), border-color var(--motion-normal) var(--ease-out), box-shadow var(--motion-normal) var(--ease-out), color var(--motion-normal) var(--ease-out);
  }
  .look-pill:hover { border-color: rgba(129,170,149,0.45); }
  .preset-card:hover { box-shadow: 0 5px 14px rgba(0,0,0,0.15); }
  @keyframes looks-arrive {
    from { opacity: 0; transform: translateY(6px); }
    to { opacity: 1; transform: none; }
  }
  .preset-diagram {
    position: relative;
    display: flex;
    width: 58px;
    height: 34px;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 3px;
    border: 1px solid rgba(129,170,149,0.32);
    border-radius: 4px;
    background: linear-gradient(145deg, #1e4437, #122a23);
  }
  .preset-diagram i { display: block; height: 3px; width: 29px; border-radius: 5px; background: #e8edeb; }
  .preset-diagram i + i { width: 39px; height: 2px; background: #aac8b7; }
  .preset-diagram.lower { align-items: flex-start; justify-content: flex-end; padding: 0 0 5px 6px; }
  .preset-diagram.lower i { width: 21px; }
  .preset-diagram.lower i + i { width: 34px; }
  .preset-diagram.split { flex-direction: row; gap: 4px; }
  .preset-diagram.split i { width: 17px; height: 19px; border: 1px solid #e8edeb; background: rgba(232,237,235,0.12); }
  .preset-diagram.split i + i { width: 21px; height: 19px; }
  .canvas-tools { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .canvas-edit-status { display: inline-flex; align-items: center; gap: 6px; color: #a8dfc5; font-size: 9px; }
  .canvas-edit-status > span { width: 6px; height: 6px; border-radius: 50%; background: var(--semantic-live); }
  .canvas-edit-status.muted { color: var(--text-dim); }
  .canvas-edit-toggle { padding: 6px 9px; border-color: rgba(129,170,149,0.3); border-radius: 7px; background: rgba(129,170,149,0.1); color: #c0ddcd; font-size: 10px; }
  .look-form {
    width: 100%;
    max-width: none;
    max-height: 100%;
    min-height: 0;
    overflow-y: auto;
    gap: 11px;
    padding: 4px 8px 20px 2px;
  }
  .look-form > label, .look-form > .field { padding: 9px; border: 1px solid rgba(255,255,255,0.06); border-radius: 8px; background: rgba(255,255,255,0.018); }
  .look-form .field-row { gap: 8px; }
  .look-form .field-row label { min-width: 0; }
  .positioning-row, .box-editor, .assign-block { border-color: rgba(255,255,255,0.08); }
  .positioning-row { flex-wrap: wrap; padding: 10px; border: 1px solid rgba(255,255,255,0.07); border-radius: 8px; background: rgba(255,255,255,0.02); }
  .box-editor { padding: 8px; border: 1px solid rgba(255,255,255,0.07); border-radius: 8px; }
  .box-fields { flex-direction: column; gap: 8px; }
  .box-field code { font-size: 9px; }
  .look-form :global(.ts-section) { padding: 10px; border: 1px solid rgba(255,255,255,0.07); border-radius: 8px; background: rgba(255,255,255,0.018); }
  .look-form .assign-block { padding: 10px; border: 1px solid rgba(255,255,255,0.07); border-radius: 8px; background: rgba(255,255,255,0.018); }
  @media (max-width: 960px) {
    .look-editor-view { height: auto; min-height: 0; }
    .look-main { display: flex; flex-direction: column; overflow: visible; }
    .look-preview-wrap { position: static; width: 100%; }
    .look-form { max-height: none; overflow: visible; }
    .look-preview-stage { max-height: 52vh; }
  }
</style>

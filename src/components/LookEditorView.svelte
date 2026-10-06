<script lang="ts">
  import { onDestroy } from "svelte";
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
    dropBackground?: { background: Background; token: number } | null;
    nativeDropActive?: boolean;
  }

  let { appState, onUpdate, onError, dropBackground = null, nativeDropActive = false }: Props = $props();

  const looks = $derived(appState?.looks ?? []);
  let activeLookId = $state<string | null>(null);
  const activeLook = $derived.by(() => {
    if (!activeLookId) return looks.find((l) => l.id === appState?.outputLookId) ?? looks.find((l) => l.name === "Main") ?? looks[0] ?? null;
    return looks.find((l) => l.id === activeLookId) ?? looks.find((l) => l.id === appState?.outputLookId) ?? looks.find((l) => l.name === "Main") ?? looks[0] ?? null;
  });

  let draft: Look | null = $state(null);
  let lookErr = $state<string | null>(null);
  let selectedElement = $state<"title" | "body" | null>(null);
  let sampleKind = $state("song-four");
  let safeMargins = $state(true);
  let showGuides = $state(true);
  let verticalGuide = $state<number | null>(null);
  let horizontalGuide = $state<number | null>(null);
  let undoStack: Look[] = $state([]);
  let redoStack: Look[] = $state([]);
  let baseline: Look | null = $state(null);
  let deleteConfirm = $state(false);
  let replacementLookId = $state("");
  let drawingRevision = $state(0);
  let appliedDropToken = $state<number | null>(null);

  $effect(() => {
    // Sync draft when active look changes
    draft = activeLook ? structuredClone($state.snapshot(activeLook)) : null;
    if (activeLook && activeLookId !== activeLook.id) activeLookId = activeLook.id;
    if (activeLook && baseline?.id !== activeLook.id) baseline = structuredClone($state.snapshot(activeLook));
  });
  $effect(() => {
    if (dropBackground && draft && appliedDropToken !== dropBackground.token) {
      appliedDropToken = dropBackground.token;
      setDraft("background", dropBackground.background);
    }
  });

  let commitTimer: ReturnType<typeof setTimeout> | null = null;
  let pendingLook: Look | null = null;
  let commitQueue: Promise<void> = Promise.resolve();

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
    pendingLook = updated;
    if (commitTimer) clearTimeout(commitTimer);
    commitTimer = setTimeout(() => {
      pendingLook = null;
      commitTimer = null;
      commitLook(updated, patch);
    }, 200);
  }

  function commitLook(updated: Look, patch = lookPatch(updated)): void {
    commitQueue = commitQueue.then(() => api.upsertLook(updated.id, patch).then(onUpdate)).catch((e: unknown) => { lookErr = String(e); });
  }

  function flushCommit(): void {
    if (!commitTimer || !pendingLook) return;
    clearTimeout(commitTimer);
    commitTimer = null;
    const updated = pendingLook;
    pendingLook = null;
    commitLook(updated);
  }
  onDestroy(flushCommit);

  function setDraft(field: keyof Look, value: unknown): void {
    lookErr = null;
    if (!draft) return;
    undoStack = [...undoStack, structuredClone($state.snapshot(draft))].slice(-80);
    redoStack = [];
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
    flushCommit();
    activeLookId = id;
    selectedElement = null;
    undoStack = [];
    redoStack = [];
    const selected = looks.find((look) => look.id === id);
    baseline = selected ? structuredClone($state.snapshot(selected)) : null;
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
    replacementLookId = looks.find((look) => look.id !== activeLook.id)?.id ?? "";
    deleteConfirm = true;
  }

  function confirmDeleteLook(): void {
    if (!activeLook) return;
    lookErr = null;
    void api
      .deleteLook(activeLook.id, replacementLookId || null)
      .then((s) => {
        onUpdate(s);
        activeLookId = null;
        deleteConfirm = false;
      })
      .catch((e: unknown) => (lookErr = String(e)));
  }

  function lookPatch(look: Look): LookPatch {
    return {
      name: look.name, titleSize: look.titleSize, bodySize: look.bodySize,
      titleFont: look.titleFont, bodyFont: look.bodyFont, textColor: look.textColor,
      showBackground: look.showBackground, textPosition: look.textPosition,
      titleStyle: look.titleStyle, bodyStyle: look.bodyStyle, positioning: look.positioning,
      titleBox: look.titleBox, bodyBox: look.bodyBox, background: look.background ?? null,
    };
  }
  async function duplicateLook(): Promise<void> {
    if (!activeLook) return;
    try {
      const state = await api.upsertLook(null, { ...lookPatch(activeLook), name: `${activeLook.name} copy` });
      onUpdate(state);
      const created = state.looks.at(-1);
      if (created) selectLook(created.id);
    } catch (e) { lookErr = String(e); }
  }
  function focusRename(): void {
    selectedElement = null;
    requestAnimationFrame(() => {
      const input = document.getElementById("look-name-input") as HTMLInputElement | null;
      input?.focus();
      input?.select();
    });
  }
  function exportLook(): void {
    if (!activeLook) return;
    const portable = structuredClone($state.snapshot(activeLook));
    if (portable.background?.type === "image") portable.background = { ...portable.background, path: "", thumb: "" };
    if (portable.background?.type === "video") portable.background = { ...portable.background, path: "", thumb: "" };
    const blob = new Blob([JSON.stringify({ format: "makrstudio-look", version: 1, look: portable }, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `${activeLook.name.trim().replace(/[^a-z0-9-_]+/gi, "-") || "look"}.json`;
    anchor.click();
    URL.revokeObjectURL(url);
  }
  async function importLook(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    try {
      const decoded = JSON.parse(await file.text()) as { format?: string; version?: number; look?: unknown };
      const candidate = (decoded.look ?? decoded) as Partial<Look>;
      if (typeof candidate.name !== "string" || !candidate.titleStyle || !candidate.bodyStyle) throw new Error("This file is not a MakrStudio Look JSON file.");
      const parsed = candidate as Look;
      const background = parsed.background ? await api.resolveLookBackground(parsed.background) : null;
      const state = await api.upsertLook(null, { ...lookPatch(parsed), name: parsed.name, background });
      onUpdate(state);
      const created = state.looks.at(-1);
      if (created) selectLook(created.id);
      lookErr = null;
    } catch (e) { lookErr = `Could not import Look: ${String(e)}`; }
    input.value = "";
  }

  function assignTo(target: "output" | "stage", id: string | null): void {
    lookErr = null;
    const fn = target === "output" ? api.setOutputLook : api.setStageLook;
    void fn(id).then(onUpdate).catch((e: unknown) => (lookErr = String(e)));
  }
  function assignKind(kind: "song" | "scripture" | "generic", id: string | null): void {
    lookErr = null;
    void api.setDefaultLook(kind, id).then(onUpdate).catch((e: unknown) => (lookErr = String(e)));
  }

  // Bounding box editor
  let boxDrag: {
    role: "title" | "body";
    mode: "move" | "resize-nw" | "resize-ne" | "resize-sw" | "resize-se";
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
  }
  function onBoxPointerDown(e: PointerEvent, role: "title" | "body", mode: "move" | "resize-nw" | "resize-ne" | "resize-sw" | "resize-se") {
    e.preventDefault();
    e.stopPropagation();
    selectedElement = role;
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
    if (draft.positioning !== "absolute" && Math.hypot(dx, dy) > 0.35) setPositioning("absolute");
    let next: BoxGeometry;
    if (boxDrag.mode === "move") {
      next = {
        ...initial,
        x: clamp(initial.x + dx, 0, 100 - initial.width),
        y: clamp(initial.y + dy, 0, 100 - initial.height),
      };
    } else {
      const west = boxDrag.mode.endsWith("nw") || boxDrag.mode.endsWith("sw");
      const north = boxDrag.mode.endsWith("nw") || boxDrag.mode.endsWith("ne");
      const x = west ? initial.x + dx : initial.x;
      const y = north ? initial.y + dy : initial.y;
      next = {
        ...initial,
        x: clamp(x, 0, initial.x + initial.width - 5),
        y: clamp(y, 0, initial.y + initial.height - 5),
        width: clamp(initial.width + (west ? -dx : dx), 5, west ? initial.x + initial.width : 100 - initial.x),
        height: clamp(initial.height + (north ? -dy : dy), 5, north ? initial.y + initial.height : 100 - initial.y),
      };
    }
    const other = boxOf(boxDrag.role === "title" ? "body" : "title");
    const moving = boxDrag.mode === "move";
    const sx = snapAxis(next.x, next.width, [0, 5, 50, 95, 100, other.x, other.x + other.width, other.x + other.width / 2], moving ? "move" : "resize");
    const sy = snapAxis(next.y, next.height, [0, 5, 50, 95, 100, other.y, other.y + other.height, other.y + other.height / 2], moving ? "move" : "resize");
    if (moving) { next.x = sx.value; next.y = sy.value; }
    else { next.width = sx.value; next.height = sy.value; }
    verticalGuide = showGuides ? sx.guide : null;
    horizontalGuide = showGuides ? sy.guide : null;
    updateBox(boxDrag.role, next);
  }
  function endBoxDrag(e: PointerEvent): void {
    if (!boxDrag) return;
    const el = e.currentTarget as HTMLElement;
    if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
    boxDrag = null;
    verticalGuide = null;
    horizontalGuide = null;
  }
  function clamp(v: number, lo = 0, hi = 100): number {
    return Math.min(hi, Math.max(lo, v));
  }
  function n0(v: number): string {
    return Math.round(v).toString();
  }

  function snapAxis(start: number, size: number, targets: number[], mode: "move" | "resize"): { value: number; guide: number | null } {
    const edges = mode === "move" ? [start, start + size, start + size / 2] : [start + size];
    let best = { delta: 1.25, value: mode === "move" ? start : size, guide: null as number | null };
    for (const edge of edges) for (const target of targets) {
      const delta = Math.abs(edge - target);
      if (delta < best.delta) {
        best = { delta, value: mode === "move" ? start + target - edge : target - start, guide: target };
      }
    }
    return best;
  }

  function applyHistory(value: Look): void {
    draft = structuredClone($state.snapshot(value));
    if (appState) onUpdate({ ...appState, looks: appState.looks.map((look) => look.id === value.id ? structuredClone($state.snapshot(value)) : look) } as ClientState);
    scheduleCommit(value);
  }
  function undo(): void {
    const previous = undoStack.at(-1);
    if (!previous || !draft) return;
    redoStack = [...redoStack, structuredClone($state.snapshot(draft))];
    undoStack = undoStack.slice(0, -1);
    applyHistory(previous);
  }
  function redo(): void {
    const next = redoStack.at(-1);
    if (!next || !draft) return;
    undoStack = [...undoStack, structuredClone($state.snapshot(draft))];
    redoStack = redoStack.slice(0, -1);
    applyHistory(next);
  }
  function revertChanges(): void {
    if (!baseline) return;
    undoStack = [...undoStack, structuredClone($state.snapshot(draft!))].slice(-80);
    redoStack = [];
    applyHistory(baseline);
  }
  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") { selectedElement = null; boxDrag = null; return; }
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || e.target instanceof HTMLTextAreaElement;
    if (typing && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
      e.preventDefault();
      if (e.shiftKey) redo(); else undo();
      return;
    }
    if (!selectedElement || !draft || !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key)) return;
    if (typing) return;
    e.preventDefault();
    if (draft.positioning !== "absolute") setPositioning("absolute");
    const box = boxOf(selectedElement);
    const step = e.shiftKey ? 1 : 0.1;
    const dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0;
    const dy = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
    updateBox(selectedElement, { ...box, x: clamp(box.x + dx, 0, 100 - box.width), y: clamp(box.y + dy, 0, 100 - box.height) });
  }

  // Sample slide for preview — use first project slide or a synthetic one
  const sampleSlide: Slide = $derived.by(() => {
    const base: Slide = {
      id: "sample",
      itemId: null,
      itemName: null,
      libraryId: null,
      librarySlideId: null,
      name: "Sample",
      kind: sampleKind.startsWith("scripture") ? "scripture" : sampleKind.startsWith("song") ? "song" : "generic",
      title: "Welcome to MakrStudio",
      body: "Great is Thy faithfulness\nMorning by morning new mercies I see",
      background: { type: "solid", color: "#123a5c" },
      backgroundMode: "custom",
      autoAdvanceSecs: null,
    };
    if (sampleKind === "song-four") return { ...base, title: "Amazing Grace", body: "Amazing grace, how sweet the sound\nThat saved a wretch like me\nI once was lost, but now am found\nWas blind, but now I see" };
    if (sampleKind === "song-two") return { ...base, title: "Great Is Thy Faithfulness", body: "Great is Thy faithfulness, O God my Father\nThere is no shadow of turning with Thee" };
    if (sampleKind === "scripture") return { ...base, kind: "scripture", title: "John 3:16 · KJV", body: "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life." };
    if (sampleKind === "stress") return { ...base, title: "A Long Title for a Difficult Slide", body: "This long-text stress test checks how the Look handles a paragraph with many words, narrow widths, and several lines. A volunteer can see whether the text remains readable across the screen without clipping." };
    return base;
  });

  function drawingError(error: unknown): void {
    console.error("Could not draw Look", error);
    lookErr = "Couldn't draw this Look. Try another one.";
  }

  async function resetLook(): Promise<void> {
    if (!activeLook) return;
    flushCommit();
    await commitQueue;
    try {
      const state = await api.upsertLook(activeLook.id, {
        titleSize: 72, bodySize: 40, titleFont: "sans-serif", bodyFont: "sans-serif",
        textColor: "#ffffff", showBackground: true, textPosition: "center",
        titleStyle: { ...DEFAULT_TITLE_STYLE }, bodyStyle: { ...DEFAULT_BODY_STYLE },
        positioning: "auto", titleBox: { x: 5, y: 10, width: 90, height: 20, zIndex: 1 },
        bodyBox: { x: 5, y: 35, width: 90, height: 45, zIndex: 1 }, background: null,
      });
      onUpdate(state);
      draft = structuredClone($state.snapshot(state.looks.find((l) => l.id === activeLook!.id)!));
      baseline = structuredClone($state.snapshot(draft));
      selectedElement = null;
      undoStack = [];
      redoStack = [];
      lookErr = null;
      drawingRevision++;
    } catch (error) { lookErr = `Could not reset this Look: ${String(error)}`; }
  }

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

<svelte:window onkeydown={handleKeydown} />
<div class="look-editor-view">
  <div class="looks-sidebar">
    <div class="looks-sidebar-head">
      <span class="section-title" style="margin:0">Looks — {looks.length}</span>
      <button class="ghost" onclick={addLook}>+ New</button>
    </div>
    <div class="looks-list">
      {#each looks as lk (lk.id)}
        <button class="look-pill" class:active={lk.id === activeLook?.id} aria-pressed={lk.id === activeLook?.id} onclick={() => selectLook(lk.id)}>
          <span class="gallery-thumb">
            <svelte:boundary>
              <SlideThumbnail slide={sampleSlide} effectiveBackground={lk.background ?? sampleSlide.background} look={lk} showText={true} showBackground={lk.showBackground} fallbackColor="#123a5c" aspectRatio={appState?.project.aspectRatio ?? "16:9"} />
              {#snippet failed()}<span class="thumbnail-error">Preview unavailable</span>{/snippet}
            </svelte:boundary>
          </span>
          <span class="look-swatch" style:background-color={lk.textColor}></span>
          <span class="look-pill-name">{lk.name}</span>
          {#if appState?.outputLookId === lk.id}<span class="badge">Output</span>{/if}
          {#if appState?.stageLookId === lk.id}<span class="badge stage">Stage</span>{/if}
          {#if appState?.defaultLooks?.song === lk.id}<span class="badge kind-song">Songs</span>{/if}
          {#if appState?.defaultLooks?.scripture === lk.id}<span class="badge kind-scripture">Scripture</span>{/if}
          {#if appState?.defaultLooks?.generic === lk.id}<span class="badge kind-text">Text</span>{/if}
          {#if appState?.items?.some((item) => item.kind === "song" && appState?.project.itemLooks?.[item.id] === lk.id) && appState?.defaultLooks?.song !== lk.id}<span class="badge kind-song">Songs</span>{/if}
          {#if appState?.items?.some((item) => item.kind === "scripture" && appState?.project.itemLooks?.[item.id] === lk.id) && appState?.defaultLooks?.scripture !== lk.id}<span class="badge kind-scripture">Scripture</span>{/if}
          {#if appState?.items?.some((item) => item.kind === "generic" && appState?.project.itemLooks?.[item.id] === lk.id) && appState?.defaultLooks?.generic !== lk.id}<span class="badge kind-text">Text</span>{/if}
        </button>
      {/each}
    </div>
    {#if lookErr}<p class="status err">{lookErr}</p>{/if}
  </div>

  <div class="look-main">
    {#if draft}
      {#key `${draft.id}:${drawingRevision}`}
      <div class="look-preview-wrap">
        <div class="preview-toolbar">
          <label>Sample
            <select bind:value={sampleKind}>
              <option value="song-four">Song · 4 lines</option>
              <option value="song-two">Song · 2 lines</option>
              <option value="scripture">Scripture · long verse</option>
              <option value="stress">Long-text stress test</option>
            </select>
          </label>
          <label class="toggle"><input type="checkbox" bind:checked={safeMargins} /> Safe margins</label>
          <label class="toggle"><input type="checkbox" bind:checked={showGuides} /> Guides</label>
          <span class="toolbar-spacer"></span>
          <button class="ghost" disabled={!undoStack.length} onclick={undo} title="Undo (Ctrl+Z)">↶</button>
          <button class="ghost" disabled={!redoStack.length} onclick={redo} title="Redo (Ctrl+Shift+Z)">↷</button>
          <button class="ghost" disabled={!baseline || JSON.stringify(baseline) === JSON.stringify(draft)} onclick={revertChanges}>Revert changes</button>
        </div>
        <div class="look-preview-stage" data-look-canvas class:native-drop={nativeDropActive}>
          <svelte:boundary onerror={drawingError}>
          {#key draft.id}
            <div class="look-preview-box" in:surface={{ duration: 220, distance: 6, scale: 0.006 }}>
              <SlideThumbnail slide={sampleSlide} effectiveBackground={draft.background ?? sampleSlide.background} look={draft} showText={true} showBackground={draft.showBackground} fallbackColor="#123a5c" aspectRatio={appState?.project.aspectRatio ?? "16:9"} />
            </div>
          {/key}
          {#if safeMargins}<div class="safe-margin" aria-hidden="true"></div>{/if}
          <div class="box-canvas" role="presentation" bind:this={canvasRef} onpointermove={onCanvasPointerMove} onpointerup={endBoxDrag} onpointercancel={endBoxDrag}>
              {#if showGuides}<div class="center-v"></div><div class="center-h"></div>{/if}
              {#if verticalGuide !== null}<div class="snap-v" style:left={`${verticalGuide}%`}></div>{/if}
              {#if horizontalGuide !== null}<div class="snap-h" style:top={`${horizontalGuide}%`}></div>{/if}
              <div class="box title" class:selected={selectedElement === "title"} role="button" tabindex="0" aria-label="Select, move, or resize title" style:left={`${draft.titleBox.x}%`} style:top={`${draft.titleBox.y}%`} style:width={`${draft.titleBox.width}%`} style:height={`${draft.titleBox.height}%`} style:z-index={draft.titleBox.zIndex + 3} onpointerdown={(e) => onBoxPointerDown(e, "title", "move")} onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); selectedElement = "title"; } }}>
                <span class="box-label">Title</span>
                {#if selectedElement === "title"}<span class="handle nw" role="button" tabindex="0" aria-label="Resize title box from top left" onpointerdown={(e) => onBoxPointerDown(e, "title", "resize-nw")}></span><span class="handle ne" role="button" tabindex="0" aria-label="Resize title box from top right" onpointerdown={(e) => onBoxPointerDown(e, "title", "resize-ne")}></span><span class="handle sw" role="button" tabindex="0" aria-label="Resize title box from bottom left" onpointerdown={(e) => onBoxPointerDown(e, "title", "resize-sw")}></span><span class="handle se" role="button" tabindex="0" aria-label="Resize title box from bottom right" onpointerdown={(e) => onBoxPointerDown(e, "title", "resize-se")}></span>{/if}
              </div>
              <div class="box body" class:selected={selectedElement === "body"} role="button" tabindex="0" aria-label="Select, move, or resize body" style:left={`${draft.bodyBox.x}%`} style:top={`${draft.bodyBox.y}%`} style:width={`${draft.bodyBox.width}%`} style:height={`${draft.bodyBox.height}%`} style:z-index={draft.bodyBox.zIndex + 3} onpointerdown={(e) => onBoxPointerDown(e, "body", "move")} onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); selectedElement = "body"; } }}>
                <span class="box-label">Body</span>
                {#if selectedElement === "body"}<span class="handle nw" role="button" tabindex="0" aria-label="Resize body box from top left" onpointerdown={(e) => onBoxPointerDown(e, "body", "resize-nw")}></span><span class="handle ne" role="button" tabindex="0" aria-label="Resize body box from top right" onpointerdown={(e) => onBoxPointerDown(e, "body", "resize-ne")}></span><span class="handle sw" role="button" tabindex="0" aria-label="Resize body box from bottom left" onpointerdown={(e) => onBoxPointerDown(e, "body", "resize-sw")}></span><span class="handle se" role="button" tabindex="0" aria-label="Resize body box from bottom right" onpointerdown={(e) => onBoxPointerDown(e, "body", "resize-se")}></span>{/if}
              </div>
          </div>
          {#snippet failed()}
            <div class="drawing-fallback" role="alert">
              <p>Couldn't draw this Look. Try another one.</p>
              <button onclick={() => resetLook()}>Reset this Look</button>
              <p>Pick another Look in the gallery above.</p>
            </div>
          {/snippet}
          </svelte:boundary>
        </div>
        <span class="field-hint">Live preview updates as you change this Look.</span>
      </div>

      <svelte:boundary onerror={drawingError}>
      <div class="look-form" class:context-selected={selectedElement !== null}>
        {#if selectedElement}
          {@const role = selectedElement}
          {@const style = role === "title" ? draft.titleStyle : draft.bodyStyle}
          {@const font = role === "title" ? draft.titleFont : draft.bodyFont}
          {@const size = role === "title" ? draft.titleSize : draft.bodySize}
          <section class="element-inspector">
            <header><strong>{role === "title" ? "Title" : "Body"}</strong><button class="ghost" onclick={() => { selectedElement = null; }}>Deselect</button></header>
            <label>Font
              <select value={font} onchange={(e) => setDraft(role === "title" ? "titleFont" : "bodyFont", (e.currentTarget as HTMLSelectElement).value)}>
                <option value="sans-serif" style:font-family="sans-serif">Sans-serif</option>
                <option value="Inter" style:font-family="Inter">Inter (bundled)</option>
                <option value="Archivo Black" style:font-family="Archivo Black">Archivo Black (bundled)</option>
                <option value="serif" style:font-family="serif">Serif</option>
                <option value="monospace" style:font-family="monospace">Monospace</option>
                <option value="Arial" style:font-family="Arial">Arial (system)</option>
                <option value="Georgia" style:font-family="Georgia">Georgia (system)</option>
              </select>
            </label>
            <p class="font-preview" style:font-family={font}>The quick brown fox — 012345</p>
            <small>The WebView does not expose the full installed-font list, so these include bundled and safe system families. Fonts missing on another computer fall back to a safe font.</small>
            <label>Size
              <input type="number" min="8" max="300" value={size} oninput={(e) => setDraft(role === "title" ? "titleSize" : "bodySize", Number((e.currentTarget as HTMLInputElement).value))} />
            </label>
            <label>Colour <input type="color" value={style.color ?? draft.textColor} oninput={(e) => setStyle(role, { color: (e.currentTarget as HTMLInputElement).value })} /></label>
            <div class="format-toggles">
              <label><input type="checkbox" checked={style.bold} onchange={(e) => setStyle(role, { bold: (e.currentTarget as HTMLInputElement).checked })} /> Bold</label>
              <label><input type="checkbox" checked={style.italic} onchange={(e) => setStyle(role, { italic: (e.currentTarget as HTMLInputElement).checked })} /> Italic</label>
              <label><input type="checkbox" checked={style.allCaps} onchange={(e) => setStyle(role, { allCaps: (e.currentTarget as HTMLInputElement).checked })} /> ALL CAPS</label>
              <label><input type="checkbox" checked={style.shrinkToFit} onchange={(e) => setStyle(role, { shrinkToFit: (e.currentTarget as HTMLInputElement).checked })} /> Shrink to fit</label>
            </div>
            {#if style.shrinkToFit}<label>Minimum size <input type="number" min="8" max="300" value={style.minSize ?? (role === "title" ? 24 : 16)} oninput={(e) => setStyle(role, { minSize: Number((e.currentTarget as HTMLInputElement).value) })} /></label>{/if}
            <div class="align-options"><span>Align</span>{#each ["left", "center", "right"] as align}<button class:active={style.align === align} onclick={() => setStyle(role, { align: align as TextStyle["align"] })}>{align}</button>{/each}</div>
            <details>
              <summary>More</summary>
              <LookStyleFields {role} {style} uid="canvas" showAlignment={false} onChange={(p) => setStyle(role, p)} />
              <label>Position and size
                <div class="numeric-boxes">
                  {#each ["x", "y", "width", "height"] as key}
                    <label>{key.toUpperCase()}<input type="number" min="0" max="100" value={boxOf(role)[key as keyof BoxGeometry]} oninput={(e) => { const b = boxOf(role); updateBox(role, { ...b, [key]: Number((e.currentTarget as HTMLInputElement).value) }); setPositioning("absolute"); }} /></label>
                  {/each}
                </div>
              </label>
            </details>
          </section>
        {/if}
        <label>
          Name
          <input id="look-name-input" type="text" value={draft.name} oninput={(e) => setDraft("name", (e.target as HTMLInputElement).value)} />
        </label>
        <div class="look-actions">
          <button class="ghost" onclick={duplicateLook}>Duplicate</button>
          <button class="ghost" onclick={focusRename}>Rename</button>
          <button class="ghost" onclick={exportLook}>Export .json</button>
          <label class="import-look">Import .json<input type="file" accept="application/json,.json" onchange={importLook} /></label>
        </div>
        {#if !selectedElement}
          <div class="layout-presets inspector-presets" aria-label="Starting layout">
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
          </div>
        {/if}
        <div class="field-row element-only">
          <label>
            Title size
            <input type="number" min="16" max="300" value={draft.titleSize} oninput={(e) => setDraft("titleSize", Number((e.target as HTMLInputElement).value))} />
          </label>
          <label>
            Body size
            <input type="number" min="16" max="300" value={draft.bodySize} oninput={(e) => setDraft("bodySize", Number((e.target as HTMLInputElement).value))} />
          </label>
        </div>
        <div class="field-row element-only">
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
        <label class="element-only">
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
        <label class="element-only">
          Text position
          <select value={draft.textPosition} onchange={(e) => setDraft("textPosition", (e.target as HTMLSelectElement).value as TextPosition)}>
            <option value="top">Top</option>
            <option value="center">Center</option>
            <option value="bottom">Bottom</option>
          </select>
        </label>

        <div class="element-only"><LookStyleFields role="title" style={draft.titleStyle} uid="lev" onChange={(p) => setStyle("title", p)} /></div>
        <div class="element-only"><LookStyleFields role="body" style={draft.bodyStyle} uid="lev" onChange={(p) => setStyle("body", p)} /></div>

        <div class="positioning-row element-only">
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
          <div class="box-editor element-only">
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

        <div class="assign-block">
          <span class="assign-title">Default Look for each kind</span>
          <label>Songs<select value={appState?.defaultLooks?.song ?? ""} onchange={(e) => assignKind("song", (e.currentTarget as HTMLSelectElement).value || null)}><option value="">Use Output Look</option>{#each looks as lk (lk.id)}<option value={lk.id}>{lk.name}</option>{/each}</select></label>
          <label>Scripture<select value={appState?.defaultLooks?.scripture ?? ""} onchange={(e) => assignKind("scripture", (e.currentTarget as HTMLSelectElement).value || null)}><option value="">Use Output Look</option>{#each looks as lk (lk.id)}<option value={lk.id}>{lk.name}</option>{/each}</select></label>
          <label>Text<select value={appState?.defaultLooks?.generic ?? ""} onchange={(e) => assignKind("generic", (e.currentTarget as HTMLSelectElement).value || null)}><option value="">Use Output Look</option>{#each looks as lk (lk.id)}<option value={lk.id}>{lk.name}</option>{/each}</select></label>
          <p class="field-hint">Output Auto uses an item override, then this kind Look, then the Output Look.</p>
        </div>

        {#if deleteConfirm}
          <div class="delete-look-choice">
            <strong>Delete “{activeLook?.name}”</strong>
            <label>Reassign mapped slides and displays to
              <select bind:value={replacementLookId}>
                {#each looks.filter((look) => look.id !== activeLook?.id) as replacement (replacement.id)}<option value={replacement.id}>{replacement.name}</option>{/each}
              </select>
            </label>
            <button class="danger" onclick={confirmDeleteLook}>Delete Look</button>
            <button class="ghost" onclick={() => (deleteConfirm = false)}>Cancel</button>
          </div>
        {:else}
          <button class="danger" onclick={deleteLook}>Delete this look</button>
        {/if}
      </div>
      {#snippet failed()}
        <div class="drawing-fallback inspector-fallback" role="alert">
          <p>Couldn't draw this Look. Try another one.</p>
          <button onclick={() => resetLook()}>Reset this Look</button>
          <p>Pick another Look in the gallery above.</p>
        </div>
      {/snippet}
      </svelte:boundary>
      {/key}
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
    background: #123a5c;
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
    height: auto;
    min-height: 0;
    width: 100%;
    flex-direction: column;
    container-type: inline-size;
  }
  .looks-sidebar {
    width: 100%;
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
    display: flex;
    flex-direction: column;
    align-items: start;
    gap: 18px;
    overflow: visible;
    padding: 0 2px 0 0;
  }
  .look-preview-wrap {
    position: static;
    width: 100%;
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
    background: #123a5c;
    min-height: 160px;
    flex: none;
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
  .look-form.context-selected > :not(.element-inspector) { display: none; }
  .look-form:not(.context-selected) > .element-only { display:none; }
  .element-inspector { display: grid; gap: 11px; padding: 12px; border: 1px solid rgba(129,170,149,.25); border-radius: 10px; background: rgba(129,170,149,.055); }
  .element-inspector header { display:flex; align-items:center; justify-content:space-between; color:var(--text); }
  .element-inspector small { color:var(--text-dim); }
  .font-preview { margin:0; padding:8px; border-radius:6px; background:rgba(0,0,0,.2); font-size:18px; }
  .format-toggles { display:grid; grid-template-columns:1fr 1fr; gap:7px; }
  .format-toggles label { display:flex; align-items:center; flex-direction:row; gap:6px; }
  .align-options { display:flex; gap:6px; align-items:center; }
  .align-options span { margin-right:auto; color:var(--text-dim); }
  .align-options button.active { border-color:var(--accent); background:rgba(129,170,149,.14); }
  .numeric-boxes { display:grid; grid-template-columns:repeat(4,1fr); gap:6px; }
  .numeric-boxes label { font-size:10px; }
  .numeric-boxes input { width:100%; min-width:0; }
  .preview-toolbar { display:flex; align-items:center; gap:7px; flex-wrap:wrap; }
  .preview-toolbar label { display:flex; align-items:center; flex-direction:row; gap:6px; font-size:10px; }
  .preview-toolbar select { width:auto; min-width:130px; }
  .preview-toolbar .toggle input { width:auto; }
  .toolbar-spacer { flex:1; }
  .safe-margin { position:absolute; z-index:4; inset:5%; border:1px dashed rgba(255,255,255,.35); pointer-events:none; }
  .look-preview-stage.native-drop { outline:3px solid var(--accent); outline-offset:2px; }
  .center-v,.center-h,.snap-v,.snap-h { position:absolute; pointer-events:none; z-index:2; }
  .center-v,.snap-v { top:0; bottom:0; width:1px; left:50%; border-left:1px dashed rgba(255,255,255,.25); }
  .center-h,.snap-h { left:0; right:0; height:1px; top:50%; border-top:1px dashed rgba(255,255,255,.25); }
  .snap-v,.snap-h { border-color:#ffd166; border-style:solid; }
  .box { background:rgba(8,13,12,.04); border-color:rgba(255,255,255,.48); }
  .box.selected { border:2px solid rgba(255,255,255,.95); background:rgba(8,13,12,.19); }
  .handle { position:absolute; width:10px; height:10px; background:var(--accent); border:2px solid white; border-radius:3px; cursor:nwse-resize; }
  .handle.nw { left:-6px; top:-6px; }
  .handle.ne { right:-6px; top:-6px; cursor:nesw-resize; }
  .handle.sw { left:-6px; bottom:-6px; cursor:nesw-resize; }
  .handle.se { right:-6px; bottom:-6px; }
  .gallery-thumb { position:relative; display:block; width:54px; height:30px; flex:none; overflow:hidden; border-radius:4px; background:#111; border:1px solid rgba(255,255,255,.15); }
  .gallery-thumb :global(.slide-render) { width:100%; height:100%; font-size:8px; }
  .badge.kind-song { background:#347a55; }
  .badge.kind-scripture { background:#566db3; }
  .badge.kind-text { background:#686879; }
  .look-actions { display:flex; gap:6px; flex-wrap:wrap; }
  .import-look { position:relative; display:inline-flex; align-items:center; padding:6px 10px; border:1px solid var(--border); border-radius:6px; cursor:pointer; }
  .import-look input { position:absolute; inset:0; opacity:0; width:100%; cursor:pointer; }
  .delete-look-choice { display:grid; gap:8px; padding:10px; border:1px solid var(--danger); border-radius:8px; }
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
  .look-form {
    width: 100%;
    max-width: none;
    max-height: none;
    min-height: 0;
    overflow: visible;
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

  }
  .looks-list { display:grid; grid-template-columns:repeat(auto-fit, minmax(160px, 1fr)); gap:8px; }
  .looks-sidebar { border-right:0; border-bottom:1px solid var(--border); padding:0 0 12px; }
  .look-pill { min-width:0; flex-wrap:wrap; }
  .thumbnail-error { display:grid; place-items:center; height:100%; font-size:8px; color:white; }
  .drawing-fallback { display:grid; align-content:center; justify-items:center; gap:10px; padding:20px; min-height:160px; color:white; background:#123a5c; text-align:center; }
  .drawing-fallback p { margin:0; }
  .inspector-fallback { border-radius:10px; }
  @container (min-width: 620px) {
    .look-main { display:grid; grid-template-columns:minmax(0, 1fr) 260px; grid-template-rows:auto; }
  }
</style>

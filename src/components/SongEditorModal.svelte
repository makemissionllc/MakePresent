<script lang="ts">
  import { surface, veil } from "../lib/motion";
  import { onDestroy, untrack } from "svelte";
  import { withTimeout } from "../lib/withTimeout";
  import { smartSongLayout } from "../lib/smartSongLayout";
  import { api } from "../lib/sync";
  import { formatImportedLyrics } from "../lib/lyrics";
  import { findDuplicateSongs } from "../lib/duplicateSongs";
  import type { LyricsHit, LyricsRecord } from "../lib/types";
  import { parseToSongSlides, type SongSlide, type VAlign, type HAlign } from "../lib/parser";

  interface Props {
    open: boolean;
    initialTitle?: string;
    initialBody?: string;
    initialLyricsId?: number | null;
    existingTitles?: string[];
    onConfirm: (title: string, slides: { title: string; body: string; positioning?: { vAlign: VAlign; hAlign: HAlign }; groupId?: string; groupLabel?: string }[], raw: string, details: { author: string; copyright: string; ccliNumber: string; showCreditLine: boolean }) => void;
    onCancel: () => void;
  }

  let { open, initialTitle = "", initialBody = "", initialLyricsId = null, existingTitles = [], onConfirm, onCancel }: Props = $props();

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

  const SONG_EDITOR_PREFERENCES_KEY = "makrstudio.song-editor.preferences.v1";
  interface SongEditorPreferences {
    maxLines: 1 | 2 | 4;
    globalVAlign: VAlign;
    globalHAlign: HAlign;
  }

  function readSongEditorPreferences(): SongEditorPreferences {
    const defaults: SongEditorPreferences = { maxLines: 2, globalVAlign: "center", globalHAlign: "center" };
    try {
      const saved: unknown = JSON.parse(window.localStorage.getItem(SONG_EDITOR_PREFERENCES_KEY) ?? "null");
      if (!saved || typeof saved !== "object" || Array.isArray(saved)) return defaults;
      const value = saved as Partial<SongEditorPreferences>;
      return {
        maxLines: value.maxLines === 1 || value.maxLines === 2 || value.maxLines === 4 ? value.maxLines : defaults.maxLines,
        globalVAlign: value.globalVAlign === "top" || value.globalVAlign === "center" || value.globalVAlign === "bottom" ? value.globalVAlign : defaults.globalVAlign,
        globalHAlign: value.globalHAlign === "left" || value.globalHAlign === "center" || value.globalHAlign === "right" ? value.globalHAlign : defaults.globalHAlign,
      };
    } catch {
      return defaults;
    }
  }

  const initialSongEditorPreferences = readSongEditorPreferences();
  let songEditorPreferences = $state(initialSongEditorPreferences);

  function rememberSongEditorPreferences(next: SongEditorPreferences): void {
    songEditorPreferences = next;
    try {
      window.localStorage.setItem(SONG_EDITOR_PREFERENCES_KEY, JSON.stringify(next));
    } catch {
      // The editor keeps the preference for this session when storage is unavailable.
    }
  }

  const SECTION_COLORS: Record<string, string> = {
    verse: "#4f8cff",
    "verse 1": "#4f8cff",
    "verse 2": "#4f8cff",
    chorus: "#ff7a18",
    "chorus 1": "#ff7a18",
    "chorus 2": "#ff7a18",
    "pre-chorus": "#1f9d6a",
    bridge: "#9b59b6",
    outro: "#64748b",
    intro: "#38bdf8",
    tag: "#f59e0b",
  };
  function colorFor(label: string): string { return SECTION_COLORS[label.toLowerCase()] || "#4f8cff"; }

  let songTitle = $state("");
  let rawText = $state("");
  let author = $state("");
  let copyright = $state("");
  let ccliNumber = $state("");
  let showCreditLine = $state(false);
  let maxLines = $state<1 | 2 | 4>(initialSongEditorPreferences.maxLines);
  let globalVAlign = $state<VAlign>(initialSongEditorPreferences.globalVAlign);
  let globalHAlign = $state<HAlign>(initialSongEditorPreferences.globalHAlign);
  let textareaEl = $state<HTMLTextAreaElement | null>(null);
  let titleInputEl = $state<HTMLInputElement | null>(null);
  let lyricsQueryEl = $state<HTMLInputElement | null>(null);
  let lyricsQuery = $state("");
  let lyricsMatches = $state<LyricsHit[]>([]);
  let lyricsSearching = $state(false);
  let lyricsSearched = $state(false);
  let lyricsFetching = $state<number | null>(null);
  let lyricsError = $state<string | null>(null);
  let importedFrom = $state<string | null>(null);
  let pendingImport = $state<{ record: LyricsRecord; text: string } | null>(null);
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  let lyricsSearchSeq = 0;
  let lyricsFetchSeq = 0;

  // editable arrangement
  let editableSlides = $state<SongSlide[]>([]);
  let draggedIdx = $state<number | null>(null);
  let dragOverIdx = $state<number | null>(null);
  let selectedId = $state<string | null>(null);
  let arrangementUndo = $state<{ slides: SongSlide[]; vAlign: VAlign; hAlign: HAlign } | null>(null);
  let arrangementStatus = $state<string | null>(null);
  const focusFrames = new Set<number>();
  let disposed = false;
  function scheduleFocus(callback: () => void): void {
    if (disposed || !open) return;
    const frame = requestAnimationFrame(() => {
      focusFrames.delete(frame);
      if (!disposed && open) callback();
    });
    focusFrames.add(frame);
  }
  function clearPendingWork(): void {
    lyricsSearchSeq++;
    lyricsFetchSeq++;
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = null;
    for (const frame of focusFrames) cancelAnimationFrame(frame);
    focusFrames.clear();
  }
  onDestroy(() => { disposed = true; clearPendingWork(); });
  function rememberArrangement(): void {
    arrangementUndo = {
      slides: editableSlides.map((slide) => ({ ...slide, lines: [...slide.lines], positioning: { ...slide.positioning } })),
      vAlign: globalVAlign, hAlign: globalHAlign,
    };
    arrangementStatus = null;
  }
  function undoArrangement(): void {
    if (!arrangementUndo) return;
    editableSlides = arrangementUndo.slides;
    globalVAlign = arrangementUndo.vAlign;
    globalHAlign = arrangementUndo.hAlign;
    rememberSongEditorPreferences({ ...songEditorPreferences, globalVAlign, globalHAlign });
    arrangementUndo = null;
    selectedId = null;
    arrangementStatus = "Last arrangement change undone.";
  }
  function applySmartLayout(): void {
    if (!editableSlides.length) return;
    try {
      const layout = smartSongLayout(editableSlides, maxLines);
      rememberArrangement();
      editableSlides = layout;
      selectedId = null;
      arrangementStatus = "Smart layout applied. Review the slides, or Undo to restore the previous arrangement.";
    } catch {
      arrangementStatus = "Could not arrange these lyrics. Your existing slides were kept.";
    }
  }

  $effect(() => {
    if (open) {
      lyricsSearchSeq++;
      lyricsFetchSeq++;
      songTitle = initialTitle;
      rawText = initialBody;
      author = "";
      copyright = "";
      ccliNumber = "";
      showCreditLine = false;
      lyricsQuery = initialTitle;
      lyricsMatches = [];
      lyricsError = null;
      importedFrom = null;
      pendingImport = null;
      lyricsSearching = false;
      lyricsSearched = false;
      lyricsFetching = null;
      maxLines = songEditorPreferences.maxLines;
      globalVAlign = songEditorPreferences.globalVAlign;
      globalHAlign = songEditorPreferences.globalHAlign;
      draggedIdx = null;
      dragOverIdx = null;
      scheduleFocus(() => {
        if (textareaEl?.value) textareaEl?.focus();
        else lyricsQueryEl?.focus();
      });
      if (initialLyricsId != null) void chooseLyrics(initialLyricsId);
      else if (initialTitle.trim().length >= 2) void searchLyrics(initialTitle);
    } else {
      clearPendingWork();
      arrangementUndo = null;
    }
    return clearPendingWork;
  });

  function onLyricsQuery(e: Event): void {
    lyricsQuery = (e.target as HTMLInputElement).value;
    lyricsSearchSeq++;
    lyricsMatches = [];
    lyricsSearching = false;
    lyricsSearched = false;
    lyricsError = null;
    if (searchTimer) clearTimeout(searchTimer);
    if (lyricsQuery.trim().length < 2) {
      return;
    }
    searchTimer = setTimeout(() => void searchLyrics(lyricsQuery), 450);
  }

  async function searchLyrics(value: string): Promise<void> {
    const seq = ++lyricsSearchSeq;
    const query = value.trim();
    if (query.length < 2) return;
    lyricsSearching = true;
    lyricsError = null;
    try {
      const found = await withTimeout(api.searchLyrics(query), 15000, "Lyrics search took too long. Try searching again.");
      if (seq !== lyricsSearchSeq || !open) return;
      lyricsMatches = found.filter((item) => item.hasLyrics).slice(0, 8);
      lyricsSearched = true;
    } catch (e) {
      if (seq !== lyricsSearchSeq || !open) return;
      lyricsMatches = [];
      lyricsError = String(e);
    } finally {
      if (seq === lyricsSearchSeq) lyricsSearching = false;
    }
  }

  function applyImported(record: LyricsRecord, text: string): void {
    songTitle = record.title;
    rawText = text;
    importedFrom = `${record.source} · ${record.artist}`;
    pendingImport = null;
    lyricsMatches = [];
    scheduleFocus(() => textareaEl?.focus());
  }

  async function chooseLyrics(id: number): Promise<void> {
    const seq = ++lyricsFetchSeq;
    lyricsFetching = id;
    lyricsError = null;
    try {
      const record = await withTimeout(api.getLyrics(id), 15000, "Lyrics loading took too long. Choose the result again to retry.");
      if (seq !== lyricsFetchSeq || !open) return;
      const text = formatImportedLyrics(record.lyrics);
      if (!text) throw new Error("This result has no usable lyric lines.");
      if (rawText.trim()) pendingImport = { record, text };
      else applyImported(record, text);
    } catch (e) {
      if (seq === lyricsFetchSeq && open) lyricsError = String(e);
    } finally {
      if (seq === lyricsFetchSeq) lyricsFetching = null;
    }
  }

  // Reparse only when lyric content or line limit changes. Naming and
  // positioning must not destroy manual arrangement edits.
  const parsedBase = $derived(parseToSongSlides(rawText, undefined, { maxLinesPerSlide: maxLines }));
  const parsedMeta = $derived(parsedBase);
  const effectiveTitle = $derived(parsedMeta.metadata.title || songTitle || "Untitled");
  const hasTitle = $derived(Boolean(songTitle.trim() || parsedMeta.metadata.title?.trim()));

  // Intelligent duplicate guard: warn when the title closely matches a song
  // already in the library. First confirm arms the warning; second adds anyway.
  const duplicateHits = $derived(hasTitle ? findDuplicateSongs(effectiveTitle, existingTitles) : []);
  let duplicateAck = $state(false);
  $effect(() => {
    effectiveTitle;
    duplicateAck = false;
  });

  // Opening a session or changing source lyrics regenerates the preview.
  $effect(() => {
    if (!open) return;
    const base = parsedBase.slides;
    const positioning = untrack(() => ({ vAlign: globalVAlign, hAlign: globalHAlign }));
    editableSlides = base.map((s) => ({ ...s, positioning: { ...positioning } }));
    selectedId = null;
    arrangementUndo = null;
    arrangementStatus = null;
  });

  // Apply global positioning to all
  function applyGlobalPositioning(v: VAlign, h: HAlign) {
    rememberArrangement();
    globalVAlign = v;
    globalHAlign = h;
    editableSlides = editableSlides.map((s) => ({ ...s, positioning: { vAlign: globalVAlign, hAlign: globalHAlign } }));
    rememberSongEditorPreferences({ ...songEditorPreferences, globalVAlign: v, globalHAlign: h });
  }

  function changeLineCount(value: 1 | 2 | 4): void {
    maxLines = value;
    rememberSongEditorPreferences({ ...songEditorPreferences, maxLines: value });
  }

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
    const insertion = (needsNewlineBefore ? "\n\n" : rawText ? "\n\n" : "") + header + "\n";
    rawText = before + insertion + after;
    scheduleFocus(() => {
      el.focus();
      const pos = before.length + insertion.length;
      el.setSelectionRange(pos, pos);
    });
  }

  function grouped(): { label: string; slides: SongSlide[]; startIdx: number }[] {
    const runs: { label: string; slides: SongSlide[]; startIdx: number }[] = [];
    editableSlides.forEach((slide, idx) => {
      const last = runs.at(-1);
      if (last && last.slides[0].groupId === slide.groupId) last.slides.push(slide);
      else runs.push({ label: slide.groupLabel, slides: [slide], startIdx: idx });
    });
    return runs;
  }

  const groups = $derived(grouped());

  function onDragStart(e: DragEvent, idx: number): void {
    draggedIdx = idx;
    if (e.dataTransfer) { e.dataTransfer.effectAllowed = "move"; e.dataTransfer.setData("text/plain", String(idx)); }
  }
  function onDragOver(e: DragEvent, idx: number): void {
    e.preventDefault();
    if (draggedIdx === null) return;
    dragOverIdx = idx;
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  }
  function onDrop(e: DragEvent, idx: number): void {
    e.preventDefault();
    e.stopPropagation();
    if (draggedIdx === null) return;
    const from = draggedIdx;
    const to = idx;
    if (from === to) { draggedIdx = null; dragOverIdx = null; return; }
    rememberArrangement();
    const copy = [...editableSlides];
    const [moved] = copy.splice(from, 1);
    const insertAt = to > from ? to - 1 : to;
    copy.splice(insertAt, 0, moved);
    editableSlides = copy;
    draggedIdx = null; dragOverIdx = null;
  }
  function onDropGrid(e: DragEvent): void {
    e.preventDefault();
    if (draggedIdx === null) return;
    const to = dragOverIdx ?? editableSlides.length;
    const from = draggedIdx;
    if (from === to || from + 1 === to) { draggedIdx = null; dragOverIdx = null; return; }
    rememberArrangement();
    const copy = [...editableSlides];
    const [moved] = copy.splice(from, 1);
    copy.splice(to > from ? to - 1 : to, 0, moved);
    editableSlides = copy;
    draggedIdx = null; dragOverIdx = null;
  }
  function duplicate(idx: number): void {
    rememberArrangement();
    const src = editableSlides[idx];
    const clone: SongSlide = { ...src, id: Math.random().toString(36).slice(2,9), lines: [...src.lines], positioning: { ...src.positioning } };
    const copy = [...editableSlides];
    copy.splice(idx + 1, 0, clone);
    editableSlides = copy;
  }
  function duplicateGroup(startIdx: number, count: number): void {
    const toDup = editableSlides.slice(startIdx, startIdx + count);
    if (!toDup.length) return;
    rememberArrangement();
    const clones = toDup.map((s) => ({ ...s, id: Math.random().toString(36).slice(2,9), lines: [...s.lines], positioning: { ...s.positioning }}));
    const copy = [...editableSlides];
    copy.splice(startIdx + count, 0, ...clones);
    editableSlides = copy;
  }
  function removeSlide(idx: number): void {
    rememberArrangement();
    editableSlides = editableSlides.filter((_, i) => i !== idx);
  }

  function setSlideAlign(idx: number, v: VAlign, h: HAlign): void {
    rememberArrangement();
    editableSlides = editableSlides.map((s, i) => i === idx ? { ...s, positioning: { vAlign: v, hAlign: h } } : s);
  }

  function handleConfirm(): void {
    if (!hasTitle || editableSlides.length === 0) return;
    if (duplicateHits.length > 0 && !duplicateAck) {
      duplicateAck = true;
      arrangementStatus = null;
      return;
    }
    const title = effectiveTitle.trim() || "Untitled";
    const payload = editableSlides.map((s, idx) => {
      const run = groups.find((g) => idx >= g.startIdx && idx < g.startIdx + g.slides.length)!;
      return {
        title: run.slides.length > 1 ? `${s.groupLabel} - Part ${idx - run.startIdx + 1}` : s.groupLabel,
        body: s.lines.join("\n"),
        positioning: s.positioning,
        groupId: s.groupId,
        groupLabel: s.groupLabel,
      };
    });
    onConfirm(title, payload, rawText, { author, copyright, ccliNumber, showCreditLine });
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.defaultPrevented) return;
    if (e.key === "Escape") { e.preventDefault(); onCancel(); }
  }
</script>

{#if open}
  <div class="overlay" role="presentation" transition:veil={{ duration: 180 }}>
    <button class="backdrop" aria-label="Close dialog" tabindex="-1" onclick={onCancel}></button>
    <div class="dialog wide" role="dialog" aria-modal="true" aria-label="Song Editor" tabindex="-1" in:surface={{ duration: 280, distance: 14 }} onclick={(e) => e.stopPropagation()} onkeydown={(e) => { e.stopPropagation(); onKeydown(e); }}>
      <header class="dialog-head">
        <h2>Song Editor</h2>
        <div class="head-actions">
          <span class="song-count">{editableSlides.length} slide{editableSlides.length===1?"":"s"}</span>
          <button class="close" title="Close" aria-label="Close" onclick={onCancel}>&times;</button>
        </div>
      </header>

      <div class="controls">
        <label>Lines/slide
          <select value={maxLines} onchange={(e)=> changeLineCount(Number((e.target as HTMLSelectElement).value) as 1|2|4)}>
            <option value={1}>1 line</option>
            <option value={2}>2 lines (default)</option>
            <option value={4}>4 lines</option>
          </select>
        </label>
        <label>All slides: vertical
          <select value={globalVAlign} onchange={(e)=> applyGlobalPositioning((e.target as HTMLSelectElement).value as VAlign, globalHAlign)}>
            <option value="top">Top</option>
            <option value="center">Center</option>
            <option value="bottom">Bottom (Lower-Third)</option>
          </select>
        </label>
        <label>All slides: horizontal
          <select value={globalHAlign} onchange={(e)=> applyGlobalPositioning(globalVAlign, (e.target as HTMLSelectElement).value as HAlign)}>
            <option value="left">Left</option>
            <option value="center">Center</option>
            <option value="right">Right</option>
          </select>
        </label>
        <span class="hint" title="Lines per slide and global alignment are saved on this device.">{effectiveTitle} · Saved setup</span>
      </div>

      <div class="body">
        <div class="editor-pane">
          <section class="lyrics-finder" aria-label="Find song lyrics">
            <div class="finder-heading"><strong>Find lyrics</strong><span>Online · LRCLIB</span></div>
            <div class="finder-search">
              <input bind:this={lyricsQueryEl} type="search" value={lyricsQuery} placeholder="Search song title or artist" aria-label="Search online song lyrics" oninput={onLyricsQuery} onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); if (searchTimer) clearTimeout(searchTimer); void searchLyrics(lyricsQuery); } }} />
              <button class="finder-go" onclick={() => void searchLyrics(lyricsQuery)} disabled={lyricsQuery.trim().length < 2 || lyricsSearching}>{lyricsSearching ? "Searching…" : "Search"}</button>
            </div>
            {#if lyricsError}<p class="finder-error" role="alert">{lyricsError}</p>{/if}
            {#if pendingImport}
              <div class="replace-prompt" role="group" aria-label="Confirm lyric replacement">
                <span>Replace the lyrics below with <strong>{pendingImport.record.title}</strong> by {pendingImport.record.artist}?</span>
                <button class="ghost" onclick={() => (pendingImport = null)}>Keep mine</button>
                <button class="finder-go" onclick={() => { if (pendingImport) applyImported(pendingImport.record, pendingImport.text); }}>Use result</button>
              </div>
            {:else if lyricsMatches.length > 0}
              <ul class="finder-results" aria-label="Online lyric results">
                {#each lyricsMatches as item (item.id)}
                  <li><button class="finder-result" onclick={() => void chooseLyrics(item.id)} disabled={lyricsFetching !== null}>
                    <span class="finder-result-copy"><strong>{item.title}</strong><small>{item.artist}{item.album ? ` · ${item.album}` : ""}</small></span>
                    <span class="finder-result-action">{lyricsFetching === item.id ? "Loading…" : "Use lyrics →"}</span>
                  </button></li>
                {/each}
              </ul>
            {:else if lyricsSearched && lyricsQuery.trim().length >= 2 && !lyricsSearching && !lyricsError && !importedFrom}
              <p class="finder-empty">No matching lyrics yet. Try the artist name, or enter lyrics below.</p>
            {/if}
            {#if importedFrom}<span class="finder-source">Imported from {importedFrom} · Edit any line before adding.</span>{/if}
            <span class="finder-note">Review the wording and your permission to display it before presenting.</span>
          </section>
          <label class="field-label">
            Song Title
            <input bind:this={titleInputEl} type="text" class="title-input" placeholder="e.g. Amazing Grace" value={songTitle} oninput={(e)=> songTitle=(e.target as HTMLInputElement).value} onkeydown={onKeydown} required />
          </label>
          {#if parsedMeta.metadata.title && parsedMeta.metadata.title !== songTitle}
            <span class="meta-hint">Detected title: <strong>{parsedMeta.metadata.title}</strong></span>
          {/if}
          {#if parsedMeta.metadata.style}<span class="meta-hint">Style: {parsedMeta.metadata.style}</span>{/if}

          <details class="song-credit-fields">
            <summary>Song credits <span>Optional</span></summary>
            <div class="credit-input-grid">
              <label>Author<input bind:value={author} maxlength="240" placeholder="Writer or composer" /></label>
              <label>Copyright<input bind:value={copyright} maxlength="240" placeholder="© Year Name" /></label>
              <label>CCLI number<input bind:value={ccliNumber} maxlength="80" placeholder="123456" /></label>
            </div>
            <label class="credit-toggle"><input type="checkbox" bind:checked={showCreditLine} /> Show credit line on Output</label>
          </details>

          <div class="chips">
            {#each TAGS as tag}<button class="chip" onclick={()=>insertTag(tag)}>{tag}</button>{/each}
          </div>

          <textarea bind:this={textareaEl} class="lyrics" placeholder={`Paste full lyrics — e.g.\n\n### Verse 1\nAmazing grace, how sweet the sound\nThat saved a wretch like me\nI once was lost, but now I'm found\nWas blind, but now I see\n\n### Chorus\nMy chains are gone...`} value={rawText} oninput={(e)=> rawText=(e.target as HTMLTextAreaElement).value} onkeydown={onKeydown} rows={12}></textarea>
          <span class="hint">{rawText.length} chars · {editableSlides.length} slides · {groups.length} sections</span>
        </div>

        <div class="preview-pane" role="region" aria-label="Arrangement grid" ondragover={(e)=>{e.preventDefault(); if(draggedIdx!==null && dragOverIdx===null) dragOverIdx=editableSlides.length;}} ondrop={onDropGrid}>
          <div class="preview-head">
            <span>Arrangement</span>
            <span class="preview-count">{editableSlides.length} slides</span>
            <button class="ghost small" disabled={!editableSlides.length} onclick={applySmartLayout} title="Balance lyric length and phrase endings within the selected line limit">Smart layout</button>
            <button class="ghost small" disabled={!arrangementUndo} onclick={undoArrangement}>Undo</button>
          </div>
          <p class="layout-hint">Smart layout keeps lyric order and section groups. Editing lyrics or Lines/slide rebuilds the preview.</p>
          {#if arrangementStatus}<p class="layout-hint" role="status">{arrangementStatus}</p>{/if}

          {#if editableSlides.length===0}
            <p class="empty">No slides — add <code>### Verse 1</code> or type lyrics ({maxLines}-line splitting).</p>
          {:else}
            <div class="grid">
              {#each groups as g}
                <div class="group">
                  <div class="banner" style:border-left-color={colorFor(g.label)}>
                    <span class="banner-label">{g.label}</span>
                    <span class="banner-count">{g.slides.length}</span>
                    <button class="ghost small" title="Duplicate section" onclick={()=>duplicateGroup(g.startIdx, g.slides.length)}>⧉ Duplicate</button>
                  </div>
                  <div class="cards">
                    {#each g.slides as slide, j}
                      {@const globalIdx = g.startIdx + j}
                      {#if dragOverIdx===globalIdx}<div class="drop-line"></div>{/if}
                      <div class="card"
                        draggable="true"
                        class:selected={selectedId===slide.id}
                        class:dragging={draggedIdx===globalIdx}
                        style:border-color={selectedId===slide.id ? colorFor(g.label) : "var(--border)"}
                        ondragstart={(e)=>onDragStart(e, globalIdx)}
                        ondragover={(e)=>onDragOver(e, globalIdx)}
                        ondrop={(e)=>onDrop(e, globalIdx)}
                        ondragend={()=>{draggedIdx=null; dragOverIdx=null;}}
                        onclick={()=> selectedId = slide.id}
                        onkeydown={(e)=>{ if(e.key==="Enter"||e.key===" ") { e.preventDefault(); selectedId=slide.id; } }}
                        role="button"
                        tabindex="0"
                      >
                        <div class="card-top">
                          <span class="card-num" style:background={colorFor(g.label)}>{globalIdx+1}</span>
                          <span class="card-title">{g.label}{g.slides.length>1 ? ` · Part ${j + 1}` : ""}</span>
                          <span class="spacer"></span>
                          <button class="icon" title="Duplicate" onclick={(e)=>{e.stopPropagation(); duplicate(globalIdx);}}>⧉</button>
                          <button class="icon danger" title="Remove" onclick={(e)=>{e.stopPropagation(); removeSlide(globalIdx);}}>×</button>
                        </div>
                        <div class="thumb" style:text-align={slide.positioning.hAlign} style:align-items={slide.positioning.vAlign==="top"?"flex-start":slide.positioning.vAlign==="bottom"?"flex-end":"center"} style:justify-content={slide.positioning.hAlign==="left"?"flex-start":slide.positioning.hAlign==="right"?"flex-end":"center"}>
                          {#each slide.lines as line}<span class="line">{line}</span>{/each}
                          {#if slide.lines.length===0}<span class="line empty-line">(empty)</span>{/if}
                        </div>
                        <div class="align-row">
                          <button class:selected={slide.positioning.vAlign==="top"} onclick={(e)=>{e.stopPropagation(); setSlideAlign(globalIdx,"top", slide.positioning.hAlign);}}>Top</button>
                          <button class:selected={slide.positioning.vAlign==="center"} onclick={(e)=>{e.stopPropagation(); setSlideAlign(globalIdx,"center", slide.positioning.hAlign);}}>Center</button>
                          <button class:selected={slide.positioning.vAlign==="bottom"} onclick={(e)=>{e.stopPropagation(); setSlideAlign(globalIdx,"bottom", slide.positioning.hAlign);}}>Bottom</button>
                          <span class="sep"></span>
                          <button class:selected={slide.positioning.hAlign==="left"} onclick={(e)=>{e.stopPropagation(); setSlideAlign(globalIdx, slide.positioning.vAlign,"left");}}>L</button>
                          <button class:selected={slide.positioning.hAlign==="center"} onclick={(e)=>{e.stopPropagation(); setSlideAlign(globalIdx, slide.positioning.vAlign,"center");}}>C</button>
                          <button class:selected={slide.positioning.hAlign==="right"} onclick={(e)=>{e.stopPropagation(); setSlideAlign(globalIdx, slide.positioning.vAlign,"right");}}>R</button>
                        </div>
                      </div>
                    {/each}
                  </div>
                </div>
              {/each}
              {#if dragOverIdx===editableSlides.length}<div class="drop-line"></div>{/if}
            </div>
          {/if}
        </div>
      </div>

      <div class="actions">
        <span class="spacer"></span>
        {#if duplicateAck && duplicateHits.length > 0}
          <span class="duplicate-warn" role="alert">Possible duplicate of “{duplicateHits[0].title}” ({duplicateHits[0].reason}). Press Add Song again to add anyway.</span>
        {/if}
        <button class="ghost" onclick={onCancel}>Cancel</button>
        <button class="primary" onclick={handleConfirm} disabled={editableSlides.length===0 || !hasTitle}>{duplicateAck && duplicateHits.length > 0 ? "Add Anyway" : `Add Song · ${editableSlides.length} slides`}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .layout-hint { color: var(--text-dim); font-size: 11px; margin: 6px 12px 10px; line-height: 1.5; }
  .overlay{position:fixed; inset:0; z-index:80; display:flex; align-items:center; justify-content:center;}
  .backdrop{position:absolute; inset:0; background:rgba(0,0,0,0.55); border:none; cursor:pointer;}
  .dialog{position:relative; width:min(1120px,97vw); max-height:92vh; background:var(--panel); border:1px solid var(--border); border-radius:12px; box-shadow:0 18px 60px rgba(0,0,0,0.5); overflow:hidden; display:flex; flex-direction:column;}
  .dialog-head{display:flex; align-items:center; justify-content:space-between; padding:12px 16px; border-bottom:1px solid var(--border); background:var(--panel-2); flex-shrink:0;}
  .dialog-head h2{margin:0; font-family:var(--font-display); font-size:14px; font-weight:600; letter-spacing:0.02em; text-transform:uppercase; color:var(--text);}
  .head-actions{display:flex; align-items:center; gap:10px;}
  .song-count{font-size:11px; color:var(--accent); background:rgba(79,140,255,0.12); padding:3px 8px; border-radius:999px; font-weight:700;}
  .close{background:transparent; border:1px solid var(--border); border-radius:6px; width:28px; height:28px; color:var(--text-dim);}
  .close:hover{background:var(--panel); color:var(--text);}
  .controls{display:flex; gap:12px; align-items:center; flex-wrap:wrap; padding:10px 16px; border-bottom:1px solid var(--border); background:var(--panel); font-size:12px; color:var(--text-dim);}
  .controls label{display:flex; gap:6px; align-items:center; font-size:12px;}
  .controls select{background:var(--panel-2); border:1px solid var(--border); border-radius:6px; padding:4px 8px; color:var(--text); font-size:12px; transition:border-color var(--motion-fast) var(--ease-out),box-shadow var(--motion-fast) var(--ease-out);}
  .controls select:focus-visible{outline:2px solid var(--accent); outline-offset:2px; border-color:var(--accent); box-shadow:0 0 0 3px color-mix(in srgb,var(--accent) 15%,transparent);}
  .controls .hint{margin-left:auto; font-size:11px; color:var(--text-dim); max-width:260px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
  .body{display:grid; grid-template-columns: minmax(0,420px) minmax(0,1fr); grid-template-rows:minmax(0,1fr); gap:0; min-height:0; flex:1; overflow:hidden;}
  @media (max-width:860px){ .body{grid-template-columns:minmax(0,1fr); grid-template-rows:minmax(0,1fr) minmax(120px,38vh);} .editor-pane{border-right:none;} .preview-pane{border-left:none; border-top:1px solid var(--border); max-height:none;} }
  .editor-pane{padding:14px; display:flex; flex-direction:column; gap:10px; overflow:auto; min-height:0; border-right:1px solid var(--border);}
  .lyrics-finder{display:grid; gap:9px; padding:12px; border:1px solid rgba(129,170,149,.22); border-radius:11px; background:linear-gradient(135deg,rgba(129,170,149,.09),rgba(129,170,149,.015) 70%);}
  .finder-heading{display:flex; align-items:center; justify-content:space-between; gap:8px; color:var(--text); font-size:12px;}
  .finder-heading span{color:var(--text-dim); font-size:10px;}
  .finder-search{display:flex; gap:7px; min-width:0;}
  .finder-search input{flex:1; min-width:0; padding:8px 10px; border:1px solid var(--border); border-radius:7px; background:var(--panel); color:var(--text); font-size:12px;}
  .finder-search input:focus{outline:none; border-color:var(--accent); box-shadow:0 0 0 3px rgba(129,170,149,.13);}
  .finder-go{flex:none; padding:7px 10px; border:1px solid rgba(129,170,149,.45); border-radius:7px; background:rgba(129,170,149,.17); color:var(--text); font-size:11px; font-weight:700;}
  .finder-go:hover{background:rgba(129,170,149,.27);}
  .finder-results{display:grid; gap:5px; max-height:178px; overflow:auto; margin:0; padding:0 2px 0 0; list-style:none;}
  .finder-result{display:flex; align-items:center; justify-content:space-between; gap:8px; width:100%; min-width:0; padding:8px 9px; border:1px solid var(--border); border-radius:8px; background:var(--panel); text-align:left; transition:background var(--motion-fast) var(--ease-out),border-color var(--motion-fast) var(--ease-out),box-shadow var(--motion-normal) var(--ease-out);}
  .finder-result:hover{border-color:var(--accent); background:var(--panel-2); box-shadow:0 3px 10px rgba(0,0,0,.16);}
  .finder-result-copy{display:grid; gap:2px; min-width:0;}
  .finder-result-copy strong,.finder-result-copy small{overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
  .finder-result-copy strong{color:var(--text); font-size:11px;}
  .finder-result-copy small{color:var(--text-dim); font-size:10px;}
  .finder-result-action{flex:none; color:var(--accent); font-size:10px; font-weight:700;}
  .finder-empty,.finder-error,.finder-note,.finder-source{margin:0; font-size:10px; line-height:1.4;}
  .finder-empty,.finder-note{color:var(--text-dim);}
  .finder-error{color:var(--semantic-error);}
  .finder-source{color:#b9d9c8;}
  .replace-prompt{display:flex; align-items:center; gap:7px; flex-wrap:wrap; padding:9px; border:1px solid rgba(129,170,149,.3); border-radius:8px; background:var(--panel); color:var(--text); font-size:11px; line-height:1.4;}
  .replace-prompt span{flex:1 1 100%;}
  .replace-prompt button{font-size:10px; padding:6px 9px;}
  .field-label{display:flex; flex-direction:column; gap:6px; font-size:12px; color:var(--text-dim);}
  .title-input{background:var(--panel-2); border:1px solid var(--border); border-radius:6px; padding:8px 10px; color:var(--text); font-size:13px;}
  .title-input:focus{outline:none; border-color:var(--accent); box-shadow:0 0 0 3px rgba(79,140,255,0.15);}
  .meta-hint{font-size:11px; color:var(--text-dim);}
  .meta-hint strong{color:var(--text);}
  .song-credit-fields{display:grid;gap:9px;padding:10px;border:1px solid var(--border);border-radius:8px;background:var(--panel-2);}
  .song-credit-fields summary{color:var(--text);font-size:11px;font-weight:700;cursor:pointer;}
  .song-credit-fields summary span{margin-left:5px;color:var(--text-dim);font-size:10px;font-weight:400;}
  .credit-input-grid{display:grid;grid-template-columns:1fr 1fr;gap:8px;}
  .credit-input-grid label{display:grid;gap:4px;color:var(--text-dim);font-size:10px;}
  .credit-input-grid label:first-child{grid-column:1/-1;}
  .credit-input-grid input{min-width:0;padding:6px 8px;border:1px solid var(--border);border-radius:6px;background:var(--panel);color:var(--text);font-size:11px;}
  .credit-toggle{display:flex;align-items:center;gap:7px;color:var(--text-dim);font-size:10px;}
  .credit-toggle input{accent-color:var(--accent);}
  .chips{display:flex; flex-wrap:wrap; gap:5px;}
  .chip{font-size:10px; font-weight:600; padding:4px 8px; border-radius:999px; border:1px solid var(--border); background:var(--panel-2); color:var(--text-dim); cursor:pointer;}
  .chip:hover{background:var(--panel); color:var(--text); border-color:var(--accent);}
  .lyrics{width:100%; min-height:200px; flex:1; background:var(--panel-2); border:1px solid var(--border); border-radius:8px; padding:10px; color:var(--text); font-family:var(--font-mono,monospace); font-size:12px; line-height:1.5; resize:vertical;}
  .lyrics:focus{outline:none; border-color:var(--accent); box-shadow:0 0 0 3px rgba(79,140,255,0.15);}
  .hint{font-size:11px; color:var(--text-dim);}
  .preview-pane{background:var(--panel-2); display:flex; flex-direction:column; min-height:0; overflow:hidden;}
  .preview-head{display:flex; flex-wrap:wrap; gap:8px; align-items:center; justify-content:space-between; padding:10px 12px; border-bottom:1px solid var(--border); font-size:11px; font-weight:700; letter-spacing:0.08em; text-transform:uppercase; color:var(--text-dim);}
  .preview-count{color:var(--accent);}
  .empty{padding:16px; font-size:12px; color:var(--text-dim); line-height:1.5;}
  .empty code{background:var(--panel); padding:1px 4px; border-radius:4px;}
  .grid{overflow:auto; padding:12px; display:flex; flex-direction:column; gap:14px; flex:1; min-height:0;}
  .group{display:flex; flex-direction:column; gap:6px;}
  .banner{display:flex; align-items:center; gap:8px; padding:6px 8px; background:var(--panel); border:1px solid var(--border); border-left-width:4px; border-radius:6px; font-size:11px; font-weight:700; color:var(--text);}
  .banner-label{text-transform:uppercase; letter-spacing:0.06em;}
  .banner-count{background:var(--panel-2); padding:2px 6px; border-radius:999px; font-size:10px; color:var(--text-dim);}
  .ghost.small{font-size:10px; padding:3px 7px; border-radius:999px; margin-left:auto;}
  .cards{display:grid; grid-template-columns: repeat(auto-fill, minmax(180px,1fr)); gap:10px;}
  .card{background:var(--panel); border:1px solid var(--border); border-radius:10px; padding:8px; display:flex; flex-direction:column; gap:6px; cursor:grab; content-visibility:auto; contain-intrinsic-size:auto 200px; transition:transform var(--motion-fast) var(--ease-out),border-color var(--motion-fast) var(--ease-out),box-shadow var(--motion-fast) var(--ease-out);}
  .card:hover:not(:active){transform:translateY(-1px); border-color:var(--accent);}
  .card:active{cursor:grabbing;}
  .card.selected{box-shadow:0 0 0 2px currentColor;}
  .card.dragging{opacity:0.5;}
  .card-top{display:flex; align-items:center; gap:6px; font-size:11px;}
  .card-num{min-width:20px; height:20px; border-radius:999px; display:grid; place-items:center; color:white; font-weight:700; font-size:10px;}
  .card-title{font-weight:600; color:var(--text); font-size:11px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; flex:1;}
  .icon{width:22px; height:22px; border-radius:6px; display:grid; place-items:center; font-size:11px; padding:0; line-height:1;}
  .icon.danger{color:#e11d48;}
  .thumb{border:1px dashed var(--border); border-radius:6px; min-height:64px; padding:8px; display:flex; flex-direction:column; gap:2px; background: rgba(0,0,0,0.15);}
  .thumb .line{font-size:12px; color:var(--text); line-height:1.35; word-break:break-word;}
  .empty-line{color:var(--text-dim); font-style:italic;}
  .align-row{display:flex; gap:3px; align-items:center;}
  .align-row button{font-size:10px; padding:3px 6px; border-radius:4px; flex:1;}
  .align-row button.selected{background:var(--accent); color:white; border-color:var(--accent);}
  .sep{width:1px; height:14px; background:var(--border); margin:0 2px;}
  .drop-line{height:3px; background:var(--accent); border-radius:999px; margin:2px 0;}
  .actions{display:flex; gap:8px; padding:12px 16px; border-top:1px solid var(--border); background:var(--panel); flex-shrink:0; align-items:center;}
  .duplicate-warn{flex:1; min-width:0; font-size:11px; line-height:1.4; color:var(--semantic-warning, #f7b538);}
  .spacer{flex:1;}
  button{border:1px solid var(--border); border-radius:6px; background:var(--panel-2); color:var(--text); padding:7px 14px; font-size:13px; font-weight:500;}
  button.ghost{background:transparent;}
  button.ghost:hover{background:var(--panel-2);}
  button.primary{background:var(--accent); border-color:var(--accent); color:white;}
  button.primary:disabled{opacity:0.45; cursor:not-allowed;}
  @media (max-width:640px){
    .dialog{width:100vw; height:100dvh; max-height:100dvh; border-radius:0;}
    .dialog-head{padding:10px 12px;}
    .controls{gap:7px; padding:8px 10px;}
    .controls label{flex:1 1 150px; justify-content:space-between; min-width:0;}
    .controls select{max-width:58%; min-width:0;}
    .controls .hint{flex:1 1 100%; max-width:none; margin:0;}
    .editor-pane{padding:10px;}
    .actions{gap:6px; padding:8px 10px; flex-wrap:wrap;}
    .duplicate-warn{flex:1 1 100%;}
  }
  @media (prefers-reduced-motion:reduce){.controls select,.card{transition:none}.card:hover:not(:active){transform:none}}
</style>

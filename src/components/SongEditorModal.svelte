<script lang="ts">
  import { parseToSongSlides, type SongSlide, type VAlign, type HAlign } from "../lib/parser";
  import { parseSong } from "../lib/songParser";

  interface Props {
    open: boolean;
    initialTitle?: string;
    initialBody?: string;
    onConfirm: (title: string, slides: { title: string; body: string; positioning?: { vAlign: VAlign; hAlign: HAlign }; groupId?: string; groupLabel?: string }[], raw: string) => void;
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
  let maxLines = $state<1 | 2 | 4>(2);
  let globalVAlign = $state<VAlign>("center");
  let globalHAlign = $state<HAlign>("center");
  let textareaEl = $state<HTMLTextAreaElement | null>(null);
  let titleInputEl = $state<HTMLInputElement | null>(null);

  // editable arrangement
  let editableSlides = $state<SongSlide[]>([]);
  let draggedIdx = $state<number | null>(null);
  let dragOverIdx = $state<number | null>(null);
  let selectedId = $state<string | null>(null);

  $effect(() => {
    if (open) {
      songTitle = initialTitle;
      rawText = initialBody;
      maxLines = 2;
      globalVAlign = "center";
      globalHAlign = "center";
      requestAnimationFrame(() => {
        if (rawText) textareaEl?.focus();
        else titleInputEl?.focus();
      });
    }
  });

  // Parse raw -> slides (respecting maxLines + global positioning)
  const parsedBase = $derived(parseToSongSlides(rawText, songTitle, { maxLinesPerSlide: maxLines, defaultVAlign: globalVAlign, defaultHAlign: globalHAlign }));
  const parsedMeta = $derived(parseSong(rawText, songTitle));
  const effectiveTitle = $derived(parsedMeta.metadata.title || parsedBase.title || songTitle || "Untitled");

  // Sync editableSlides when parse changes (but preserve manual reorder if raw unchanged? we reset on raw/max change)
  $effect(() => {
    // track deps
    const base = parsedBase.slides;
    // copy with fresh ids preserved? use base as source
    editableSlides = base.map((s) => ({ ...s, positioning: { ...s.positioning } }));
    selectedId = null;
  });

  // Apply global positioning to all
  function applyGlobalPositioning() {
    editableSlides = editableSlides.map((s) => ({ ...s, positioning: { vAlign: globalVAlign, hAlign: globalHAlign } }));
  }
  $effect(() => {
    // when globals change, update if editable exists
    void globalVAlign; void globalHAlign;
    if (editableSlides.length) {
      // avoid loop: only if differs
      const need = editableSlides.some((s) => s.positioning.vAlign !== globalVAlign || s.positioning.hAlign !== globalHAlign);
      if (need) editableSlides = editableSlides.map((s) => ({ ...s, positioning: { vAlign: globalVAlign, hAlign: globalHAlign } }));
    }
  });

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
    requestAnimationFrame(() => {
      el.focus();
      const pos = before.length + insertion.length;
      el.setSelectionRange(pos, pos);
    });
  }

  function grouped(): { label: string; id: string; slides: SongSlide[]; startIdx: number }[] {
    const map = new Map<string, SongSlide[]>();
    const order: string[] = [];
    editableSlides.forEach((s) => {
      if (!map.has(s.groupId)) { map.set(s.groupId, []); order.push(s.groupId); }
      map.get(s.groupId)!.push(s);
    });
    let idx = 0;
    return order.map((gid) => {
      const arr = map.get(gid)!;
      const g = { label: arr[0].groupLabel, id: gid, slides: arr, startIdx: idx };
      idx += arr.length;
      return g;
    });
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
    const copy = [...editableSlides];
    const [moved] = copy.splice(from, 1);
    copy.splice(to > from ? to - 1 : to, 0, moved);
    editableSlides = copy;
    draggedIdx = null; dragOverIdx = null;
  }
  function duplicate(idx: number): void {
    const src = editableSlides[idx];
    const clone: SongSlide = { ...src, id: Math.random().toString(36).slice(2,9), lines: [...src.lines], positioning: { ...src.positioning } };
    const copy = [...editableSlides];
    copy.splice(idx + 1, 0, clone);
    editableSlides = copy;
  }
  function duplicateGroup(groupId: string): void {
    const toDup = editableSlides.filter((s) => s.groupId === groupId);
    if (!toDup.length) return;
    const clones = toDup.map((s) => ({ ...s, id: Math.random().toString(36).slice(2,9), lines: [...s.lines], positioning: { ...s.positioning }}));
    // insert after last of group
    let lastIdx = -1;
    editableSlides.forEach((s, i) => { if (s.groupId === groupId) lastIdx = i; });
    const copy = [...editableSlides];
    copy.splice(lastIdx + 1, 0, ...clones);
    editableSlides = copy;
  }
  function removeSlide(idx: number): void {
    editableSlides = editableSlides.filter((_, i) => i !== idx);
  }

  function setSlideAlign(idx: number, v: VAlign, h: HAlign): void {
    editableSlides = editableSlides.map((s, i) => i === idx ? { ...s, positioning: { vAlign: v, hAlign: h } } : s);
  }

  function handleConfirm(): void {
    const title = effectiveTitle.trim() || "Untitled";
    if (editableSlides.length === 0) return;
    const payload = editableSlides.map((s) => ({
      title: s.groupLabel + (editableSlides.filter(x=>x.groupId===s.groupId).length>1 ? ` - Part ${s.subIndex}` : ""),
      body: s.lines.join("\n"),
      positioning: s.positioning,
      groupId: s.groupId,
      groupLabel: s.groupLabel,
    }));
    // Normalize duplicate Part titles: if only one per group, keep groupLabel alone
    // But if multiple, keep Part suffix — already set above
    // For single, the map above added suffix unconditionally, fix:
    const counts = new Map<string, number>();
    payload.forEach((p) => counts.set(p.groupId!, (counts.get(p.groupId!)||0)+1));
    const final = payload.map((p) => ({
      title: (counts.get(p.groupId!)||0) > 1 ? p.title : p.groupLabel!,
      body: p.body,
      positioning: p.positioning!,
      groupId: p.groupId!,
      groupLabel: p.groupLabel!,
    }));
    onConfirm(title, final, rawText);
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") { e.preventDefault(); onCancel(); }
  }
</script>

{#if open}
  <div class="overlay" role="presentation">
    <button class="backdrop" aria-label="Close dialog" tabindex="-1" onclick={onCancel}></button>
    <div class="dialog wide" role="dialog" aria-modal="true" aria-label="Song Editor" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <header class="dialog-head">
        <h2>Song Editor</h2>
        <div class="head-actions">
          <span class="song-count">{editableSlides.length} slide{editableSlides.length===1?"":"s"}</span>
          <button class="close" title="Close" aria-label="Close" onclick={onCancel}>&times;</button>
        </div>
      </header>

      <div class="controls">
        <label>Lines/slide
          <select value={maxLines} onchange={(e)=> maxLines = Number((e.target as HTMLSelectElement).value) as 1|2|4}>
            <option value={1}>1 line</option>
            <option value={2}>2 lines (default)</option>
            <option value={4}>4 lines</option>
          </select>
        </label>
        <label>Vertical
          <select value={globalVAlign} onchange={(e)=> globalVAlign = (e.target as HTMLSelectElement).value as VAlign}>
            <option value="top">Top</option>
            <option value="center">Center</option>
            <option value="bottom">Bottom (Lower-Third)</option>
          </select>
        </label>
        <label>Horizontal
          <select value={globalHAlign} onchange={(e)=> globalHAlign = (e.target as HTMLSelectElement).value as HAlign}>
            <option value="left">Left</option>
            <option value="center">Center</option>
            <option value="right">Right</option>
          </select>
        </label>
        <span class="hint">{effectiveTitle}</span>
      </div>

      <div class="body">
        <div class="editor-pane">
          <label class="field-label">
            Song Title
            <input bind:this={titleInputEl} type="text" class="title-input" placeholder="e.g. Amazing Grace" value={songTitle} oninput={(e)=> songTitle=(e.target as HTMLInputElement).value} onkeydown={onKeydown} />
          </label>
          {#if parsedMeta.metadata.title && parsedMeta.metadata.title !== songTitle}
            <span class="meta-hint">Detected title: <strong>{parsedMeta.metadata.title}</strong></span>
          {/if}
          {#if parsedMeta.metadata.style}<span class="meta-hint">Style: {parsedMeta.metadata.style}</span>{/if}

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
          </div>

          {#if editableSlides.length===0}
            <p class="empty">No slides — add <code>### Verse 1</code> or type lyrics (2-line auto-split).</p>
          {:else}
            <div class="grid">
              {#each groups as g}
                <div class="group">
                  <div class="banner" style:border-left-color={colorFor(g.label)}>
                    <span class="banner-label">{g.label}</span>
                    <span class="banner-count">{g.slides.length}</span>
                    <button class="ghost small" title="Duplicate section" onclick={()=>duplicateGroup(g.id)}>⧉ Duplicate</button>
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
                          <span class="card-title">{g.label}{groups.filter(x=>x.id===g.id).length>1 || g.slides.length>1 ? ` · Part ${slide.subIndex}` : ""}</span>
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
        {#if onBack}<button class="ghost" onclick={onBack}>Back</button>{:else}<button class="ghost" onclick={onCancel}>Cancel</button>{/if}
        <span class="spacer"></span>
        <button class="ghost" onclick={onCancel}>Cancel</button>
        <button class="primary" onclick={handleConfirm} disabled={editableSlides.length===0}>Add Song · {editableSlides.length} slides</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay{position:fixed; inset:0; z-index:60; display:flex; align-items:center; justify-content:center;}
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
  .controls select{background:var(--panel-2); border:1px solid var(--border); border-radius:6px; padding:4px 8px; color:var(--text); font-size:12px;}
  .controls .hint{margin-left:auto; font-size:11px; color:var(--text-dim); max-width:260px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
  .body{display:grid; grid-template-columns: 420px 1fr; gap:0; min-height:0; flex:1; overflow:hidden;}
  @media (max-width:860px){ .body{grid-template-columns:1fr;} .preview-pane{border-left:none; border-top:1px solid var(--border); max-height:42vh;} }
  .editor-pane{padding:14px; display:flex; flex-direction:column; gap:10px; overflow:auto; min-height:0; border-right:1px solid var(--border);}
  .field-label{display:flex; flex-direction:column; gap:6px; font-size:12px; color:var(--text-dim);}
  .title-input{background:var(--panel-2); border:1px solid var(--border); border-radius:6px; padding:8px 10px; color:var(--text); font-size:13px;}
  .title-input:focus{outline:none; border-color:var(--accent); box-shadow:0 0 0 3px rgba(79,140,255,0.15);}
  .meta-hint{font-size:11px; color:var(--text-dim);}
  .meta-hint strong{color:var(--text);}
  .chips{display:flex; flex-wrap:wrap; gap:5px;}
  .chip{font-size:10px; font-weight:600; padding:4px 8px; border-radius:999px; border:1px solid var(--border); background:var(--panel-2); color:var(--text-dim); cursor:pointer;}
  .chip:hover{background:var(--panel); color:var(--text); border-color:var(--accent);}
  .lyrics{width:100%; min-height:200px; flex:1; background:var(--panel-2); border:1px solid var(--border); border-radius:8px; padding:10px; color:var(--text); font-family:var(--font-mono,monospace); font-size:12px; line-height:1.5; resize:vertical;}
  .lyrics:focus{outline:none; border-color:var(--accent); box-shadow:0 0 0 3px rgba(79,140,255,0.15);}
  .hint{font-size:11px; color:var(--text-dim);}
  .preview-pane{background:var(--panel-2); display:flex; flex-direction:column; min-height:0; overflow:hidden;}
  .preview-head{display:flex; justify-content:space-between; padding:10px 12px; border-bottom:1px solid var(--border); font-size:11px; font-weight:700; letter-spacing:0.08em; text-transform:uppercase; color:var(--text-dim);}
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
  .card{background:var(--panel); border:1px solid var(--border); border-radius:10px; padding:8px; display:flex; flex-direction:column; gap:6px; cursor:grab;}
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
  .actions{display:flex; gap:8px; padding:12px 16px; border-top:1px solid var(--border); background:var(--panel); flex-shrink:0;}
  .spacer{flex:1;}
  button{border:1px solid var(--border); border-radius:6px; background:var(--panel-2); color:var(--text); padding:7px 14px; font-size:13px; font-weight:500;}
  button.ghost{background:transparent;}
  button.ghost:hover{background:var(--panel-2);}
  button.primary{background:var(--accent); border-color:var(--accent); color:white;}
  button.primary:disabled{opacity:0.45; cursor:not-allowed;}
</style>

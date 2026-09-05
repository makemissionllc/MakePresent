<script lang="ts">
  import { PRESETS } from "../presets";
  import type { PlaylistTemplate, ServicePreset } from "../types";

  interface Props {
    open: boolean;
    onClose: () => void;
    onCreate: (presetId: string, opts: { title: string; aspect: string; theme: string; transition: string }) => void;
    onCreateFromPlaylist: (playlistId: string, opts: { title: string; aspect: string; theme: string; transition: string }) => void;
    playlists: PlaylistTemplate[];
    recentName?: string;
  }

  let { open, onClose, onCreate, onCreateFromPlaylist, playlists = [], recentName = "" }: Props = $props();

  const ASPECTS = [
    { id: "16:9", label: "16:9 1080p", sub: "1920×1080" },
    { id: "16:9-4k", label: "16:9 4K", sub: "3840×2160" },
    { id: "4:3", label: "4:3 Legacy", sub: "1024×768" },
    { id: "Vertical", label: "Vertical LED Wall", sub: "1080×1920" },
  ];
  const THEMES = ["Dark", "Lower Third", "Gradient", "Motion BG"] as const;
  const TRANSITIONS = [
    { id: "Cut", label: "Cut" },
    { id: "Fade 300ms", label: "Fade 300ms" },
    { id: "Dissolve", label: "Dissolve" },
  ] as const;

  function todayLabel(): string {
    const d = new Date();
    return d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
  }

  interface GalleryItem {
    kind: "preset" | "playlist";
    id: string;
    name: string;
    category: string;
    description: string;
    slideCount: number;
    defaultAspect: string;
    presetId?: string;
    playlistId?: string;
  }

  const gallery = $derived<GalleryItem[]>([
    ...PRESETS.map((p: ServicePreset) => ({
      kind: "preset" as const,
      id: p.id,
      name: p.name,
      category: p.category,
      description: p.description,
      slideCount: p.playlistItems.length,
      defaultAspect: p.defaultAspect,
      presetId: p.id,
    })),
    ...playlists.map((pt: PlaylistTemplate) => ({
      kind: "playlist" as const,
      id: pt.id,
      name: pt.name,
      category: "Saved",
      description: "Saved reusable playlist — load to start a service.\nSaved {new Date(pt.createdAt).toLocaleDateString()}.",
      slideCount: pt.items.length,
      defaultAspect: "16:9",
      playlistId: pt.id,
    })),
  ]);

  const hasSaved = $derived(playlists.length > 0);

  let selectedId = $state<string>("sunday-morning");
  const selected = $derived(gallery.find((g) => g.id === selectedId) ?? gallery[0]);

  let title = $state("");
  let aspect = $state("16:9");
  let theme = $state("Dark");
  let transition = $state("Cut");

  // auto-name when selection changes
  $effect(() => {
    if (!open) return;
    const base = selected.name;
    if (!title) {
      title = `${base} - ${todayLabel()}`;
    }
    aspect = selected.defaultAspect;
  });

  function create(): void {
    const t = title.trim() || `${selected.name} - ${todayLabel()}`;
    const aspectSend = aspect === "16:9-4k" ? "16:9" : aspect;
    if (selected.kind === "playlist" && selected.playlistId) {
      onCreateFromPlaylist(selected.playlistId, { title: t, aspect: aspectSend, theme, transition });
    } else if (selected.presetId) {
      onCreate(selected.presetId, { title: t, aspect: aspectSend, theme, transition });
    }
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === "Escape") onClose();
  }
</script>

{#if open}
  <div class="hub-overlay" role="presentation" onkeydown={onKey}>
    <div class="hub" role="dialog" aria-modal="true" aria-label="View Hub" tabindex="-1">
      <header class="hub-head">
        <div class="brand">
          <span class="logo">MP</span>
          <div>
            <h1>View Hub</h1>
            <p>Start a new View — choose a starting Playlist, then set your title, resolution &amp; theme</p>
          </div>
        </div>
        <button class="close" onclick={onClose} aria-label="Close">&times;</button>
      </header>

      <div class="hub-body">
        <!-- Left & Center: Playlist Gallery (presets + saved playlists, unified) -->
        <section class="gallery" aria-label="Starting Playlists">
          <div class="gallery-head">
            <h2>Starting Playlist</h2>
            {#if recentName}
              <span class="recent-hint">Recent View: {recentName}</span>
            {/if}
          </div>
          {#if hasSaved}
            <p class="gallery-note">Built-in starting points below; your saved Playlists appear at the top of the list.</p>
          {/if}
          <div class="grid">
            {#each gallery as item}
              <button
                class="card"
                class:selected={item.id === selectedId}
                data-kind={item.kind}
                onclick={() => (selectedId = item.id)}
                aria-pressed={item.id === selectedId}
              >
                <span class="card-badge" data-kind={item.kind} data-category={item.category}>{item.category}</span>
                <span class="card-icon">
                  {#if item.kind === "playlist"}♻
                  {:else if item.id === "sunday-morning"}☀️
                  {:else if item.id === "midweek"}🙏
                  {:else if item.id === "youth"}⚡
                  {:else}＋
                  {/if}
                </span>
                <span class="card-title">{item.name}</span>
                <span class="card-desc">{item.description}</span>
                <span class="card-meta">{item.slideCount} slides · {item.defaultAspect}</span>
              </button>
            {/each}
          </div>

          <div class="recent">
            <h3>Recent View</h3>
            {#if recentName}
              <div class="recent-item">
                <span class="dot"></span>
                <span>{recentName}</span>
                <span class="muted">— will be overwritten only on Create</span>
              </div>
            {:else}
              <span class="muted">No recent View yet — your new service will appear here next launch.</span>
            {/if}
          </div>
        </section>

        <!-- Right: Inspector -->
        <aside class="inspector" aria-label="View Configuration">
          <div class="inspector-head">
            <h3>{selected.name}</h3>
            <p>{selected.description}</p>
          </div>

          <label class="field">
            <span>View Title / Date</span>
            <input type="text" value={title} placeholder="Sunday Morning - {todayLabel()}" oninput={(e)=> title=(e.target as HTMLInputElement).value} />
            <span class="help">Auto-naming: e.g. “Sunday Morning - Sept 6, 2026”</span>
          </label>

          <div class="field">
            <span>Target Resolution &amp; Aspect Ratio</span>
            <div class="toggles">
              {#each ASPECTS as a}
                <button class:selected={aspect===a.id} onclick={()=> aspect=a.id}>
                  <strong>{a.label}</strong><span>{a.sub}</span>
                </button>
              {/each}
            </div>
          </div>

          <label class="field">
            <span>Primary Theme / Look</span>
            <select value={theme} onchange={(e)=> theme=(e.target as HTMLSelectElement).value}>
              {#each THEMES as t}<option value={t}>{t}</option>{/each}
            </select>
          </label>

          <label class="field">
            <span>Default Transition</span>
            <select value={transition} onchange={(e)=> transition=(e.target as HTMLSelectElement).value}>
              {#each TRANSITIONS as t}<option value={t.id}>{t.label}</option>{/each}
            </select>
          </label>

          <div class="inspector-actions">
            <button class="ghost" onclick={onClose}>Cancel</button>
            <button class="primary" onclick={create}>Create View — {selected.slideCount} slides</button>
          </div>
        </aside>
      </div>
    </div>
  </div>
{/if}

<style>
  .hub-overlay{position:fixed; inset:0; z-index:70; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,0.55); backdrop-filter: blur(6px);}
  .hub{width:min(1180px,96vw); max-height:92vh; background:var(--panel); border:1px solid var(--border); border-radius:16px; overflow:hidden; display:flex; flex-direction:column; box-shadow:0 24px 80px rgba(0,0,0,0.6);}
  .hub-head{display:flex; align-items:center; justify-content:space-between; padding:18px 20px; border-bottom:1px solid var(--border); background: var(--panel-2);}
  .brand{display:flex; gap:12px; align-items:center; color:var(--text);}
  .logo{width:40px; height:40px; border-radius:10px; display:grid; place-items:center; background: var(--accent); font-weight:800; letter-spacing:0.06em; color:white;}
  .brand h1{margin:0; font-family:var(--font-display); font-size:16px; text-transform:uppercase; letter-spacing:0.06em; color:var(--text);}
  .brand p{margin:2px 0 0; font-size:11px; color:var(--text-dim);}
  .close{width:32px; height:32px; border-radius:8px; background:var(--panel-2); border:1px solid var(--border); color:var(--text-dim); font-size:18px;}
  .close:hover{background:var(--panel); color:var(--text);}
  .hub-body{display:grid; grid-template-columns: 1fr 340px; gap:0; min-height:0; flex:1; overflow:hidden;}
  @media (max-width:900px){ .hub-body{grid-template-columns:1fr;} .inspector{border-left:none; border-top:1px solid var(--border);} }
  .gallery{padding:18px; overflow:auto; background: var(--panel-2); min-height:0;}
  .gallery-head{display:flex; align-items:baseline; justify-content:space-between; margin-bottom:10px;}
  .gallery-head h2{margin:0; font-size:11px; font-weight:700; letter-spacing:0.08em; text-transform:uppercase; color:var(--text-dim);}
  .gallery-note{font-size:11px; color:var(--text-dim); margin:-4px 0 10px;}
  .recent-hint{font-size:11px; color:var(--text-dim);}
  .grid{display:grid; grid-template-columns: repeat(2, minmax(0,1fr)); gap:12px;}
  @media (max-width:640px){ .grid{grid-template-columns:1fr;} }
  .card{position:relative; text-align:left; padding:14px; border-radius:12px; border:1px solid var(--border); background: var(--panel); color:var(--text); min-height:140px; display:flex; flex-direction:column; gap:6px; overflow:hidden;}
  .card:hover{border-color: var(--border); background: var(--panel-2);}
  .card.selected{border-color: var(--accent); background: var(--panel-2); box-shadow:0 0 0 3px rgba(79,140,255,0.15);}
  .card-badge{position:absolute; top:10px; right:10px; font-size:9px; font-weight:700; letter-spacing:0.06em; text-transform:uppercase; padding:3px 7px; border-radius:999px; border:1px solid transparent; color:white;}
  .card-badge[data-category="Sunday Service"]{background: var(--color-green); border-color: var(--color-green);}
  .card-badge[data-category="Midweek"]{background: #1e3a4d; border-color: #234a5e; color: white;}
  .card-badge[data-category="Youth"]{background: var(--brand-orange-500); border-color: var(--brand-orange-500); color:white;}
  .card-badge[data-category="Custom"]{background: var(--panel-2); border-color: var(--border); color: var(--text-dim);}
  .card-badge[data-kind="playlist"]{background: var(--accent); border-color: var(--accent); color:white;}
  .card-icon{font-size:22px; margin-top:6px; opacity:0.9;}
  .card-title{font-family:var(--font-display); font-size:13px; font-weight:700; line-height:1.2; color:var(--text);}
  .card-desc{font-size:11px; color:var(--text-dim); line-height:1.35; white-space:pre-line;}
  .card-meta{margin-top:auto; font-size:10px; color:var(--text-dim);}
  .recent{margin-top:16px; padding-top:12px; border-top:1px solid var(--border);}
  .recent h3{margin:0 0 6px; font-size:11px; font-weight:700; letter-spacing:0.06em; text-transform:uppercase; color:var(--text-dim);}
  .recent-item{display:flex; gap:8px; align-items:center; font-size:12px; color:var(--text);}
  .dot{width:8px; height:8px; border-radius:999px; background:var(--accent);}
  .muted{font-size:11px; color:var(--text-dim);}
  .inspector{padding:16px; overflow:auto; background:var(--panel); border-left:1px solid var(--border); display:flex; flex-direction:column; gap:14px; min-height:0;}
  .inspector-head h3{margin:0; font-size:13px; font-weight:700; color:var(--text);}
  .inspector-head p{margin:4px 0 0; font-size:11px; color:var(--text-dim); line-height:1.4; white-space:pre-line;}
  .field{display:flex; flex-direction:column; gap:6px; font-size:11px; color:var(--text-dim);}
  .field input, .field select{width:100%; background:var(--panel-2); border:1px solid var(--border); border-radius:6px; padding:8px 10px; color:var(--text); font-size:13px;}
  .field input:focus, .field select:focus{outline:none; border-color:var(--accent); box-shadow:0 0 0 3px rgba(79,140,255,0.15);}
  .help{font-size:10px; color:var(--text-dim);}
  .toggles{display:grid; grid-template-columns:1fr 1fr; gap:6px;}
  .toggles button{display:flex; flex-direction:column; gap:2px; padding:8px 10px; border-radius:8px; border:1px solid var(--border); background:var(--panel-2); text-align:left; font-size:11px; color:var(--text);}
  .toggles button.selected{border-color:var(--accent); background:rgba(79,140,255,0.12); color:var(--text);}
  .toggles button strong{font-size:11px;}
  .toggles button span{font-size:10px; color:var(--text-dim);}
  .inspector-actions{display:flex; gap:8px; margin-top:auto; padding-top:6px;}
  button{border:1px solid var(--border); border-radius:6px; background:var(--panel-2); color:var(--text); padding:7px 12px; font-size:13px; font-weight:500;}
  button.ghost{background:transparent;}
  button.primary{background:var(--accent); border-color:var(--accent); color:white; flex:1;}
  button.primary:hover{transform:translateY(-1px);}
</style>

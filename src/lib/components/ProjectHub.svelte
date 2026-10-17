<script lang="ts">
  import type { PlaylistTemplate, ServicePreset } from "../types";
  import BrandLockup from "../../components/BrandLockup.svelte";
  import { surface, veil } from "../motion";

  interface Props {
    open: boolean;
    onClose: () => void;
    onCreate: (presetId: string, opts: { title: string; aspect: string; theme: string; transition: string }) => void | Promise<void>;
    onCreateFromPlaylist: (playlistId: string, opts: { title: string; aspect: string; theme: string; transition: string }) => void | Promise<void>;
    onDeletePlaylist: (playlistId: string) => void;
    playlists: PlaylistTemplate[];
    presets: ServicePreset[];
    startingPointsReady?: boolean;
    recentName?: string;
    tutorialHighlightContinue?: boolean;
  }

  let { open, onClose, onCreate, onCreateFromPlaylist, onDeletePlaylist, playlists = [], presets = [], recentName = "", tutorialHighlightContinue = false, startingPointsReady = false }: Props = $props();

  const LAST_STARTING_POINT_KEY = "makrstudio.view-hub.last-starting-point.v1";
  const FAVORITE_STARTING_POINTS_KEY = "makrstudio.view-hub.favorite-starting-points.v1";

  function readLastStartingPoint(): string | null {
    try {
      const saved = window.localStorage.getItem(LAST_STARTING_POINT_KEY);
      return saved && saved.trim().length > 0 && saved.length <= 200 ? saved : null;
    } catch {
      return null;
    }
  }

  function readFavoriteStartingPoints(): string[] {
    try {
      const saved: unknown = JSON.parse(window.localStorage.getItem(FAVORITE_STARTING_POINTS_KEY) ?? "[]");
      if (!Array.isArray(saved)) return [];
      return [...new Set(saved.filter((id): id is string => typeof id === "string" && id.trim().length > 0 && id.length <= 200))];
    } catch {
      return [];
    }
  }

  const ASPECTS = [
    { id: "16:9", label: "16:9 Widescreen", sub: "Standard displays" },
    { id: "4:3", label: "4:3 Classic", sub: "Legacy projectors" },
    { id: "Vertical", label: "Vertical", sub: "Portrait displays" },
  ];
  // Keep the picker honest: only offer themes that map to an implemented Look.
  const THEMES = ["Dark", "Lower Third"] as const;
  const TRANSITIONS = [
    { id: "cut", label: "Cut" },
    { id: "fade", label: "Fade (400 ms)" },
    { id: "wipe", label: "Wipe (left to right)" },
    { id: "push", label: "Push (left)" },
  ] as const;

  const VIEW_PREFERENCES_KEY = "makrstudio.view-hub.preferences.v1";

  interface ViewPreferences {
    aspect?: string;
    theme?: (typeof THEMES)[number];
    transition?: (typeof TRANSITIONS)[number]["id"];
  }

  function readViewPreferences(): ViewPreferences {
    try {
      const parsed = JSON.parse(window.localStorage.getItem(VIEW_PREFERENCES_KEY) ?? "null") as Partial<ViewPreferences> | null;
      return {
        ...(ASPECTS.some((option) => option.id === parsed?.aspect) ? { aspect: parsed!.aspect } : {}),
        ...(THEMES.includes(parsed?.theme as (typeof THEMES)[number]) ? { theme: parsed!.theme } : {}),
        ...(TRANSITIONS.some((option) => option.id === parsed?.transition) ? { transition: parsed!.transition } : {}),
      };
    } catch {
      return {};
    }
  }

  const initialViewPreferences = readViewPreferences();
  let viewPreferences = $state<ViewPreferences>(initialViewPreferences);

  function rememberPreference<K extends keyof ViewPreferences>(key: K, value: NonNullable<ViewPreferences[K]>): void {
    viewPreferences = { ...viewPreferences, [key]: value };
    try {
      window.localStorage.setItem(VIEW_PREFERENCES_KEY, JSON.stringify(viewPreferences));
    } catch {
      // Keep the choice for this session if local storage is unavailable.
    }
  }

  function chooseAspect(value: string): void {
    aspect = value;
    rememberPreference("aspect", value);
  }

  function chooseTheme(value: (typeof THEMES)[number]): void {
    theme = value;
    rememberPreference("theme", value);
  }

  function chooseTransition(value: (typeof TRANSITIONS)[number]["id"]): void {
    transition = value;
    rememberPreference("transition", value);
  }

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
    ...playlists.map((pt: PlaylistTemplate) => ({
      kind: "playlist" as const,
      id: pt.id,
      name: pt.name,
      category: "Saved",
      description: `Saved reusable playlist — load to start a service.\nSaved ${new Date(pt.createdAt).toLocaleDateString()}.`,
      slideCount: pt.items.length,
      defaultAspect: "16:9",
      playlistId: pt.id,
    })),
    ...presets.map((p: ServicePreset) => ({
      kind: "preset" as const,
      id: p.id,
      name: p.name,
      category: p.category,
      description: p.description,
      slideCount: p.playlistItems.length,
      defaultAspect: p.defaultAspect,
      presetId: p.id,
    })),
  ]);

  const hasSaved = $derived(playlists.length > 0);
  let galleryQuery = $state("");
  let favoritesOnly = $state(false);
  let favoriteStartingPointIds = $state(readFavoriteStartingPoints());
  const favoriteStartingPointSet = $derived(new Set(favoriteStartingPointIds));
  const favoriteCount = $derived(gallery.filter((item) => favoriteStartingPointSet.has(item.id)).length);
  const visibleGallery = $derived.by(() => {
    const query = galleryQuery.trim().toLocaleLowerCase();
    const ordered = [
      ...gallery.filter((item) => favoriteStartingPointSet.has(item.id)),
      ...gallery.filter((item) => !favoriteStartingPointSet.has(item.id)),
    ];
    return ordered.filter((item) => {
      if (favoritesOnly && !favoriteStartingPointSet.has(item.id)) return false;
      return !query || `${item.name} ${item.category} ${item.description}`.toLocaleLowerCase().includes(query);
    });
  });

  function toggleFavoriteStartingPoint(id: string): void {
    favoriteStartingPointIds = favoriteStartingPointSet.has(id)
      ? favoriteStartingPointIds.filter((favoriteId) => favoriteId !== id)
      : [...favoriteStartingPointIds, id];
    try {
      window.localStorage.setItem(FAVORITE_STARTING_POINTS_KEY, JSON.stringify(favoriteStartingPointIds));
    } catch {
      // Keep favorites for this session if local storage is unavailable.
    }
  }

  const initialStartingPointId = readLastStartingPoint();
  let lastStartingPointId = $state(initialStartingPointId);
  let selectedId = $state<string>(initialStartingPointId ?? "sunday-morning");
  const selected = $derived(gallery.find((g) => g.id === selectedId) ?? (lastStartingPointId && !startingPointsReady ? null : gallery[0] ?? null));

  let title = $state("");
  let aspect = $state(initialViewPreferences.aspect ?? "16:9");
  let theme = $state(initialViewPreferences.theme ?? "Dark");
  let transition = $state(initialViewPreferences.transition ?? "cut");
  let creating = $state(false);
  let lastSelectedId: string | null = null;
  let lastAutoTitle = "";
  let wasOpen = false;

  // Auto-name for a new selection, but preserve a title the operator has edited.
  $effect(() => {
    if (!open) {
      wasOpen = false;
      return;
    }
    if (!wasOpen) {
      wasOpen = true;
      lastSelectedId = null;
      lastAutoTitle = "";
      title = "";
    }
    if (!selected) return;
    if (!gallery.some((item) => item.id === selectedId)) {
      selectedId = selected.id;
      if (lastStartingPointId) chooseStartingPoint(selected.id);
    }
    if (selected.id !== lastSelectedId) {
      if (!title.trim() || title === lastAutoTitle) {
        title = `${selected.name} - ${todayLabel()}`;
        lastAutoTitle = title;
      }
      lastSelectedId = selected.id;
    }
    aspect = viewPreferences.aspect ?? selected.defaultAspect;
  });

  async function create(): Promise<void> {
    if (!selected || creating) return;
    const t = title.trim() || `${selected.name} - ${todayLabel()}`;
    const opts = { title: t, aspect, theme, transition };
    creating = true;
    try {
      if (selected.kind === "playlist" && selected.playlistId) {
        await onCreateFromPlaylist(selected.playlistId, opts);
      } else if (selected.presetId) {
        await onCreate(selected.presetId, opts);
      }
    } finally {
      creating = false;
    }
  }

  function close(): void {
    if (!creating) onClose();
  }

  function chooseStartingPoint(id: string): void {
    selectedId = id;
    lastStartingPointId = id;
    try {
      window.localStorage.setItem(LAST_STARTING_POINT_KEY, id);
    } catch {
      // Keep the selection for this session when local storage is unavailable.
    }
  }

  function deletePlaylist(id: string): void {
    if (favoriteStartingPointSet.has(id)) toggleFavoriteStartingPoint(id);
    if (lastStartingPointId === id) {
      const replacement = gallery.find((item) => item.id !== id);
      lastStartingPointId = replacement?.id ?? null;
      selectedId = replacement?.id ?? "";
      try {
        if (replacement) window.localStorage.setItem(LAST_STARTING_POINT_KEY, replacement.id);
        else window.localStorage.removeItem(LAST_STARTING_POINT_KEY);
      } catch {
        // The deleted choice is cleared for this session even when storage is unavailable.
      }
    }
    onDeletePlaylist(id);
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === "Escape" && !creating) onClose();
  }
</script>

{#if open}
  <div class="hub-overlay" role="presentation" transition:veil={{ duration: 200 }} onkeydown={onKey}>
    <div class="hub" role="dialog" aria-modal="true" aria-busy={creating} aria-label="View Hub" tabindex="-1" in:surface={{ duration: 300, distance: 16 }}>
      <header class="hub-head">
        <div class="brand">
          <BrandLockup />
          <div>
            <h1>View Hub</h1>
            <p>Start a new View — choose a starting Playlist, title, slide ratio &amp; Look</p>
          </div>
        </div>
        <button class="close" onclick={close} disabled={creating} aria-label="Close">&times;</button>
      </header>

      <div class="hub-body">
        <!-- Left & Center: Playlist Gallery (presets + saved playlists, unified) -->
        <section class="gallery" aria-label="Starting Playlists">
          <div class="gallery-head">
            <h2>Starting Playlist</h2>
            {#if recentName}
              <span class="recent-hint">Current View: {recentName}</span>
            {/if}
          </div>
          {#if hasSaved}
            <p class="gallery-note">Built-in starting points below; your saved Playlists appear at the top of the list.</p>
          {/if}
          {#if gallery.length > 5 || favoriteCount > 0}
            <div class="gallery-tools">
              {#if gallery.length > 5}
                <label class="gallery-search">
                  <span class="sr-only">Filter starting playlists</span>
                  <input type="search" bind:value={galleryQuery} placeholder="Find a starting playlist" autocomplete="off" disabled={creating} />
                  {#if galleryQuery.trim()}<span class="search-count" aria-live="polite">{visibleGallery.length} of {gallery.length}</span>{/if}
                </label>
              {/if}
              <button class="favorites-filter" class:active={favoritesOnly} aria-pressed={favoritesOnly} onclick={() => favoritesOnly = !favoritesOnly} disabled={creating}>
                <span aria-hidden="true">★</span> Favorites <span class="favorite-count">{favoriteCount}</span>
              </button>
            </div>
          {/if}
          <div class="grid">
            {#each visibleGallery as item (item.kind + ":" + item.id)}
              <div class="card-wrap">
                <button
                  class="card"
                  class:selected={item.id === selectedId}
                  data-kind={item.kind}
                  onclick={() => chooseStartingPoint(item.id)}
                  aria-pressed={item.id === selectedId}
                  disabled={creating}
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
                  {#if item.id === lastStartingPointId}
                    <span class="last-starting-tag" in:surface={{ duration: 160, distance: 3, scale: 0.99 }}>Last selected</span>
                  {/if}
                  <span class="card-desc">{item.description}</span>
                  <span class="card-meta">{item.slideCount} slides · {item.defaultAspect}</span>
                </button>
                <button
                  class="favorite-starting-point"
                  class:favorited={favoriteStartingPointSet.has(item.id)}
                  aria-label={favoriteStartingPointSet.has(item.id) ? `Remove ${item.name} from favorites` : `Add ${item.name} to favorites`}
                  aria-pressed={favoriteStartingPointSet.has(item.id)}
                  title={favoriteStartingPointSet.has(item.id) ? "Remove from favorites" : "Add to favorites"}
                  onclick={() => toggleFavoriteStartingPoint(item.id)}
                  disabled={creating}
                ><span aria-hidden="true">{favoriteStartingPointSet.has(item.id) ? "★" : "☆"}</span></button>
                {#if item.kind === "playlist" && item.playlistId}
                  <button
                    class="delete-saved"
                    aria-label={`Delete saved playlist ${item.name}`}
                    title="Delete saved playlist"
                    onclick={(e) => { e.stopPropagation(); deletePlaylist(item.playlistId!); }}
                    disabled={creating}
                  >×</button>
                {/if}
              </div>
            {/each}
            {#if gallery.length > 0 && visibleGallery.length === 0}
              <div class="empty-search">
                <p class="muted">{favoritesOnly && favoriteCount === 0 ? "No favorites yet. Star a starting playlist to keep it here." : favoritesOnly ? "No favorite starting playlists match that search." : `No starting playlists match “${galleryQuery.trim()}”.`}</p>
                {#if favoritesOnly}<button class="show-all" onclick={() => { favoritesOnly = false; galleryQuery = ""; }}>Show all starting playlists</button>{/if}
              </div>
            {:else if gallery.length === 0}
              <p class="muted">Loading starting playlists…</p>
            {/if}
          </div>

          <div class="recent">
            <h3>Current View</h3>
            {#if recentName}
              <div class="recent-item">
                <span class="dot"></span>
                <div class="recent-copy">
                  <strong>{recentName}</strong>
                  <span class="muted">Autosaved here. Starting a new View replaces this workspace.</span>
                </div>
              </div>
              <button class="resume-current" class:tutorial-highlight={tutorialHighlightContinue} onclick={close} disabled={creating} title="Return to this autosaved View without creating a new one" aria-label={`Continue editing ${recentName}`}>
                <span>Continue this View</span><span aria-hidden="true">→</span>
              </button>
            {:else}
              <span class="muted">No recent View yet — your new service will appear here next launch.</span>
            {/if}
          </div>
        </section>

        <!-- Right: Inspector -->
        <aside class="inspector" aria-label="View Configuration">
          <div class="inspector-head">
            <h3>{selected?.name ?? "Choose a starting playlist"}</h3>
            <p>{selected?.description ?? "Starting playlists are loading."}</p>
          </div>

          <label class="field">
            <span>View Title / Date</span>
            <input type="text" value={title} placeholder="Sunday Morning - {todayLabel()}" oninput={(e)=> title=(e.target as HTMLInputElement).value} disabled={creating} />
            <span class="help">Auto-naming: e.g. “Sunday Morning - Sept 6, 2026”</span>
          </label>

          <div class="field">
            <span>Slide aspect ratio</span>
            <div class="toggles">
              {#each ASPECTS as a}
                <button class:selected={aspect===a.id} aria-pressed={aspect===a.id} onclick={()=> chooseAspect(a.id)} disabled={creating}>
                  <strong>{a.label}</strong><span>{a.sub}</span>
                </button>
              {/each}
            </div>
          </div>

          <label class="field">
            <span>Primary Theme / Look</span>
            <select value={theme} onchange={(e)=> chooseTheme((e.target as HTMLSelectElement).value as (typeof THEMES)[number])} disabled={creating}>
              {#each THEMES as t}<option value={t}>{t}</option>{/each}
            </select>
          </label>

          <label class="field">
            <span>Default Transition</span>
            <select value={transition} onchange={(e)=> chooseTransition((e.target as HTMLSelectElement).value as (typeof TRANSITIONS)[number]["id"])} disabled={creating}>
              {#each TRANSITIONS as t}<option value={t.id}>{t.label}</option>{/each}
            </select>
            <span class="help">Your aspect, Look, and transition are remembered on this device.</span>
          </label>

          {#if creating}<p class="creation-status" role="status">Setting up your View. Please wait…</p>{/if}
          <div class="inspector-actions">
            <button class="ghost" onclick={close} disabled={creating}>Cancel</button>
            <button class="primary" onclick={create} disabled={!selected || creating}>
              {#if creating}<span class="spinner" aria-hidden="true"></span>Creating View…{:else}Create View — {selected?.slideCount ?? 0} slides{/if}
            </button>
          </div>
        </aside>
      </div>
    </div>
  </div>
{/if}

<style>
  .hub-overlay{position:fixed; inset:0; z-index:70; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,0.62);}
  .hub{width:min(1120px,96vw); max-height:92vh; background:var(--panel); border:1px solid var(--border); border-radius:12px; overflow:hidden; display:flex; flex-direction:column; box-shadow:0 20px 60px rgba(0,0,0,0.48);}
  .hub-head{display:flex; align-items:center; justify-content:space-between; padding:16px 20px; border-bottom:1px solid var(--border); background:var(--panel);}
  .brand{display:flex; gap:20px; align-items:center; flex-wrap:wrap; min-width:0; color:var(--text);}
  .brand h1{margin:0; font-family:var(--font-body); font-size:16px; font-weight:650; letter-spacing:-0.01em; color:var(--text);}
  .brand p{margin:2px 0 0; font-size:11px; color:var(--text-dim);}
  .close{width:32px; height:32px; border-radius:8px; background:var(--panel-2); border:1px solid var(--border); color:var(--text-dim); font-size:18px;}
  .close:hover{background:var(--panel); color:var(--text);}
  .hub-body{display:grid; grid-template-columns: minmax(0,1fr) 340px; grid-template-rows:minmax(0,1fr); gap:0; min-height:0; flex:1; overflow:hidden;}
  @media (max-width:900px){ .hub-body{grid-template-columns:minmax(0,1fr); grid-template-rows:minmax(0,1fr) minmax(230px,42vh);} .inspector{border-left:none; border-top:1px solid var(--border);} }
  .gallery{padding:18px; overflow:auto; background: var(--panel-2); min-height:0;}
  .gallery-head{display:flex; align-items:baseline; justify-content:space-between; margin-bottom:10px;}
  .gallery-head h2{margin:0; font-size:11px; font-weight:700; letter-spacing:0.08em; text-transform:uppercase; color:var(--text-dim);}
  .gallery-note{font-size:11px; color:var(--text-dim); margin:-4px 0 10px;}
  .gallery-tools{display:flex; align-items:center; gap:8px; margin:0 0 10px; flex-wrap:wrap;}
  .gallery-search{display:flex; align-items:center; gap:10px; flex:1 1 220px; min-width:0; margin:0;}
  .gallery-search input{flex:1; min-width:0; padding:8px 10px; border:1px solid var(--border); border-radius:8px; background:var(--panel); color:var(--text); font-size:12px;}
  .gallery-search input:focus-visible{outline:2px solid var(--accent); outline-offset:2px;}
  .search-count{flex:none; color:var(--text-dim); font-size:10px; font-variant-numeric:tabular-nums;}
  .favorites-filter{display:flex; align-items:center; justify-content:center; gap:5px; min-height:34px; padding:6px 10px; border-radius:8px; color:var(--text-dim); font-size:11px; white-space:nowrap;}
  .favorites-filter:hover,.favorites-filter.active{border-color:color-mix(in srgb,var(--accent) 55%,var(--border)); color:var(--text); background:color-mix(in srgb,var(--accent) 10%,var(--panel));}
  .favorites-filter>span:first-child{color:var(--accent);}
  .favorite-count{min-width:16px; padding:1px 5px; border-radius:999px; background:var(--panel); color:var(--text-dim); font-size:10px; font-variant-numeric:tabular-nums;}
  .empty-search{grid-column:1/-1; display:grid; justify-items:start; gap:6px; margin:10px 0;}
  .show-all{padding:5px 8px; color:var(--accent); font-size:11px;}
  .sr-only{position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; border:0;}
  .recent-hint{font-size:11px; color:var(--text-dim);}
  .grid{display:grid; grid-template-columns: repeat(2, minmax(0,1fr)); gap:12px;}
  @media (max-width:640px){ .grid{grid-template-columns:1fr;} }
  .card-wrap{position:relative; min-width:0;}
  .card{position:relative; width:100%; text-align:left; padding:14px; border-radius:9px; border:1px solid var(--border); background:var(--panel); color:var(--text); min-height:140px; display:flex; flex-direction:column; gap:6px; overflow:hidden; transition:background-color var(--motion-normal) var(--ease-out),border-color var(--motion-normal) var(--ease-out),box-shadow var(--motion-normal) var(--ease-out),transform var(--motion-fast) var(--ease-out);}
  .card:hover{border-color:#46514e; background:var(--panel); transform:translateY(-1px);}
  .card.selected{border-color:var(--accent); background:#202825; box-shadow:0 0 0 2px rgba(129,170,149,0.12);}
  .card-badge{position:absolute; top:10px; right:42px; font-size:9px; font-weight:700; letter-spacing:0.06em; text-transform:uppercase; padding:3px 7px; border-radius:999px; border:1px solid transparent; color:white;}
  .card-badge[data-category="Sunday Service"]{background: var(--color-green); border-color: var(--color-green);}
  .card-badge[data-category="Midweek"]{background: #1e3a4d; border-color: #234a5e; color: white;}
  .card-badge[data-category="Youth"]{background: var(--brand-orange-500); border-color: var(--brand-orange-500); color:white;}
  .card-badge[data-category="Custom"]{background: var(--panel-2); border-color: var(--border); color: var(--text-dim);}
  .card-badge[data-kind="playlist"]{background: var(--accent); border-color: var(--accent); color:white;}
  .card-icon{font-size:20px; margin-top:6px; opacity:0.8;}
  .card-title{font-family:var(--font-display); font-size:13px; font-weight:700; line-height:1.2; color:var(--text);}
  .last-starting-tag{align-self:flex-start; padding:3px 7px; border:1px solid color-mix(in srgb,var(--accent) 38%,var(--border)); border-radius:999px; background:color-mix(in srgb,var(--accent) 8%,var(--panel)); color:var(--accent); font-size:9px; font-weight:650; line-height:1.2;}
  .card-desc{font-size:11px; color:var(--text-dim); line-height:1.35; white-space:pre-line;}
  .card-meta{margin-top:auto; padding-right:28px; font-size:10px; color:var(--text-dim);}
  .favorite-starting-point{position:absolute; top:8px; right:8px; z-index:2; display:grid; place-items:center; width:28px; height:28px; padding:0; border-radius:8px; background:var(--panel-2); color:var(--text-dim); font-size:16px; line-height:1; transition:color var(--motion-fast) var(--ease-out),border-color var(--motion-fast) var(--ease-out),background-color var(--motion-fast) var(--ease-out),transform var(--motion-fast) var(--ease-out);}
  .favorite-starting-point:hover{border-color:var(--accent); color:var(--text); transform:scale(1.04);}
  .favorite-starting-point.favorited{color:var(--accent); border-color:color-mix(in srgb,var(--accent) 45%,var(--border));}
  .favorite-starting-point.favorited span{animation:favorite-pop 220ms var(--ease-out) both;}
  @keyframes favorite-pop{50%{transform:scale(1.3)}}
  .delete-saved{position:absolute; right:8px; bottom:8px; z-index:2; width:24px; height:24px; padding:0; border-radius:6px; color:var(--text-dim);}
  .delete-saved:hover{border-color:var(--color-burgundy); background:var(--danger-bg); color:var(--danger-text);}
  .recent{display:flex; flex-direction:column; gap:9px; margin-top:16px; padding-top:12px; border-top:1px solid var(--border);}
  .recent h3{margin:0; font-size:11px; font-weight:700; letter-spacing:0.06em; text-transform:uppercase; color:var(--text-dim);}
  .recent-item{display:flex; gap:9px; align-items:flex-start; font-size:12px; color:var(--text);}
  .recent-copy{display:grid; gap:3px; min-width:0;}
  .recent-copy strong{overflow-wrap:anywhere; font-size:12px;}
  .resume-current{display:flex; align-items:center; justify-content:space-between; gap:12px; width:100%; padding:10px 12px; border:1px solid color-mix(in srgb,var(--accent) 38%,var(--border)); border-radius:8px; background:color-mix(in srgb,var(--accent) 8%,var(--panel)); color:var(--accent); font-size:12px; font-weight:650; text-align:left; cursor:pointer; transition:background var(--motion-fast) var(--ease-out),border-color var(--motion-fast) var(--ease-out),transform var(--motion-fast) var(--ease-out);}
  .resume-current:hover{border-color:var(--accent); background:color-mix(in srgb,var(--accent) 13%,var(--panel)); transform:translateY(-1px);}
  .resume-current.tutorial-highlight{outline:2px solid var(--accent); outline-offset:2px; animation:resume-pulse 1.2s var(--ease-out) 2;}
  @keyframes resume-pulse{50%{box-shadow:0 0 0 5px color-mix(in srgb,var(--accent) 18%,transparent);}}
  @media (prefers-reduced-motion: reduce){.resume-current{transition:none}.resume-current.tutorial-highlight{animation:none}}
  .dot{width:8px; height:8px; border-radius:999px; background:var(--accent);}
  .muted{font-size:11px; color:var(--text-dim);}
  .inspector{padding:16px; overflow:auto; background:var(--panel); border-left:1px solid var(--border); display:flex; flex-direction:column; gap:14px; min-height:0;}
  .inspector-head h3{margin:0; font-size:13px; font-weight:700; color:var(--text);}
  .inspector-head p{margin:4px 0 0; font-size:11px; color:var(--text-dim); line-height:1.4; white-space:pre-line;}
  .field{display:flex; flex-direction:column; gap:6px; font-size:11px; color:var(--text-dim);}
  .field input, .field select{width:100%; background:var(--panel-2); border:1px solid var(--border); border-radius:6px; padding:8px 10px; color:var(--text); font-size:13px;}
  .field input:focus, .field select:focus{outline:none; border-color:var(--accent); box-shadow:0 0 0 2px rgba(129,170,149,0.14);}
  .help{font-size:10px; color:var(--text-dim);}
  .toggles{display:grid; grid-template-columns:1fr 1fr; gap:6px;}
  .toggles button{display:flex; flex-direction:column; gap:2px; padding:8px 10px; border-radius:8px; border:1px solid var(--border); background:var(--panel-2); text-align:left; font-size:11px; color:var(--text);}
  .toggles button.selected{border-color:var(--accent); background:rgba(129,170,149,0.12); color:var(--text);}
  .toggles button strong{font-size:11px;}
  .toggles button span{font-size:10px; color:var(--text-dim);}
  .inspector-actions{display:flex; gap:8px; margin-top:auto; padding-top:6px;}
  button{border:1px solid var(--border); border-radius:6px; background:var(--panel-2); color:var(--text); padding:7px 12px; font-size:13px; font-weight:500;}
  button.ghost{background:transparent;}
  button.primary{background:#456b58; border-color:#527962; color:#f4faf6; flex:1;}
  button.primary:hover{background:#527962;}
  button.primary{display:flex; align-items:center; justify-content:center; gap:8px; min-height:38px;}
  .spinner{width:14px; height:14px; flex:none; border:2px solid rgba(244,250,246,.38); border-top-color:#f4faf6; border-radius:50%; animation:spin 700ms linear infinite;}
  .creation-status{margin:-7px 0 0; color:var(--text-dim); font-size:11px; text-align:center;}
  @keyframes spin{to{transform:rotate(360deg)}}
  button:focus-visible, .field input:focus-visible, .field select:focus-visible{outline:2px solid var(--accent); outline-offset:2px;}
  button:disabled{opacity:.55; cursor:not-allowed;}
  @media (max-width:640px){
    .hub{width:100vw; height:100dvh; max-height:100dvh; border-radius:0;}
    .hub-head{padding:12px 14px; gap:10px;}
    .brand{gap:10px;}
    .brand p{max-width:48ch; line-height:1.35;}
    .gallery{padding:14px;}
    .gallery-head{align-items:flex-start; gap:8px; flex-direction:column;}
    .grid{grid-template-columns:repeat(2,minmax(0,1fr)); gap:8px;}
    .card{min-height:132px; padding:11px;}
    .card-badge{top:8px; right:42px;}
    .inspector{padding:12px 14px; gap:10px;}
    .toggles{grid-template-columns:repeat(3,minmax(0,1fr));}
    .toggles button{padding:7px;}
    .inspector-actions{position:sticky; bottom:-12px; padding:10px 0 12px; background:var(--panel);}
  }
  @media (max-width:380px){
    .grid{grid-template-columns:1fr;}
    .card{min-height:112px;}
    .toggles{grid-template-columns:1fr 1fr;}
  }
  @media (prefers-reduced-motion:reduce){.card,.favorite-starting-point{transition:none}.card:hover,.favorite-starting-point:hover{transform:none}.favorite-starting-point.favorited span{animation:none}.spinner{animation:none;border-top-color:rgba(244,250,246,.55)}}
</style>

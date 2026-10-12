<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { api } from "../lib/sync";
  import { addCachedMediaSlide } from "../lib/mediaSlide";
  import type { BibleInfo, ChapterVerse, Library, LyricsHit, MediaAsset, ScriptureMatch } from "../lib/types";
  import { isMedia } from "../lib/types";
  import { surface, veil } from "../lib/motion";
  import { withTimeout } from "../lib/withTimeout";
  import { indexSongs, rankSongs } from "../lib/songSearch";

  interface Props {
    open: boolean;
    library: Library | null;
    onClose: () => void;
    onImportLyrics: (hit: LyricsHit) => void;
  }

  let { open, library, onClose, onImportLyrics }: Props = $props();

  let query = $state("");
  const HISTORY_KEY = "makrstudio.search.recent.v1";
  function readRecentQueries(): string[] {
    try {
      const saved: unknown = JSON.parse(localStorage.getItem(HISTORY_KEY) ?? "[]");
      if (!Array.isArray(saved)) return [];
      return [...new Set(saved.filter((item): item is string => typeof item === "string")
        .map((item) => item.trim().slice(0, 120)).filter(Boolean))].slice(0, 8);
    } catch { return []; }
  }
  let recentQueries = $state<string[]>(readRecentQueries());
  let historyStatus = $state<string | null>(null);
  function saveRecentQueries(): void {
    try {
      localStorage.setItem(HISTORY_KEY, JSON.stringify(recentQueries));
      historyStatus = null;
    } catch { historyStatus = "Search history is available for this session, but could not be saved on this device."; }
  }
  function rememberQuery(value: string): void {
    const term = value.trim().slice(0, 120);
    if (!term) return;
    recentQueries = [term, ...recentQueries.filter((item) => item.toLocaleLowerCase() !== term.toLocaleLowerCase())].slice(0, 8);
    saveRecentQueries();
  }
  function useQuery(value: string): void {
    if (debounce) clearTimeout(debounce);
    debounce = null;
    query = value;
    scriptureResults = [];
    mediaResults = [];
    onlineSongs = [];
    onlineError = null;
    onlineLoading = false;
    void doSearch(value);
    inputEl?.focus();
  }
  let inputEl = $state<HTMLInputElement | null>(null);
  let scriptureResults = $state<ScriptureMatch[]>([]);
  let scriptureTranslation = $state("KJV");
  let mediaResults = $state<MediaAsset[]>([]);
  let onlineSongs = $state<LyricsHit[]>([]);
  let onlineError = $state<string | null>(null);
  let onlineLoading = $state(false);
  let loading = $state(false);
  let errorMsg = $state<string | null>(null);
  let inserting = $state<string | null>(null);
  let seq = 0;
  let paletteGeneration = 0;
  let scriptureBiblePromise: Promise<BibleInfo | null> | null = null;

  function getSearchBible(): Promise<BibleInfo | null> {
    if (!scriptureBiblePromise) {
      const pending = withTimeout(api.listBibles(), 15000, "Bible list did not respond.").then((bibles) => {
        let rememberedId: string | null = null;
        try { rememberedId = window.localStorage.getItem("makrstudio.scripture-bible-id"); } catch { /* use the first available Bible */ }
        return bibles.find((bible) => bible.id === rememberedId) ?? bibles.find((bible) => bible.id === "kjv") ?? bibles[0] ?? null;
      });
      scriptureBiblePromise = pending;
      void pending.catch(() => {
        if (scriptureBiblePromise === pending) scriptureBiblePromise = null;
      });
    }
    return scriptureBiblePromise;
  }

  async function labelMatchesForBible(matches: ScriptureMatch[], request: number): Promise<ScriptureMatch[]> {
    if (!matches.length) return [];
    const bible = await getSearchBible();
    if (request !== seq) return [];
    const translation = bible?.shortName?.trim() || bible?.name?.trim() || "KJV";
    scriptureTranslation = translation;
    if (!bible) return matches.map((match) => ({ ...match, translation }));
    const chapters = new Map<string, Promise<ChapterVerse[]>>();
    for (const match of matches) {
      const key = `${match.book}:${match.chapter}`;
      if (!chapters.has(key)) chapters.set(key, withTimeout(api.getChapter(bible.id, match.book, match.chapter), 15000, "The selected Bible chapter could not be loaded. Retry search or choose another translation in Scripture."));
    }
    const loaded = new Map<string, ChapterVerse[]>();
    await Promise.all([...chapters].map(async ([key, promise]) => loaded.set(key, await promise)));
    return matches.flatMap((match) => {
      const verse = loaded.get(`${match.book}:${match.chapter}`)?.find((item) => item.verse === match.verse);
      return verse ? [{ ...match, text: verse.text, translation }] : [];
    });
  }

  const songIndex = $derived(indexSongs(library?.songs ?? []));
  const libraryResults = $derived(rankSongs(songIndex, query));

  function songVerseCount(s: any): number {
    return s.arrangement?.length ?? (s.blocks ? Object.keys(s.blocks).length : (s.slides?.length ?? 0));
  }
  function songBlockTitles(s: any): string {
    const arr: string[] = s.arrangement ?? (s.blocks ? Object.keys(s.blocks) : (s.slides?.map((v: any) => v.title) ?? []));
    return arr.slice(0, 3).join(", ") + (arr.length > 3 ? "…" : "");
  }

  // Focus when opened
  $effect(() => {
    ++paletteGeneration;
    let focusFrame = 0;
    if (open) {
      query = "";
      scriptureResults = [];
      mediaResults = [];
      onlineSongs = [];
      onlineError = null;
      onlineLoading = false;
      errorMsg = null;
      loading = false;
      scriptureBiblePromise = null;
      seq++;
      focusFrame = requestAnimationFrame(() => {
        inputEl?.focus();
        inputEl?.select();
      });
    } else {
      seq++;
      if (debounce) {
        clearTimeout(debounce);
        debounce = null;
      }
    }
    return () => {
      ++paletteGeneration;
      seq++;
      if (debounce) clearTimeout(debounce);
      debounce = null;
      if (focusFrame) cancelAnimationFrame(focusFrame);
    };
  });

  let debounce: ReturnType<typeof setTimeout> | null = null;
  function onInput(e: Event): void {
    const v = (e.target as HTMLInputElement).value;
    query = v;
    seq++;
    errorMsg = null;
    scriptureResults = [];
    mediaResults = [];
    loading = v.trim().length > 0;
    onlineSongs = [];
    onlineError = null;
    onlineLoading = v.trim().length >= 3;
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(() => void doSearch(v), 420);
  }

  async function doSearch(q: string): Promise<void> {
    const cur = ++seq;
    const trimmed = q.trim();
    errorMsg = null;
    onlineError = null;
    onlineLoading = false;
    if (!trimmed) {
      // Empty query: show recent media, clear scripture/media filtered to empty? Keep top media
      scriptureResults = [];
      onlineSongs = [];
      onlineError = null;
      onlineLoading = false;
      try {
        const all = await withTimeout(api.listMedia(), 15000, "Media list did not respond.");
        if (cur !== seq) return;
        mediaResults = all.slice(0, 6);
      } catch {
        if (cur === seq) mediaResults = [];
      }
      if (cur === seq) loading = false;
      return;
    }
    loading = true;
    errorMsg = null;
    // Local library is immediate. Keep online results optional so a service
    // outage never interferes with Scripture or cached media search.
    if (trimmed.length >= 3) {
      onlineLoading = true;
      void withTimeout(api.searchLyrics(trimmed), 15000, "Online lyrics search timed out. Try again.")
        .then((found) => { if (cur === seq) onlineSongs = found.filter((hit) => hit.hasLyrics).slice(0, 6); })
        .catch((e: unknown) => { if (cur === seq) onlineError = String(e); })
        .finally(() => { if (cur === seq) onlineLoading = false; });
    }
    const [scr, med] = await Promise.allSettled([
      withTimeout(api.searchScripture(trimmed), 15000, "Scripture search timed out. Try again."),
      withTimeout(api.searchMedia(trimmed), 15000, "Media search timed out. Try again."),
    ]);
    if (cur !== seq) return;
    if (med.status === "fulfilled") mediaResults = med.value.slice(0, 8);
    else mediaResults = [];
    if (scr.status === "fulfilled") {
      try {
        const labelled = await labelMatchesForBible(scr.value.slice(0, 8), cur);
        if (cur !== seq) return;
        scriptureResults = labelled;
      } catch (error) {
        if (cur !== seq) return;
        scriptureResults = [];
        errorMsg = `Could not read Scripture from the selected Bible: ${String(error)}`;
      }
    } else scriptureResults = [];
    const failures = [scr, med].flatMap((result) => result.status === "rejected" ? [String(result.reason)] : []);
    if (failures.length) errorMsg = [errorMsg, ...failures].filter(Boolean).join(" ");
    loading = false;
  }

  function fileUrl(path: string): string {
    if (!path) return "";
    try {
      return convertFileSrc(path);
    } catch {
      return "";
    }
  }

  async function insertSong(songId: string): Promise<void> {
    if (inserting) return;
    const session = paletteGeneration;
    const term = query;
    inserting = `song-${songId}`;
    try {
      await api.addSongToPlaylist(songId);
      rememberQuery(term);
      if (open && session === paletteGeneration && query === term) onClose();
    } catch (e) {
      if (open && session === paletteGeneration && query === term) errorMsg = String(e);
    } finally {
      inserting = null;
    }
  }

  async function insertScripture(m: ScriptureMatch): Promise<void> {
    if (inserting) return;
    const session = paletteGeneration;
    const term = query;
    inserting = `scr-${m.reference}`;
    try {
      const translation = m.translation?.trim() || "KJV";
      await api.addSlide(`${m.reference} · ${translation}`, m.text, undefined, "scripture");
      rememberQuery(term);
      if (open && session === paletteGeneration && query === term) onClose();
    } catch (e) {
      if (open && session === paletteGeneration && query === term) errorMsg = String(e);
    } finally {
      inserting = null;
    }
  }

  async function insertMedia(asset: MediaAsset): Promise<void> {
    if (inserting) return;
    const session = paletteGeneration;
    const term = query;
    inserting = `media-${asset.hash}`;
    try {
      await addCachedMediaSlide(asset);
      rememberQuery(term);
      if (open && session === paletteGeneration && query === term) onClose();
    } catch (e) {
      if (open && session === paletteGeneration && query === term) errorMsg = String(e);
    } finally {
      inserting = null;
    }
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  function onBackdropClick(): void {
    onClose();
  }
</script>

{#if open}
  <div class="overlay" role="presentation" transition:veil={{ duration: 170 }}>
    <button class="backdrop" aria-label="Close search" tabindex="-1" onclick={onBackdropClick}></button>
    <div
      class="palette"
      role="dialog"
      aria-modal="true"
      aria-label="Global search"
      tabindex="-1"
      in:surface={{ duration: 260, distance: 10 }}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => { e.stopPropagation(); onKeydown(e); }}
    >
      <header class="palette-head">
        <div class="search-row">
          <span class="search-icon" aria-hidden="true">⌕</span>
          <input
            bind:this={inputEl}
            type="text"
            placeholder="Search songs, scriptures, media…"
            aria-label="Search songs, Scripture, and media"
            maxlength="120"
            value={query}
            oninput={onInput}
          />
          {#if query}<button class="clear-query" aria-label="Clear search" onclick={() => useQuery("")}>Clear</button>{/if}
          <span class="hint">Ctrl+K</span>
        </div>
        <button class="close" aria-label="Close" onclick={onClose}>&times;</button>
      </header>

      {#if errorMsg}
        <div class="palette-error" role="alert">{errorMsg}
          {#if query.trim()}<button disabled={loading} onclick={() => void doSearch(query)}>Retry search</button>{/if}
        </div>
      {/if}

      <div class="palette-body">
        {#if !query.trim() && recentQueries.length}
          <section class="category recent-searches" aria-label="Recent searches">
            <h3>Recent searches <button onclick={() => { recentQueries = []; saveRecentQueries(); }}>Clear history</button></h3>
            <div>{#each recentQueries as term (term)}<button onclick={() => useQuery(term)}>{term}</button>{/each}</div>
            <p class="empty">Stored on this device. Choose a search to run it again.</p>
          </section>
        {/if}
        {#if historyStatus}<p class="empty" role="status">{historyStatus}</p>{/if}
        {#if !query.trim() && libraryResults.length === 0 && scriptureResults.length === 0 && mediaResults.length === 0 && !loading}
          <p class="empty-hint">Search your songs, online lyrics, Bibles, and media. Choose an online song to review its slides before adding it.</p>
        {/if}

        {#if loading}
          <p class="loading-hint" role="status"><span class="mini-spinner" aria-hidden="true"></span> Searching…</p>
        {/if}

        <!-- Songs / Library -->
        <section class="category">
          <h3>Songs — Library <span class="count">{libraryResults.length}</span></h3>
          {#if libraryResults.length === 0}
            <p class="empty">No songs match “{query.trim()}”.</p>
          {:else}
            <ul class="result-list">
              {#each libraryResults as hit (hit.song.id)}
                {@const song = hit.song}
                <li>
                  <button
                    class="result"
                    onclick={() => void insertSong(song.id)}
                    disabled={inserting !== null}
                    title="Click to add whole song to playlist (uses arrangement)"
                  >
                    <span class="result-title">{song.title}</span>
                    {#if query.trim()}<span class="result-meta">{hit.reason}</span>{/if}
                    <span class="result-meta">{songVerseCount(song)} {songVerseCount(song) === 1 ? "verse" : "verses"} • {songBlockTitles(song)}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        <section class="category">
          <h3>Songs — Online lyrics <span class="count">{onlineSongs.length}</span></h3>
          <p class="online-note">Search terms are sent to LRCLIB. Review lyrics before adding them.</p>
          {#if !query.trim() || query.trim().length < 3}
            <p class="empty">Type a song title or artist to find lyrics.</p>
          {:else if onlineError}
            <p class="empty">Online lyrics are unavailable right now. Your library search still works.</p>
          {:else if onlineLoading}
            <p class="empty"><span class="mini-spinner" aria-hidden="true"></span> Finding online lyrics…</p>
          {:else if onlineSongs.length === 0}
            <p class="empty">No online lyrics found. Try adding the artist name.</p>
          {:else}
            <ul class="result-list">
              {#each onlineSongs as hit (hit.id)}
                <li><button class="result online-result" disabled={inserting !== null} onclick={() => { rememberQuery(query); onImportLyrics(hit); }}>
                  <span class="result-title">{hit.title}</span>
                  <span class="result-meta">{hit.artist}{hit.album ? ` · ${hit.album}` : ""}</span>
                  <span class="online-action">Review lyrics and slides →</span>
                </button></li>
              {/each}
            </ul>
          {/if}
        </section>

        <!-- Scripture / Bibles -->
        <section class="category">
          <h3>Scripture — {scriptureTranslation} <span class="count">{scriptureResults.length}</span></h3>
          {#if query.trim() && scriptureResults.length === 0 && !loading}
            <p class="empty">No verses match “{query.trim()}”.</p>
          {:else if !query.trim()}
            <p class="empty">Type a reference or keywords (e.g. “John 3:16”, “psalm 23”, “love”).</p>
          {:else}
            <ul class="result-list">
              {#each scriptureResults as m (m.reference)}
                <li>
                  <button
                    class="result"
                    onclick={() => void insertScripture(m)}
                    disabled={inserting !== null}
                    title="Click to add as slide"
                  >
                    <span class="result-title">{m.reference} · {m.translation || "KJV"}</span>
                    <span class="result-preview">{m.text}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        <!-- Media cache -->
        <section class="category">
          <h3>Media — Cache <span class="count">{mediaResults.length}</span></h3>
          {#if mediaResults.length === 0 && !loading}
            <p class="empty">{query.trim() ? `No media matches “${query.trim()}”.` : "No cached media yet — import via Add media or drag-drop."}</p>
          {:else}
            <ul class="result-list media-list">
              {#each mediaResults as asset (asset.hash)}
                <li>
                  <button
                    class="result media-result"
                    onclick={() => void insertMedia(asset)}
                    disabled={inserting !== null}
                    title="Click to create a new slide with this background"
                  >
                    <span
                      class="media-thumb"
                      style:background-image={isMedia(asset.background)
                        ? `url('${fileUrl(asset.background.thumb)}')`
                        : "none"}
                      aria-hidden="true"
                    ></span>
                    <span class="media-meta">
                      <span class="result-title">{asset.fileName}</span>
                      <span class="result-meta">{asset.kind} • {asset.hash.slice(0, 8)}…</span>
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      </div>

      <footer class="palette-foot">
        <span>↵ Insert into playlist</span>
        <span>Esc Close</span>
        <span>Ctrl+K Reopen</span>
      </footer>
    </div>
  </div>
{/if}

<style>
  .recent-searches h3 { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .recent-searches > div { display: flex; flex-wrap: wrap; gap: 6px; }
  .recent-searches button, .clear-query { font: inherit; font-size: 11px; padding: 5px 8px; }
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 12vh;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.52);
    border: none;
    border-radius: 0;
    cursor: pointer;
  }
  .palette {
    position: relative;
    width: min(720px, 94vw);
    max-height: 78vh;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.04);
    overflow: hidden;
  }
  .palette-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--panel-2);
  }
  .search-row {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 10px;
  }
  .search-icon {
    color: var(--text-dim);
    font-size: 14px;
  }
  .search-row input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text);
    font-size: 14px;
  }
  .search-row input::placeholder {
    color: var(--text-dim);
  }
  .hint {
    font-size: 10px;
    color: var(--text-dim);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 6px;
    background: var(--panel-2);
  }
  .close {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 6px;
    width: 30px;
    height: 30px;
    color: var(--text-dim);
  }
  .close:hover { background: var(--panel); color: var(--text); }
  .palette-error {
    margin: 8px 14px 0;
    padding: 8px 10px;
    background: var(--semantic-error-bg, rgba(225, 29, 72, 0.12));
    border: 1px solid var(--semantic-error, #e11d48);
    border-radius: 6px;
    color: var(--semantic-error, #e11d48);
    font-size: 12px;
  }
  .palette-body {
    flex: 1;
    overflow: auto;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .empty-hint, .loading-hint, .empty {
    color: var(--text-dim);
    font-size: 12px;
    margin: 0;
  }
  .mini-spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 1.5px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 999px;
    animation: spin 700ms linear infinite;
    vertical-align: middle;
    margin-right: 6px;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .category h3 {
    margin: 0 0 8px;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .count {
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 1px 6px;
    font-size: 10px;
  }
  .result-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .result {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 2px;
    text-align: left;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 9px 10px;
  }
  .result:hover { border-color: var(--accent); background: var(--panel); }
  .result { transition: background var(--motion-fast) var(--ease-out), border-color var(--motion-fast) var(--ease-out), box-shadow var(--motion-fast) var(--ease-out); }
  .result:focus-visible { box-shadow: 0 0 0 3px rgba(129,170,149,0.14); }
  .online-result { position: relative; padding-right: 130px; }
  .online-note { margin: -3px 0 8px; color: var(--text-dim); font-size: 10px; line-height: 1.4; }
  .online-action { position: absolute; right: 10px; top: 50%; transform: translateY(-50%); color: var(--accent); font-size: 10px; font-weight: 700; white-space: nowrap; }
  @media (max-width: 560px) { .online-result { padding-right: 10px; } .online-action { position: static; transform: none; margin-top: 4px; } }
  .result:disabled { opacity: 0.6; }
  .result-title { font-size: 13px; font-weight: 600; color: var(--text); }
  .result-meta, .result-preview {
    font-size: 11px;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .result-preview { white-space: normal; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .media-list .media-result {
    flex-direction: row;
    align-items: center;
    gap: 10px;
  }
  .media-thumb {
    width: 44px;
    height: 28px;
    border-radius: 4px;
    background: #000;
    background-size: cover;
    background-position: center;
    flex-shrink: 0;
    border: 1px solid var(--border);
  }
  .media-meta { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .palette-foot {
    display: flex;
    gap: 14px;
    padding: 8px 14px;
    border-top: 1px solid var(--border);
    background: var(--panel-2);
    color: var(--text-dim);
    font-size: 10px;
  }
</style>

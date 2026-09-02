<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { api } from "../lib/sync";
  import type { Library, MediaAsset, ScriptureMatch } from "../lib/types";
  import { isMedia } from "../lib/types";

  interface Props {
    open: boolean;
    library: Library | null;
    onClose: () => void;
  }

  let { open, library, onClose }: Props = $props();

  let query = $state("");
  let inputEl = $state<HTMLInputElement | null>(null);
  let scriptureResults = $state<ScriptureMatch[]>([]);
  let mediaResults = $state<MediaAsset[]>([]);
  let loading = $state(false);
  let errorMsg = $state<string | null>(null);
  let inserting = $state<string | null>(null);
  let seq = 0;

  const libraryResults = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const songs = library?.songs ?? [];
    if (!q) return songs.slice(0, 5);
    return songs
      .filter((s: any) => {
        if (s.title.toLowerCase().includes(q)) return true;
        const blocks: any[] = s.blocks
          ? Object.values(s.blocks)
          : (s.slides ?? []);
        return blocks.some(
          (v: any) =>
            v.title.toLowerCase().includes(q) ||
            v.body.toLowerCase().includes(q),
        );
      })
      .slice(0, 8);
  });

  function songVerseCount(s: any): number {
    return s.arrangement?.length ?? (s.blocks ? Object.keys(s.blocks).length : (s.slides?.length ?? 0));
  }
  function songBlockTitles(s: any): string {
    const arr: string[] = s.arrangement ?? (s.blocks ? Object.keys(s.blocks) : (s.slides?.map((v: any) => v.title) ?? []));
    return arr.slice(0, 3).join(", ") + (arr.length > 3 ? "…" : "");
  }

  // Focus when opened
  $effect(() => {
    if (open) {
      query = "";
      scriptureResults = [];
      mediaResults = [];
      errorMsg = null;
      loading = false;
      seq++;
      requestAnimationFrame(() => {
        inputEl?.focus();
        inputEl?.select();
      });
    }
  });

  let debounce: ReturnType<typeof setTimeout> | null = null;
  function onInput(e: Event): void {
    const v = (e.target as HTMLInputElement).value;
    query = v;
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(() => void doSearch(v), 180);
  }

  async function doSearch(q: string): Promise<void> {
    const cur = ++seq;
    const trimmed = q.trim();
    if (!trimmed) {
      // Empty query: show recent media, clear scripture/media filtered to empty? Keep top media
      scriptureResults = [];
      try {
        const all = await api.listMedia();
        if (cur !== seq) return;
        mediaResults = all.slice(0, 6);
      } catch {
        mediaResults = [];
      }
      loading = false;
      return;
    }
    loading = true;
    errorMsg = null;
    // Parallel: scripture + media (library is client-side)
    const [scr, med] = await Promise.allSettled([
      api.searchScripture(trimmed),
      api.searchMedia(trimmed),
    ]);
    if (cur !== seq) return;
    if (scr.status === "fulfilled") scriptureResults = scr.value.slice(0, 8);
    else scriptureResults = [];
    if (med.status === "fulfilled") mediaResults = med.value.slice(0, 8);
    else mediaResults = [];
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
    inserting = `song-${songId}`;
    try {
      await api.addSongToPlaylist(songId);
      onClose();
    } catch (e) {
      errorMsg = String(e);
    } finally {
      inserting = null;
    }
  }

  async function insertScripture(m: ScriptureMatch): Promise<void> {
    inserting = `scr-${m.reference}`;
    try {
      await api.addSlide(m.reference, m.text);
      onClose();
    } catch (e) {
      errorMsg = String(e);
    } finally {
      inserting = null;
    }
  }

  async function insertMedia(asset: MediaAsset): Promise<void> {
    inserting = `media-${asset.hash}`;
    try {
      const base = asset.fileName.replace(/\.[^/.]+$/, "") || "Media";
      const created = await api.addSlide(base, "");
      const newId = created.project.slides.at(-1)?.id;
      if (!newId) throw new Error("failed to create media slide");
      await api.updateSlide(newId, { background: asset.background });
      onClose();
    } catch (e) {
      errorMsg = String(e);
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
  <div class="overlay" role="presentation">
    <button class="backdrop" aria-label="Close search" tabindex="-1" onclick={onBackdropClick}></button>
    <div
      class="palette"
      role="dialog"
      aria-modal="true"
      aria-label="Global search"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <header class="palette-head">
        <div class="search-row">
          <span class="search-icon" aria-hidden="true">⌕</span>
          <input
            bind:this={inputEl}
            type="text"
            placeholder="Search songs, scriptures, media…"
            value={query}
            oninput={onInput}
            onkeydown={onKeydown}
          />
          <span class="hint">Ctrl+K</span>
        </div>
        <button class="close" aria-label="Close" onclick={onClose}>&times;</button>
      </header>

      {#if errorMsg}
        <div class="palette-error" role="alert">{errorMsg}</div>
      {/if}

      <div class="palette-body">
        {#if !query.trim() && libraryResults.length === 0 && scriptureResults.length === 0 && mediaResults.length === 0 && !loading}
          <p class="empty-hint">Type to search — songs, Bibles (KJV + imported), and media cache. Results insert directly into the playlist.</p>
        {/if}

        {#if loading}
          <p class="loading-hint"><span class="mini-spinner" aria-hidden="true"></span> Searching…</p>
        {/if}

        <!-- Songs / Library -->
        <section class="category">
          <h3>Songs — Library <span class="count">{libraryResults.length}</span></h3>
          {#if libraryResults.length === 0}
            <p class="empty">No songs match “{query.trim()}”.</p>
          {:else}
            <ul class="result-list">
              {#each libraryResults as song (song.id)}
                <li>
                  <button
                    class="result"
                    onclick={() => void insertSong(song.id)}
                    disabled={inserting === `song-${song.id}`}
                    title="Click to add whole song to playlist (uses arrangement)"
                  >
                    <span class="result-title">{song.title}</span>
                    <span class="result-meta">{songVerseCount(song)} {songVerseCount(song) === 1 ? "verse" : "verses"} • {songBlockTitles(song)}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        <!-- Scripture / Bibles -->
        <section class="category">
          <h3>Scripture — All Bibles <span class="count">{scriptureResults.length}</span></h3>
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
                    disabled={inserting === `scr-${m.reference}`}
                    title="Click to add as slide"
                  >
                    <span class="result-title">{m.reference}</span>
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
                    disabled={inserting === `media-${asset.hash}`}
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

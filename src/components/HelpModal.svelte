<script lang="ts">
  import BrandLockup from "./BrandLockup.svelte";
  import { surface } from "../lib/motion";
  interface Props {
    open: boolean;
    onClose: () => void;
    onReplayTour: (step?: number) => void;
    onGettingStarted: () => void;
  }

  let { open, onClose, onReplayTour, onGettingStarted }: Props = $props();
  let dialog = $state<HTMLDialogElement>();

  $effect(() => {
    if (!open || !dialog) return;
    const previous = document.activeElement;
    const element = dialog;
    element.showModal();
    return () => {
      element.close();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  });

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape" || (e.key === "?" && !e.ctrlKey && !e.metaKey && !e.altKey)) {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    }
  }
</script>

{#if open}
    <dialog
      bind:this={dialog}
      class="dialog"
      aria-label="Help and keyboard shortcuts"
      in:surface={{ duration: 250, distance: 10 }}
      oncancel={(e) => { e.preventDefault(); onClose(); }}
      onkeydown={(e) => { e.stopPropagation(); onKeydown(e); }}
    >
      <div class="help-head">
        <strong>Help</strong>
        <button class="x" title="Close" aria-label="Close help" onclick={onClose}>×</button>
      </div>
      <BrandLockup />
      <p class="muted">A little preparation. A confident first service.</p>
      <button class="replay" onclick={() => { onClose(); onGettingStarted(); }}>Getting started</button>
      <p class="muted">
        Explore at your own pace. The interactive tutorial covers playlists, Output, songs, Scripture, reader layouts, reading progress, search, media, Looks, live tools, connections, updates, and returning to your current View. Jump to any topic and use its button to open the real controls.
      </p>
      <button
        class="replay"
        onclick={() => {
          onReplayTour();
          onClose();
        }}
      >
        Replay guided tour
      </button>
      <button class="replay" onclick={() => { onReplayTour(4); onClose(); }}>Explore Scripture reading</button>
      <button class="replay" onclick={() => { onReplayTour(17); onClose(); }}>Choose a reader layout</button>
      <button class="replay" onclick={() => { onReplayTour(7); onClose(); }}>Run the service check</button>
      <button class="replay" onclick={() => { onReplayTour(14); onClose(); }}>Follow the live slide</button>
      <button class="replay" onclick={() => { onReplayTour(15); onClose(); }}>Find entries in Logs</button>
      <button class="replay" onclick={() => { onReplayTour(16); onClose(); }}>Continue your current View</button>
      <div class="section-title">Start a service again</div>
      <p class="muted">View Hub marks your last selected starting playlist and remembers it on this device. Filter a long list by name, and keep your preferred slide ratio, Look, and transition for the next service. Pick another card any time. Reusing the saved-playlist list also avoids reloading it every time you open the Hub.</p>
      <div class="section-title">Service check</div>
      <p class="muted">The ✓ Service check reviews playlist slides, the audience display, and whether Output is open. Stage is optional. Each item opens the real control, and checking never puts a slide live.</p>
      <div class="section-title">Return to the live slide</div>
      <p class="muted">If you select another slide to edit, use Follow live slide below the Output preview or press Ctrl+Shift+L (⌘⇧L on Mac). The editor selects and scrolls to the current live slide without changing what the audience sees.</p>
      <div class="section-title">Search logs</div>
      <p class="muted">Settings → Logs shows recent entries. Filter by timestamp, level, or message text; Refresh for newer lines, or copy/export the full log for troubleshooting.</p>
      <div class="section-title">Backgrounds</div>
      <p class="muted">Pick a Look for Songs, Scripture and Text in Looks, or choose one for a single item from the item menu.</p>
      <p class="muted">Use Import media in the Media tab to add one or more images or videos directly to the library. A batch imports one file at a time and shows progress. Star a logo or background to keep it near the top of the library; Favorites narrows the grid to those saved visuals on this device. The tab also keeps your five most recently used visuals ready to add to another View; Clear removes this on-device shortcut list. Media thumbnails load as you scroll so a large library does less work at once.</p>
      <p class="muted">The content shelf remembers the last tab you used and whether it was open, so Songs, Scripture, or Media is ready where you left it next time. This preference stays on this device.</p>
      <p class="muted">
        Inheriting text slides use the playlist item's background, then the default Look for their kind. A slide with its own custom background keeps it. Open Background in the slide grid to choose a color or imported media for an item.
      </p>
      <p class="muted">
        You can drag an image or video onto a slide to change that slide's background, or onto the item Background chip to share it across the item.
      </p>
      <div class="section-title">Scripture bookmarks</div>
      <p class="muted">In Scripture, open Browse Scripture → Bookmarks. Search by reference, translation, or words from the passage, then choose Add slide. Selected verses use compact references such as John 3:16–18, 20.</p>
      <p class="muted">Recently added keeps the five passages you most recently added from Scripture search, Browse Scripture, bookmarks, or drag and drop. Select a reference chip to add it again; Clear removes the on-device shortcuts. Bookmarks remain saved separately.</p>
      <div class="section-title">Find your place and read a chapter</div>
      <p class="muted">Use Find a book to rank exact names, prefixes, and close spellings. Choose a chapter, then Read book for comfortable text that keeps clicks from adding slides by accident. Choose Auto, One, or Two columns; Auto fits the panel, and narrow layouts stay in one column. A−/A+ keeps your chosen text size (14–22) on this device. Select a verse to preview it; choose Add as slide only when you want to present it.</p>
      <p class="muted">The Scripture panel remembers the last chapter you opened. Continue reading reopens that translation, book, and chapter in one step. Your reading position and text size stay on this device.</p>
      <p class="muted">In Read book, mark a chapter read when you finish. Mark read &amp; next saves your progress and opens the next unread chapter in one step. The one-chapter daily goal and consecutive-day streak update at that point and stay on this device.</p>
      <p class="muted">Chapter numbers show what you have read in the selected translation. The progress bar tracks the book, and Next unread chapter opens the next chapter you have not marked.</p>
      <div class="section-title">Find content faster</div>
      <p class="muted">Press Ctrl+K (⌘K on Mac). Choose All, Songs, Scripture, or Media to focus the results; a scoped search skips lookups for unrelated content. Song results put matching titles first, tolerate small spelling mistakes, and also find lyric phrases. Scripture suggestions rank exact references first, then prefix and word matches, tolerating small typos before falling back to verse text.</p>
      <p class="muted">In the search window, press ↑ and ↓ to move through every result and Enter to add the highlighted one. Use Clear to start another search, or Escape to close search from any control. If a Bible cannot be read, Retry search or choose a different translation in Scripture; library songs and available media stay usable.</p>
      <div class="section-title">Prepare a song</div>
      <p class="muted">In Songs, search the library; large lists show 100 results at a time, and Show more adds another batch. A no-match search offers Clear search. Recently used songs stay at the top for quick adding to this View; Clear removes the local shortcut list. Click the star on any library song to keep it above the rest of the library on this device. Favorites stay pinned when the recent list changes, and you can unstar a song any time. In Song Editor, paste or find lyrics, choose Lines/slide, then try Smart layout. It balances text length and favors phrase endings while keeping lyric order and section groups. Review the preview before Add Song. Undo restores the last layout, reorder, duplicate, removal, or alignment change.</p>
      <p class="muted">Renaming the song keeps your arrangement. All slides alignment controls apply to every slide; the buttons on each preview card adjust just that slide. Editing the lyrics or line limit rebuilds the preview and clears arrangement Undo. If the title closely matches a song already in your library, the editor warns you and asks for a second Add Song press to add it anyway.</p>
      <div class="section-title">Bible downloads &amp; audio</div>
      <p class="muted">Bible catalog loads and downloads have bounded waits: a stalled catalog or download reports an error with Retry instead of spinning forever. In Settings → Audio, the device list shows when it is scanning, and a frozen track position surfaces a reconnect hint after repeated failures.</p>
      <div class="section-title">Lighter previews</div>
      <p class="muted">Playlist, editing, and Stage thumbnails show cached still posters for video backgrounds and overlays. Output and Stage pause video and camera previews when their windows are hidden, then resume when shown; the Stage clock also pauses. Offscreen thumbnails skip rendering, and hidden editor previews release media. Settings → What’s New records the changes and links back to this tutorial.</p>
      <div class="section-title">Keyboard shortcuts</div>
      <ul class="shortcuts">
        <li><span class="keys"><kbd>←</kbd> <kbd>→</kbd></span> Previous / next slide (when not typing)</li>
        <li><span class="keys"><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd></span> Follow the live slide <span class="muted">(⌘⇧L on Mac)</span></li>
        <li><span class="keys"><kbd>Alt</kbd>+<kbd>←</kbd> / <kbd>→</kbd></span> Previous / next chapter in reading mode</li>
        <li><span class="keys"><kbd>Ctrl</kbd>+<kbd>K</kbd></span> Search songs, Bibles &amp; media <span class="muted">(⌘K on Mac)</span></li>
        <li><span class="keys"><kbd>?</kbd></span> Open or close this shortcut guide</li>
        <li><span class="keys"><kbd>Esc</kbd></span> Close search, tour, dialog, or return from reading</li>
        <li><span class="keys"><kbd>↑</kbd> <kbd>↓</kbd> <kbd>Enter</kbd></span> Pick a Scripture suggestion or a search result</li>
        <li><span class="keys"><kbd>Enter</kbd> / <kbd>Esc</kbd></span> Confirm / cancel a dialog</li>
      </ul>
      <p class="muted">
        Prefer dragging? Songs, verses, and images can all be dragged straight
        onto the playlist — clicking works too.
      </p>
    </dialog>
{/if}

<style>
  .dialog::backdrop {
    background: rgba(0, 0, 0, 0.55);
  }
  .dialog {
    width: min(420px, 92vw);
    max-height: 84vh;
    overflow-y: auto;
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .help-head {
    display: flex;
    align-items: center;
    font-size: 14px;
  }
  .help-head strong {
    flex: 1;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 12px;
  }
  .x {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: transparent;
    color: var(--text-dim);
    width: 26px;
    height: 26px;
    line-height: 1;
    cursor: pointer;
  }
  .muted {
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.5;
    margin: 0;
  }
  .replay {
    align-self: flex-start;
    border: 1px solid var(--accent, #4f8cff);
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    padding: 6px 12px;
    cursor: pointer;
  }
  .section-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
    margin-top: 4px;
  }
  .shortcuts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 13px;
  }
  .keys {
    display: inline-flex;
    gap: 2px;
    margin-right: 6px;
  }
  kbd {
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 6px;
    background: var(--panel-2);
    white-space: nowrap;
  }
</style>

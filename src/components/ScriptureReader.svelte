<script lang="ts">
  import { tick } from "svelte";
  import type { ChapterVerse } from "../lib/types";
  import { surface } from "../lib/motion";
  import { loadReaderPreferences, saveReaderPreferences } from "../lib/readerPreferences";

  interface Props {
    book: string;
    chapter: number;
    version: string;
    verses: ChapterVerse[];
    fontSize: number;
    busy: boolean;
    read: boolean;
    canPrevious: boolean;
    canNext: boolean;
    savedVerse: number | null;
    resumeVerse: number | null;
    status: string | null;
    error: string | null;
    onRetry: () => void;
    onNavigate: (direction: -1 | 1) => void;
    onResize: (delta: -2 | 2) => void;
    onToggleRead: () => void;
    onSavePlace: (verse: number) => void;
    onClearPlace: () => void;
    onAdd: (verse: ChapterVerse) => void;
    onCopy: (verse: ChapterVerse) => void;
    onBookmark: (verse: ChapterVerse) => void;
    onExit: () => void;
  }

  let { book, chapter, version, verses, fontSize, busy, read, canPrevious, canNext, savedVerse, resumeVerse, status, error, onRetry,
    onNavigate, onResize, onToggleRead, onSavePlace, onClearPlace, onAdd, onCopy, onBookmark, onExit }: Props = $props();
  let preferences = $state(loadReaderPreferences());
  let selectedVerse = $state<number | null>(null);
  let scrollPane: HTMLDivElement;
  let heading: HTMLHeadingElement;

  $effect(() => { saveReaderPreferences(preferences); });
  $effect(() => {
    // A new chapter starts at the top; an explicit resume goes to its verse.
    const reference = `${book}:${chapter}`;
    const targetVerse = resumeVerse;
    selectedVerse = null;
    let cancelled = false;
    void tick().then(() => {
      if (cancelled || !scrollPane?.isConnected) return;
      const target = targetVerse == null ? null : scrollPane.querySelector<HTMLElement>(`[data-verse="${targetVerse}"]`);
      scrollPane.scrollTop = 0;
      if (target) scrollPane.scrollTop = target.getBoundingClientRect().top - scrollPane.getBoundingClientRect().top - 24;
      heading?.focus({ preventScroll: true });
    });
    return () => { cancelled = true; };
  });

  function finishAndContinue(): void {
    if (busy) return;
    if (!read) onToggleRead();
    if (canNext) onNavigate(1);
  }
</script>

<section class="scripture-reader" class:paper={preferences.tone === "paper"} aria-label="Focused Scripture reader" aria-busy={busy} in:surface={{ duration: 220, distance: 6 }}>
  <header class="reader-top">
    <div class="chapter-heading"><span>READING ROOM · {version}</span><h2 bind:this={heading} tabindex="-1">{book} {chapter}</h2><small>{verses.length} verses · {read ? "Chapter read" : "Take it at your own pace"}</small></div>
    <button class="exit" onclick={onExit}>← Back to browse</button>
  </header>
  <div class="reading-controls" aria-label="Reading preferences">
    <div class="segmented" role="group" aria-label="Reader layout">
      <button aria-pressed={preferences.layout === "page"} onclick={() => (preferences.layout = "page")}>Page</button>
      <button aria-pressed={preferences.layout === "columns"} onclick={() => (preferences.layout = "columns")}>Columns</button>
    </div>
    <div class="type-controls" role="group" aria-label="Reader text size">
      <button disabled={fontSize <= 14} aria-label="Make reader text smaller" onclick={() => onResize(-2)}>A−</button><span>{fontSize}</span><button disabled={fontSize >= 28} aria-label="Make reader text larger" onclick={() => onResize(2)}>A+</button>
    </div>
    <button aria-pressed={preferences.tone === "paper"} onclick={() => (preferences.tone = preferences.tone === "paper" ? "dark" : "paper")}>Paper tone</button>
    <button disabled={busy} aria-pressed={read} onclick={onToggleRead}>{read ? "✓ Chapter read" : "Mark chapter read"}</button>
  </div>
  <div class="reader-note" role={error ? "alert" : "status"}>{error || status || (savedVerse ? `Your place is saved at verse ${savedVerse}.` : "Select a verse to copy, bookmark, save your place, or add a slide.")}{#if error}<button disabled={busy} onclick={onRetry}>Retry chapter</button>{/if}{#if savedVerse}<button onclick={onClearPlace}>Clear saved place</button>{/if}</div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users need to scroll the reading pane.) -->
  <div class="reader-scroll" bind:this={scrollPane} tabindex="0" role="region" aria-label={`${book} ${chapter} verse text`}>
    <ol class="reader-passages" class:columns={preferences.layout === "columns"} style={`--reading-size:${fontSize}px`}>
      {#each verses as verse (verse.verse)}
        <li data-verse={verse.verse} class:saved={savedVerse === verse.verse} class:selected={selectedVerse === verse.verse}>
          <button class="verse" disabled={busy} aria-label={`${book} ${chapter}:${verse.verse}${savedVerse === verse.verse ? ", saved place" : ""}`} aria-pressed={selectedVerse === verse.verse} onclick={() => (selectedVerse = selectedVerse === verse.verse ? null : verse.verse)}>
            <span class="number">{verse.verse}{#if savedVerse === verse.verse}<span class="place-mark" aria-hidden="true">◆</span>{/if}</span><span>{verse.text}</span>
          </button>
          {#if selectedVerse === verse.verse}
            <div class="verse-actions" in:surface={{ duration: 150, distance: 3 }} aria-label={`Actions for verse ${verse.verse}`}>
              <button disabled={busy} onclick={() => onSavePlace(verse.verse)}>Save my place</button>
              <button disabled={busy} onclick={() => onCopy(verse)}>Copy</button>
              <button disabled={busy} onclick={() => onBookmark(verse)}>Bookmark</button>
              <button disabled={busy} onclick={() => onAdd(verse)}>Add as slide</button>
            </div>
          {/if}
        </li>
      {/each}
    </ol>
    <div class="chapter-finish"><span>{read ? "You’ve read this chapter." : "At the end of the chapter?"}</span><button class="primary" disabled={busy || (read && !canNext)} onclick={finishAndContinue}>{canNext ? (read ? "Continue to next chapter →" : "Mark read & continue →") : (read ? "✓ Chapter complete" : "Mark chapter read")}</button></div>
  </div>
  <footer><button disabled={busy || !canPrevious} onclick={() => onNavigate(-1)}>← Previous chapter</button><span>{book} {chapter}</span><button disabled={busy || !canNext} onclick={() => onNavigate(1)}>Next chapter →</button></footer>
</section>

<style>
  .scripture-reader { --reading-bg: #1b2220; --reading-ink: #e5ece6; --reading-muted: #a5b6ab; --reading-line: #36463d; --reading-accent: #a2c9ae; display: flex; flex-direction: column; height: 100%; min-height: 0; border: 1px solid var(--reading-line); border-radius: 14px; color: var(--reading-ink); background: var(--reading-bg); container-type: inline-size; container-name: reading-room; overflow: hidden; }
  .paper { --reading-bg: #f2ecdf; --reading-ink: #2e3932; --reading-muted: #5c6b60; --reading-line: #cecbbb; --reading-accent: #355d45; }
  button { min-height: 32px; padding: 6px 10px; border: 1px solid var(--reading-line); border-radius: 7px; background: transparent; color: var(--reading-ink); font: inherit; font-size: 11px; cursor: pointer; }
  button:hover:not(:disabled), button[aria-pressed="true"] { background: color-mix(in srgb, var(--reading-accent) 12%, var(--reading-bg)); border-color: var(--reading-accent); }
  button:focus-visible, .reader-scroll:focus-visible { outline-color: var(--reading-accent); }
  .reader-top { display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 12px; padding: 20px 24px 14px; }
  .chapter-heading { display: grid; gap: 4px; }
  .chapter-heading > span { color: var(--reading-accent); font-size: 9px; letter-spacing: .13em; font-weight: 700; }
  h2 { margin: 0; font-family: Georgia, "Times New Roman", serif; font-size: 28px; font-weight: 400; line-height: 1.2; outline: none; }
  small { color: var(--reading-muted); font-size: 11px; }
  .exit { border-color: transparent; color: var(--reading-accent); }
  .reading-controls { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 0 24px 12px; }
  .segmented, .type-controls { display: inline-flex; align-items: center; gap: 3px; }
  .type-controls span { min-width: 22px; text-align: center; color: var(--reading-muted); font-size: 10px; }
  .reader-note { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 12px; margin: 0; padding: 8px 24px; border-block: 1px solid var(--reading-line); color: var(--reading-muted); font-size: 11px; }
  .reader-note button { min-height: 24px; padding: 2px 5px; border-color: transparent; color: var(--reading-accent); }
  .reader-scroll { flex: 1; min-height: 0; overflow: auto; padding: 18px 24px 24px; overscroll-behavior: contain; scrollbar-color: var(--reading-line) transparent; }
  .reader-passages { max-width: 70ch; margin: 0 auto; padding: 0; list-style: none; font-family: Georgia, "Times New Roman", serif; font-size: var(--reading-size, 18px); }
  li { break-inside: avoid; border-radius: 8px; border-left: 2px solid transparent; margin-bottom: 2px; }
  li.saved { border-left-color: var(--reading-accent); }
  li.selected { background: color-mix(in srgb, var(--reading-accent) 8%, var(--reading-bg)); }
  .verse { display: flex; gap: 12px; width: 100%; padding: 9px 10px; border: 0; text-align: left; font-family: inherit; font-size: inherit; line-height: 1.85; overflow-wrap: anywhere; user-select: text; }
  .number { flex: 0 0 22px; display: grid; align-content: start; padding-top: .4em; color: var(--reading-muted); font-family: var(--font-body); font-size: 10px; text-align: right; }
  .place-mark { color: var(--reading-accent); font-size: 8px; }
  .verse-actions { display: flex; flex-wrap: wrap; gap: 6px; padding: 0 10px 12px 44px; }
  .verse-actions button { color: var(--reading-accent); font-family: var(--font-body); }
  .chapter-finish { display: flex; flex-wrap: wrap; justify-content: center; align-items: center; gap: 12px; margin-top: 24px; padding: 20px 0 6px; border-top: 1px solid var(--reading-line); color: var(--reading-muted); font-size: 12px; }
  .primary { color: var(--reading-bg); background: var(--reading-accent); border-color: var(--reading-accent); font-weight: 700; }
  .primary:hover:not(:disabled) { color: var(--reading-ink); }
  footer { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; padding: 12px 24px; border-top: 1px solid var(--reading-line); }
  footer > span { color: var(--reading-muted); font-size: 11px; }
  @container reading-room (min-width: 880px) { .reader-passages.columns { max-width: 120ch; column-count: 2; column-gap: 40px; column-rule: 1px solid var(--reading-line); } }
  @container reading-room (max-width: 500px) { .reader-top { padding: 14px 12px 10px; } .reading-controls { padding: 0 12px 10px; gap: 6px; } .reader-note { padding-inline: 12px; } .reader-scroll { padding: 12px 8px 16px; } footer { padding: 10px; } footer > span { display: none; } .verse { gap: 8px; padding-inline: 6px; } }
</style>

# Change log

User-facing features and fixes are also available in **Settings → What’s New**.

## 2026-10-12 — Faster search navigation and steadier renderers

### Added

- Global search keyboard navigation: `↑`/`↓` moves through every result, `Enter` adds the highlighted song, verse, or media.
- Intelligent Scripture ranking: exact references first, then prefix and word matches with tolerance for small typos, then verse-text matches.

### Improved

- Settings opens search directly instead of simulating a `Ctrl+K` keypress after a timeout.
- Stage next-slide text announces politely for screen readers.

### Fixed

- Output and Stage cancel pending transition frames on teardown and ignore late callbacks.
- Scripture chapter fan-out is bounded to six chapters with a 15-second limit each; one failing chapter no longer discards the other matches. Global search chapter failures are isolated per chapter.
- Playlist drag click suppression auto-releases via `once`, even if its fallback timer never runs.

## 2026-10-12 — Smarter song layout and safer arrangement editing

### Added

- Song Editor Undo restores the last layout, reorder, duplication, removal, or global/per-slide alignment change. Only one snapshot is retained; lyric/line-limit changes and closing the editor clear it.
- Optional Smart layout uses dynamic programming to balance lyric length, prefer punctuation boundaries, and discourage single-line orphans while respecting the selected line limit, contiguous section groups, and alignment boundaries. It preserves lyric lines and their order; it is a heuristic preview, not a guarantee of final text fit. Review before Add Song or use Undo.

### Improved

- Clearer All slides alignment labels, a line-limit-aware empty state, responsive arrangement toolbar, and status feedback for layout/Undo.
- Help describes the simplified paste/find lyrics → choose line limit → Smart layout → review → Add Song workflow.

### Fixed

- Naming/alignment changes no longer reparse the source and discard manual arrangement edits. Per-slide alignment is no longer overwritten by global alignment effects.
- Each new Song Editor session regenerates its arrangement even when reopening the same lyrics and resets stale drag state.
- Pending lyric-search timers and scheduled focus callbacks are cancelled on close/destruction, Undo snapshots are released on close, and late responses are invalidated. Lyric search/loading waits are bounded at 15 seconds with retry guidance.
- Escape closes Song Editor from any control. Smart layout failures retain the existing arrangement.

## 2026-10-12 — Smarter search and simpler content selection

### Added

- Recent searches: up to eight queries used successfully to add content or open lyric review are stored on this device. Select one to rerun it, or Clear history to remove the list. Invalid stored data and unavailable local storage do not prevent searching.
- Local song ranking algorithm: normalize accents/punctuation, prioritize exact titles and prefixes, then title tokens, single-edit title typos (including swapped adjacent letters), then lyric matches. Results explain whether they matched the title, a similar title, or lyrics. A derived index avoids normalizing the whole library on each keystroke; typo comparisons and history are bounded.

### Improved

- One-click Clear resets search. Local library matches appear immediately, and matching titles rank above lyric-only matches. Help explains the simplified search/reuse/add workflow.
- Search has an explicit accessible label, loading announcements, and consistent busy states across add actions.

### Fixed

- Escape closes search/naming dialogs from any control, not just their input. Search add operations prevent concurrent duplicate clicks; late results cannot close a newly opened search or overwrite its errors.
- Selected-Bible read failures now surface a retryable error rather than disappearing as empty results. Failed Bible-list requests are not cached permanently, and Bible metadata loads only when Scripture results need it.
- Camera discovery uses a shared cancellation signal and captured device identity. Closing/replacing a preview prevents new capture or fallback requests after cancellation; streams resolving late are stopped. Permission/busy errors do not reopen the default camera.
- Naming dialogs cancel scheduled focus animation frames on close/destruction.

## 2026-10-12 — Bookmark search and resource cleanup

### Added

- Search saved Scripture bookmarks by reference, translation, or passage text, with matching counts and Clear search. Help explains where to find it.

### Improved

- Selected passages use compact verse references (for example, `John 3:16–18, 20`) when adding slides, copying, or saving bookmarks.
- Global Scripture/media/lyrics search and NDI monitor startup use bounded waits. Search failures offer Retry search; available media results render before Scripture chapter lookups finish.
- Audio and NDI source polling allow only one request at a time, avoiding accumulating requests when the backend is slow.

### Fixed

- Pending Look edits are saved when switching Looks or closing Settings; deleting a Look cancels its pending debounce.
- Late MIDI/NDI listener registrations are released after Settings closes or monitor startup is cancelled. Monitor teardown releases listeners, polling, frames, and its owned discovery/receiver resources.
- Output and Stage release event listeners even when registration finishes after renderer teardown. Independent listener setup allows initial state loading to begin immediately and handles registration errors.
- Search debounces and focus animation frames are cancelled on close/destroy; editor verse-search and transient interface timers are cleared on teardown. Stale responses cannot change a newer search's translation or loading state.
- Duplicate and malformed saved bookmark entries are skipped during loading to prevent list rendering failures.

## 2026-10-12 — Scripture and workspace improvements

### Added

- Copy selected Scripture verses with their reference and translation, or clear the verse selection in one click.
- Search the downloadable Bible catalog and install the included NIV, NKJV, NLT, MSG, ESV, NIRV, NASB, and AMP translations for offline browsing. The public catalog can offer additional translations.
- Search verse text in the selected translation and add a search result directly to the playlist. Preview verses, select multiple verses or a range, add a whole chapter, and copy a verse or passage.
- Save verse, selected-range, chapter, and search-result bookmarks locally, then add a saved passage as a slide.
- Settings remembers the last tab you used.
- Previous/Next chapter buttons let you move through the selected book from the Scripture reading pane. They disable at the book boundaries and while a chapter loads.
- Press `?` to open or close the keyboard shortcut guide.

### Improved

- Bible catalog search shows a matching version count and a Clear search action for empty results.
- Bible library, book, and chapter requests time out after 15 seconds and offer retry controls for stalled or failed requests.
- Scripture slide references and search results identify their translation.
- The Scripture browser remembers the selected translation.
- The app reopens the saved project and surfaces autosave recovery when available.
- Workspace state loads before optional startup setup; a timeout now gives a retry action instead of leaving an endless loading screen.

### Fixed

- Fixed the startup freeze caused by separated playlist groups sharing the same ID. Recovery preserves every slide, its order, and each group's background and Look; future edits also normalize group identities.
- Adding a slide to an earlier playlist item now inserts it at the end of that item instead of splitting it across the playlist.
- Added an editor error boundary with Try again and Reload app controls, plus local frontend error diagnostics.
- A failed chapter request preserves the currently displayed passage and selection.
- A failed Bible download keeps the catalog visible and offers a retry for the same version.
- The `?` shortcut now closes the keyboard shortcut guide when it is already open.
- Rapidly changing Bible, book, or chapter no longer lets a slower old request replace the current Scripture selection.

## 2026-10-07 — Downloadable Bible versions

- Browse a searchable catalog of translations and install included versions from the app bundle. Downloaded Bible data remains available offline and separate from KJV/OpenLP imports.

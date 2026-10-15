# Change log

User-facing features and fixes are also available in **Settings → What’s New**.

## 2026-10-15 — Quicker content reuse, lighter background work

### Added

- Songs keeps the five most recently added library songs at the top of the source panel. Click a song to add it to the current View without searching, or Clear the local shortcut list. Successful adds from the library, drag and drop, and global search all update the recent list.
- Star any library song to keep it above the rest of the library on this device. Favorites stay pinned when the recent list changes; click the star again to unpin. Onboarding, Help, the interactive Songs tutorial, and Settings → What’s New explain it.
- View Hub remembers the last starting playlist selected on this device and marks it **Last selected**. Choose another card any time; deleting the remembered playlist moves the selection to an available starting point.
- View Hub also remembers your preferred slide ratio, Look, and transition on this device, reducing repeated setup for the next service. Filter a long starting-playlist list by name; a no-match state helps recover. The compact layout keeps the picker and settings reachable in narrow windows.
- **Import media** in the Media tab adds one or more images or videos directly to the reusable library. The empty state, onboarding, Help, and interactive Media tutorial explain the path.
- Songs search offers **Clear search** when nothing matches. Large libraries show 100 results first; **Show more** appends another batch. Help and the interactive Songs tutorial explain how to find the rest.
- Browse Scripture keeps the five passages most recently added from browsing, bookmarks, global search, or drag and drop ready for one-click reuse. Clear removes the on-device shortcuts; onboarding, Help, and the interactive Scripture tutorial explain the feature.
- Global search adds All, Songs, Scripture, and Media scopes with live result counts. Scoped searches skip unrelated lookups and hide unrelated result groups; the scope bar and search dialog adapt to narrow windows. Onboarding, Help, and the interactive Search tutorial explain it.
- Choosing a search scope skips irrelevant Scripture, media, online-lyrics, and song-ranking work and renders only that result group. Scope changes use a brief reduced-motion-aware fade to reduce avoidable search and layout work; device temperature was not measured.
- Media keeps the five most recently used visuals ready in a compact horizontal row. Click one to add it to the current playlist, or Clear the on-device shortcut list. The row adapts to narrow panels.
- First-run onboarding, Help, the interactive Songs and Media tutorial topics, and Settings → What’s New explain the shortcuts.

### Improved

- Audio playback position refreshes once per second instead of twice and pauses while the app window is hidden, reducing background IPC and UI work while keeping the visible position current to the second.
- Media-library thumbnails load as they approach the viewport. The media list uses cached still images and does not start a video decoder for its preview cards.
- The Songs, Scripture, and Media shelf remembers its last tab and whether it was open. Media loads on launch only when the shelf was left open on Media; the preference stays on this device.
- View Hub reuses its saved-playlist list after the first successful load instead of requesting it on every open. Failed loads retry the next time the Hub opens; selecting a starting playlist uses a brief reduced-motion-aware animation.
- View Hub filtering stays on-device and reduces the number of cards laid out for long saved-playlist lists; device temperature was not measured.
- Media imports selected files one at a time, reports progress, and refreshes the library once after the batch. Serial thumbnail conversion avoids overlapping media work; device temperature was not measured.
- The Songs library renders its first 100 matches and appends more on request, reducing initial layout and paint work for large libraries; device temperature was not measured.
- Song search reuses normalized titles as you type instead of cleaning every title again on each keystroke. Favorites add no background polling; device temperature was not measured.
- Switching source tabs fades in briefly, respects reduced-motion preferences, and keeps the existing responsive shelf resize animation.
- The reduction in background work was implemented to lower the chance of unnecessary device heating; device temperature and power draw were not measured.

The first-run guide, Help, the interactive Media tutorial, and Settings → What’s New describe the remembered shelf state.

## 2026-10-14 — Smoother settings, service prep, and Scripture browsing

### Added

- Service check in the top bar reviews playlist slides, the selected audience display, and Output visibility in one animated checklist. Stage appears separately as optional; each action opens its control without putting a slide live or starting background work. It is covered in onboarding, Help, What’s New, and the interactive tutorial.
- Follow live slide returns the editor to the slide already on air and scrolls it into view after you select another slide to edit. It changes editing selection only; Output is untouched. Onboarding, Help, What’s New, and the interactive tutorial explain the shortcut.
- Ctrl+Shift+L (⌘⇧L on Mac) follows the live slide from the keyboard, including while Scripture reading is open; it never changes Output.
- Settings → Logs now filters recent entries by timestamp, level, or message text without rendering lines that do not match. Help, What’s New, and the interactive tutorial point to the filter; copying and exporting still include the full log.
- View Hub identifies the current autosaved View and offers **Continue this View**, so returning from New View is one clear action. Help, What’s New, and the interactive tutorial include the shortcut.
- Read book adds **Mark read & next** to save a finished chapter and open the next unread one in one step, carrying the local daily goal forward without a reminder.
- Read book offers saved **Auto**, **One**, and **Two** column layouts. Auto follows the reader panel width, narrow panes stay in one column, and focus mode gives the passage more room. Onboarding, Help, What’s New, and the interactive tutorial cover the controls.

### Improved

- The separate Scripture reading room is gone: Read book now keeps one simple reading mode inside the Bible browser, with comfortable text, A−/A+ sizes from 14–22, chapter navigation, mark-read, and explicit add, copy, and bookmark actions.
- The reader adapts to narrow layouts with a tighter toolbar and verse list, and offscreen verses skip layout and paint until scrolled into view, keeping long chapters light on low-power devices.
- Scripture layout breakpoints now respond to the browser panel’s width, so split-pane layouts reflow correctly even in a wide application window.
- Hidden Output and Stage windows pause video and camera previews, then resume them when shown; the Stage clock pauses while hidden. This avoids ongoing media decoding and timer work when those windows are not visible. Device temperature was not measured.
- The interactive tutorial explains reader layouts, Mark read & next, hidden-window media pauses, the service check, Follow live slide, and log filters alongside Scripture browsing, search, and presentation controls. Onboarding, Help, and What’s New can reopen these lessons or Scripture tools.

### Fixed

- Repeated identical log records no longer prevent Settings from opening when Logs is the last tab used.
- Continue reading returns to the last opened chapter without stale saved-verse state, and chapter navigation keeps keyboard focus predictable.

### Verification

- `npm run check`, `npm run build`, and `git diff --check` pass. Device temperature/power draw was not measured in this verification.

## 2026-10-13 — A reading room that remembers your place

### Added

- A focused Scripture reading room with Page and Columns layouts, Paper tone, and text sizes from 14–28. Wide panels can use two columns; narrow panels reflow to one. Layout, tone, and text size are saved locally.
- Select a verse → Save my place. Continue reading reopens that translation and scrolls to the exact verse, even after browsing other passages or restarting. Clear saved place restores the last-chapter behavior. Verse actions keep copying, bookmarks, and slide insertion explicit.
- Mark read & continue completes a chapter and opens the next chapter in the same book. Previous/Next disable at book boundaries.
- A 15-topic interactive tutorial covers playlists/Views, Output/Stage, song preparation, Scripture selection/downloads, reader preferences, daily/book progress, saved places, global/recent search, media/camera/backgrounds, Looks, overlays/countdown/transitions, connections/audio/triggers/network, and release notes. Topic selection and action buttons open the real tools without adding content or starting projection.
- Settings and What’s New offer tutorial entry points, including a reader lesson. Help and animated first-run onboarding explain the same features.

### Improved

- Reading mode uses more of the window and hides book/chapter pickers until Back to browse. Controls wrap, reading text stays within a comfortable measure, and the passage scrolls independently of chapter navigation.
- Playlist, editing, and Stage thumbnails use cached still posters for video backgrounds and overlays rather than opening a video decoder for each thumbnail. The visible live Output preview retains motion; actual projection rendering keeps its existing playback.
- Intersection and visibility observers skip offscreen slide rendering and release hidden editor preview media/cameras. Thumbnail sizing no longer registers a redundant window resize listener when ResizeObserver is available.
- The editor confirmation clock stops when no renderer is visible or the editor is hidden. Font-ready callbacks cannot restart text fitting after teardown.
- Reader and tutorial transitions are short and respect reduced-motion preferences. No continuous decorative animation or reading-progress polling was added.

### Fixed

- Chapter changes clear stale reading feedback. Changing the Bible, book, or chapter interrupts pending Continue reading navigation instead of letting it replace a newer choice.
- Failed reader navigation preserves the current passage and shows Retry chapter. Saved places can also be cleared from the browser if their translation is no longer installed.
- Focused reading prevents arrow keys from advancing live slides. Alt+← / → changes chapters, and Escape returns to browsing with focus on Read book.

### Verification

- `npm run check`, `npm run build`, `npm run test:reader`, and `git diff --check` pass. The five persistence tests cover malformed/legacy data, exact verses, independent preferences, and blocked storage.
- Chromium browser checks with mocked native commands and bundled KJV text cover reading at 1440, 1000, 680, and 390 pixels; saved-place scrolling/reload; layout/tone preferences; mark-and-continue; failed chapter retry; Help/Settings/tutorial actions; and no unintended insert/projection commands. A 100-video-slide fixture has zero grid video elements, and hidden editor previews release video elements. Actual Output and Stage components retain video rendering.
- Physical projection displays, native Windows WebView2, real camera devices, and device temperature/power draw were not measured in this verification.

## 2026-10-13 — A calmer Scripture reading flow

### Added

- Continue reading restores the last translation, book, and chapter from this device in one click.
- Mark a chapter read when you finish. The one-chapter daily goal and consecutive-day streak update at that point and stay on this device, with no background polling or reminders.
- Chapter checkmarks and a book progress bar show what you have finished in the selected translation. Next unread chapter takes you straight to the next open chapter.
- Read book opens a focused, full-text reader. Verse clicks open a preview instead of inserting a slide; adding remains an explicit action. Text size, previous/next chapter, and the presentation workflow stay available.
- Find a book ranks exact names, prefixes, matching words, and small spelling mistakes. The animated eight-step guided tour covers Scripture, daily goals, book progress, and the reader; first-run onboarding and Help describe the same route.

### Improved

- Reader text and the book/chapter/verse layout adapt to the Scripture panel width, including stacked narrow layouts.
- Scripture parsing accepts numbered book elements such as the bundled NIRV XML, decodes common legacy character encodings, combines split verse labels, and removes empty books/chapters from the browser.
- A two-entry cache keeps recent downloaded translations in memory, imported XML is parsed once per session, and Bible text search waits for three characters plus a short typing pause to avoid repeated full-file reads and scans.

### Fixed

- Translations with books labeled only by sequential numbers no longer open with missing or empty book names. Empty verse placeholders are skipped, and one unusual verse label no longer removes readable text from its chapter.

## 2026-10-12 — Duplicate warnings and steadier downloads

### Added

- Song Editor duplicate guard: adding a song whose title closely matches one already in the library shows the closest match and asks for a second press to add it anyway.
- Intelligent duplicate comparison: exact and overlapping titles first, then same-word, typo-tolerant, and mostly-shared-word matches.

### Improved

- Settings → Audio shows a scanning state while listing outputs; the MIDI empty state explains how to get a device listed.
- Camera picker empty state needs one press (`Enable camera access & find cameras`) instead of two, and camera buttons disable while busy.

### Fixed

- Renaming a Look cancels its pending focus callback on teardown instead of focusing a removed input.
- Bible catalog loads are bounded at 15 seconds and downloads at 2 minutes with retry guidance; a frozen audio track position surfaces a reconnect hint after repeated failures.

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

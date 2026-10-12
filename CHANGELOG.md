# Change log

User-facing features and fixes are also available in **Settings → What’s New**.

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

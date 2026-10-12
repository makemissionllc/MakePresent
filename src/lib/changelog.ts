export interface ChangeLogItem {
  kind: "New" | "Improved" | "Fixed";
  text: string;
}

export interface ChangeLogRelease {
  date: string;
  title: string;
  items: ChangeLogItem[];
}

/** User-facing release notes shared by the Settings page and project changelog. */
export const CHANGELOG: ChangeLogRelease[] = [
  {
    date: "October 12, 2026",
    title: "Smarter song layout and safer arrangement editing",
    items: [
      { kind: "New", text: "Song Editor has Undo for the last arrangement change: layout, reorder, duplicate, remove, or alignment. Undo keeps only one snapshot and clears when lyrics or the line limit change." },
      { kind: "New", text: "Smart layout balances lyric length, favors punctuation boundaries, and avoids isolated lines where possible within your selected line limit. Review the result before adding the song, or Undo it." },
      { kind: "Fixed", text: "Renaming a song or changing alignment no longer rebuilds the arrangement and discards manual edits. Individual slide alignment now stays as chosen." },
      { kind: "Improved", text: "Song Editor labels global alignment controls clearly, updates its empty-state line limit, and keeps layout actions usable in narrower panels. Help explains the paste, layout, review, and add workflow." },
      { kind: "Fixed", text: "Song Editor cancels pending search timers and focus callbacks on close or teardown, clears Undo on close, and ignores late lyric responses. Online lyric requests have a 15-second loading limit with retry guidance." },
      { kind: "Fixed", text: "Escape closes Song Editor from any control, and each new editing session starts with a fresh arrangement and drag state. A layout failure keeps the existing slides." },
    ],
  },
  {
    date: "October 12, 2026",
    title: "Smarter search and simpler content selection",
    items: [
      { kind: "New", text: "Recent searches remember up to eight queries used to add content or review lyrics. Choose one to search again, or clear the local history." },
      { kind: "New", text: "Song search ranks exact titles, title prefixes, matching words, small spelling mistakes, and lyric matches. Accents and punctuation are normalized; match labels explain the results." },
      { kind: "Improved", text: "Clear resets a search in one click. Song titles are ranked above lyric matches, and local song search works immediately while other sources load." },
      { kind: "Fixed", text: "Escape closes search and naming dialogs even when focus is on a result or button. Search cannot add multiple items concurrently, and old insertion responses cannot close a newer search." },
      { kind: "Fixed", text: "A selected Bible read failure now explains the problem and offers Retry search instead of silently showing no verses. Failed Bible-list lookups can be retried." },
      { kind: "Fixed", text: "Closing a camera preview cancels pending discovery and prevents fallback capture from starting afterward. Permission failures no longer trigger a second camera request. Closing a naming dialog cancels scheduled focus work." },
    ],
  },
  {
    date: "October 12, 2026",
    title: "Bookmark search and resource cleanup",
    items: [
      { kind: "New", text: "Search saved Scripture bookmarks by reference, translation, or passage text. The list shows matching counts and a Clear search action." },
      { kind: "Improved", text: "Selected Scripture verses use compact references such as John 3:16–18, 20 when adding, copying, or bookmarking a passage." },
      { kind: "Fixed", text: "Switching Looks or closing Settings saves the pending style edit instead of silently dropping it. Deleting a Look cancels its pending edit." },
      { kind: "Fixed", text: "MIDI and NDI listeners that finish connecting after Settings closes are released. Monitor startup can be cancelled, and its polling and preview frames are cleared on close." },
      { kind: "Fixed", text: "Output and Stage release late event registrations during renderer teardown, and optional listener setup no longer blocks their initial state request." },
      { kind: "Improved", text: "NDI startup and global search have loading limits. Failed searches offer Retry search, and audio/NDI polling avoids overlapping requests." },
      { kind: "Fixed", text: "Closing search or rebuilding the editor cancels pending search and interface timers, releases scheduled focus work, and ignores stale responses." },
      { kind: "Fixed", text: "Duplicate or malformed saved bookmark entries are skipped on load so they cannot crash the bookmark list." },
    ],
  },
  {
    date: "October 12, 2026",
    title: "Scripture and workspace improvements",
    items: [
      { kind: "Fixed", text: "Recovered playlists with separated groups sharing an ID now open correctly. Moving or adding slides preserves unique groups and their backgrounds and Looks." },
      { kind: "Fixed", text: "Adding a slide to an earlier playlist item keeps the new slide inside that item." },
      { kind: "Improved", text: "An editor rendering failure now shows recovery buttons and writes a local diagnostic instead of leaving the opening spinner on screen." },
      { kind: "New", text: "Copy selected Scripture verses with their reference and translation, or clear the selection in one click." },
      { kind: "Fixed", text: "A failed chapter request keeps the passage you were reading instead of clearing it. A failed Bible download also keeps the catalog visible and offers Retry this version." },
      { kind: "Improved", text: "The Bible download list shows the number of matching versions and offers Clear search when no versions match." },
      { kind: "Improved", text: "Bible library, book, and chapter requests have a 15-second loading limit and retry controls, so a stalled request does not leave the browser loading indefinitely." },
      { kind: "New", text: "Search the Bible catalog and add included translations such as NIV, NKJV, NLT, MSG, ESV, NIRV, NASB, and AMP for offline browsing. The catalog can also offer other downloadable translations." },
      { kind: "Improved", text: "Scripture slides and search results identify the translation alongside the verse reference." },
      { kind: "New", text: "Search within the selected Bible and add a matching verse directly. Preview verses, select multiple verses or a range, add a chapter, copy passages, and save verse or passage bookmarks on this device." },
      { kind: "Improved", text: "The Scripture browser remembers the selected translation. The last project and autosave recovery are restored when the app opens." },
      { kind: "Improved", text: "Press ? to open or close the keyboard shortcut guide." },
      { kind: "New", text: "Settings remembers the tab you last used, so it opens where you left off." },
      { kind: "New", text: "Use Previous chapter and Next chapter in the Scripture browser to read onward without returning to the chapter grid. Navigation stops at the first and last chapter of the selected book." },
      { kind: "Fixed", text: "Pressing ? while the keyboard shortcut guide is open now closes it, as the guide describes." },
      { kind: "Fixed", text: "Workspace loading starts before optional setup and now offers a retry if the app does not answer." },
      { kind: "Fixed", text: "Quick changes to the selected Bible, book, or chapter no longer let an older response replace the current Scripture." },
    ],
  },
  {
    date: "October 7, 2026",
    title: "Downloadable Bible versions",
    items: [
      { kind: "New", text: "Browse included Bible translations and a searchable catalog of downloadable versions. Downloaded translations remain available offline." },
    ],
  },
];

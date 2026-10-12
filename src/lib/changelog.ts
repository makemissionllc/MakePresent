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

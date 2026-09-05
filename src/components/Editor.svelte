<script lang="ts">
  import { onMount } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { listen } from "@tauri-apps/api/event";
  import { api, subscribeAck, subscribeState, subscribeAutosave, subscribeLibrary } from "../lib/sync";
  import type { AckUpdate, Background, BibleInfo, ChapterVerse, ClientState, DisplayInfo, Library, LibrarySong, PlaylistTemplate, ScriptureMatch, Slide } from "../lib/types";
  import { isMedia, isLiveCamera } from "../lib/types";
  import SettingsPanel from "./SettingsPanel.svelte";
  import Modal from "./Modal.svelte";
  import SlideRender from "./SlideRender.svelte";
  import SongEditorModal from "./SongEditorModal.svelte";
  import ProjectHub from "../lib/components/ProjectHub.svelte";
  import GlobalSearch from "./GlobalSearch.svelte";
  import LookEditorView from "./LookEditorView.svelte";
  import GuidedTour from "./GuidedTour.svelte";
  import HelpModal from "./HelpModal.svelte";
  import {
    dismissHint,
    dismissTour,
    loadOnboarding,
    markUsed,
    resetTourDismissal,
    showHint,
  } from "../lib/onboarding";

  const PALETTE = [
    "#1a1a24",
    "#0f2b4a",
    "#123a5c",
    "#1f3a2f",
    "#3a2b1f",
    "#3d1f1f",
    "#2b2b3d",
    "#000000",
  ];

  const MEDIA_FILTERS = [
    {
      name: "Images",
      extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "svg", "avif"],
    },
    {
      name: "Videos",
      extensions: ["mp4", "m4v", "mov", "webm", "mkv", "avi", "ogv"],
    },
  ];

  let appState = $state<ClientState | null>(null);
  let selectedId = $state<string | null>(null);
  let displays = $state<DisplayInfo[] | null>(null);
  let savedLabel = $state<string>("Not saved");
  let errorMsg = $state<string | null>(null);
  let noticeDismissed = $state(false);
  let library = $state<Library | null>(null);
  let librarySearch = $state("");
  let settingsOpen = $state(false);
  let welcomeDismissed = $state(false);
  let importingMedia = $state(false);
  let scriptureQuery = $state("");
  let scriptureResults = $state<ScriptureMatch[]>([]);
  let scriptureOpen = $state(false);
  let scriptureIdx = $state(-1);
  let scriptureLoading = $state(false);
  let scriptureTimer: ReturnType<typeof setTimeout> | null = null;
  let scriptureBusy = $state(false);
  let scriptureStatus = $state<string | null>(null);
  let scriptureSeq = 0;

  // Scripture browse (Full panel, collapsible — FreeShow-inspired)
  let bibles = $state<BibleInfo[]>([]);
  let selectedBibleId = $state<string | null>(null);
  let bibleBooks = $state<string[]>([]);
  let selectedBook = $state<string | null>(null);
  let chapterNumbers = $state<number[]>([]);
  let selectedChapter = $state<number | null>(null);
  let chapterVerses = $state<ChapterVerse[]>([]);
  let browseCollapsed = $state(true);
  let browseLoading = $state(false);
  let browseError = $state<string | null>(null);
  let biblesFolder = $state<string>("");

  // Drag-and-drop state (native HTML5, no library)
  let draggedSlideId = $state<string | null>(null);
  let dragOverIndex = $state<number | null>(null);
  let isDragging = $state(false);
  let dragType = $state<string | null>(null);
  let dragPayload = $state<any>(null);

  // External OS file drag-and-drop — images/videos create new media slides
  let externalDragActive = $state(false);
  let externalDragError = $state<string | null>(null);
  const ALLOWED_EXTS = new Set([
    ...MEDIA_FILTERS[0].extensions,
    ...MEDIA_FILTERS[1].extensions,
  ]);

  // Library song-file drop — .pro / .cho / .usr (no cloud)
  let libraryDragActive = $state(false);
  let libraryDragError = $state<string | null>(null);
  const SONG_EXTS = new Set([
    "pro",
    "pro6",
    "pro5",
    "cho",
    "chopro",
    "chordpro",
    "chord",
    "usr",
    "txt",
  ]);

  // Add song modal (reusable Modal, replaces window.prompt "localhost:1420 says")
  let showAddSongTitleModal = $state(false);
  let showAddSongBodyModal = $state(false);
  let showSongEditor = $state(false);
  let pendingSongTitle = $state("");
  let pendingSongBody = $state("");

  // View Hub (Startup launcher)
  let showHub = $state(false);

  // Saved playlists (reusable slide sequences) — surfaced in the View Hub + Save-as-Playlist
  let templates = $state<PlaylistTemplate[]>([]);
  let showSavePlaylistModal = $state(false);

  // Global search (Ctrl/Cmd+K) — library + all Bibles + media cache
  let globalSearchOpen = $state(false);

  // Contextual onboarding — quiet inline hints + one-time guided tour.
  // All flags persist in localStorage (see lib/onboarding.ts); a hint hides
  // forever once its feature is used OR its × is clicked.
  let onboarding = $state(loadOnboarding());
  let tourActive = $state(false);
  let tourStep = $state(0);
  let tourAutoStarted = $state(false);
  let helpOpen = $state(false);

  const TOUR_STEPS = [
    {
      eyebrow: "Playlist",
      title: "Build your service here",
      body: "Add slides, drag songs or verses in, drag to reorder. Click a slide to put it live.",
    },
    {
      eyebrow: "Output",
      title: "Show it when you're ready",
      body: "Pick a display, then click Show Output. While live, ← and → move through slides.",
    },
    {
      eyebrow: "Songs & Scripture",
      title: "Your songs and verses live here",
      body: "Search Scripture, browse by book → chapter → verse, or pull a song into the playlist. Ctrl+K searches everything at once.",
    },
    {
      eyebrow: "You're set",
      title: "Ignore all of this on Sunday morning",
      body: "Nothing here blocks the app — set any slide live right now. The ? Help button replays this tour and lists shortcuts.",
    },
  ];

  /** Brand-new install only (no project/settings on disk), after the View Hub
      closes so the targets are visible. Never re-shows once dismissed. */
  $effect(() => {
    if (
      !tourAutoStarted &&
      !tourActive &&
      !showHub &&
      appState !== null &&
      appState.firstRun === true &&
      !onboarding.tourDismissed
    ) {
      tourAutoStarted = true;
      tourStep = 0;
      tourActive = true;
    }
  });

  const showTour = $derived(tourActive && !showHub && appState !== null);

  function use(feature: string): void {
    onboarding = markUsed(onboarding, feature);
  }

  function dismiss(key: string): void {
    onboarding = dismissHint(onboarding, key);
  }

  function endTour(): void {
    tourActive = false;
    onboarding = dismissTour(onboarding);
  }

  function replayTour(): void {
    onboarding = resetTourDismissal(onboarding);
    tourStep = 0;
    tourActive = true;
  }

  // Render-ack heartbeat (Phase 1 of the live-thumbnail research): the latest
  // per-window proof-of-life. Fresh = renderer applied state recently; stale =
  // early warning that Output/Stage silently stopped responding.
  let ack = $state<AckUpdate | null>(null);
  let nowMs = $state(Date.now());
  const ACK_STALE_MS = 12000;

  function ackAgeMs(at: string | null | undefined): number | null {
    if (!at) return null;
    const t = Date.parse(at);
    if (!Number.isFinite(t)) return null;
    return Math.max(0, nowMs - t);
  }

  function ackLabel(ageMs: number | null): string {
    if (ageMs === null) return "Waiting for confirmation…";
    if (ageMs < 1500) return "Confirmed just now";
    return `Confirmed ${Math.round(ageMs / 1000)}s ago`;
  }

  // Stage message (nursery alerts, countdowns, operator notes) — stage-only, never Output
  let stageMessageDraft = $state("");
  let stageMessageDuration = $state("");

  // Overlay for Output — independent lower-third / logo, stage_message-like but for main Output
  let overlayTextDraft = $state("");
  let overlayBackgroundDraft = $state<Background | null>(null);
  let overlayImporting = $state(false);

  // Draft copies for responsive editing — typing updates these immediately
  // while the backend save is debounced so the input never resets mid-keystroke.
  let draftName = $state("");
  let draftTitle = $state("");
  let draftBody = $state("");
  let draftAutoAdvance = $state("");
  let draftId: string | null = $state(null);
  let nameTimer: ReturnType<typeof setTimeout> | null = null;
  let titleTimer: ReturnType<typeof setTimeout> | null = null;
  let bodyTimer: ReturnType<typeof setTimeout> | null = null;
  let autoAdvanceTimer: ReturnType<typeof setTimeout> | null = null;

  function slideDisplayName(slide: Slide | null): string {
    if (!slide) return "Untitled";
    const n = (slide.name ?? "").trim();
    if (n) return n;
    const t = slide.title.trim();
    if (t) return t;
    return "Untitled";
  }

  // Grid/detail toggle — grid is primary workspace, detail is reached via click
  let showDetail = $state(false);
  let detailFromGrid = $state(false);

  // Central workspace view — slides (grid) vs looks (dedicated visual editor)
  let centralView = $state<"slides" | "looks">("slides");

  const project = $derived(appState?.project ?? null);
  const notice = $derived(
    appState?.notice && !noticeDismissed ? appState.notice : null,
  );
  const welcome = $derived(appState?.firstRun === true && !welcomeDismissed);
  const selected = $derived(
    project?.slides.find((s) => s.id === selectedId) ??
      project?.slides[0] ??
      null,
  );
  const librarySongs = $derived(
    (library?.songs ?? []).filter((song) =>
      song.title.toLowerCase().includes(librarySearch.trim().toLowerCase()),
    ),
  );

  // Output preview (reuses SlideRender at small size, no new backend) — near "Live on display" status
  const outputPreviewSlide = $derived.by(() => {
    if (!project) return null;
    const liveId = project.live;
    if (liveId) {
      const live = project.slides.find((s) => s.id === liveId);
      if (live) return live;
    }
    return selected;
  });
  const outputPreviewLook = $derived.by(() => {
    const looks = appState?.looks ?? [];
    if (looks.length === 0) return null;
    const mapped = looks.find((l) => l.id === appState?.outputLookId);
    if (mapped) return mapped;
    return looks.find((l) => l.name === "Main") ?? looks[0] ?? null;
  });
  const isOnAir = $derived(!!(appState?.output.visible && project?.live));

  // Stage preview (consistent treatment, straightforward)
  const stagePreviewSlide = $derived.by(() => {
    return appState?.current ?? selected;
  });
  const stagePreviewLook = $derived.by(() => {
    const looks = appState?.looks ?? [];
    if (looks.length === 0) return null;
    const mapped = looks.find((l) => l.id === appState?.stageLookId);
    if (mapped) return mapped;
    return looks.find((l) => l.name === "Stage") ?? looks[0] ?? null;
  });
  const isStageOnAir = $derived(!!appState?.stage.visible);

  // Keep draftName/draftTitle/draftBody/draftAutoAdvance in sync when selection changes — only when the
  // underlying slide identity changes, so mid-edit keystrokes are never clobbered.
  $effect(() => {
    const s = selected;
    if (!s) {
      draftName = "";
      draftTitle = "";
      draftBody = "";
      draftAutoAdvance = "";
      draftId = null;
      return;
    }
    if (draftId !== s.id) {
      draftId = s.id;
      draftName = s.name ?? "";
      draftTitle = s.title;
      draftBody = s.body;
      draftAutoAdvance = s.autoAdvanceSecs != null ? String(s.autoAdvanceSecs) : "";
      if (nameTimer) clearTimeout(nameTimer);
      if (titleTimer) clearTimeout(titleTimer);
      if (bodyTimer) clearTimeout(bodyTimer);
      if (autoAdvanceTimer) clearTimeout(autoAdvanceTimer);
    }
  });

  function formatAt(at?: string): string {
    if (!at) return "";
    const d = new Date(at);
    return d.toLocaleTimeString();
  }

  async function run(fn: () => Promise<ClientState>): Promise<void> {
    try {
      errorMsg = null;
      appState = await fn();
    } catch (e) {
      errorMsg = String(e);
    }
  }

  function goLive(slide: Slide): void {
    selectedId = slide.id;
    void run(() => api.setLiveSlide(slide.id));
  }

  function openDetail(slide: Slide): void {
    selectedId = slide.id;
    showDetail = true;
    detailFromGrid = true;
  }

  function closeDetail(): void {
    showDetail = false;
    detailFromGrid = false;
  }

  function clearOutput(): void {
    void run(() => api.clearOutput());
  }

  function clearText(): void {
    void api
      .clearText()
      .then((s) => (appState = s))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function clearBackground(): void {
    void api
      .clearBackground()
      .then((s) => (appState = s))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  async function addSlide(): Promise<void> {
    try {
      errorMsg = null;
      welcomeDismissed = true;
      const s = await api.addSlide("New Slide", "");
      appState = s;
      const created = s.project.slides.at(-1);
      if (created) {
        selectedId = created.id;
        appState = await api.setLiveSlide(created.id);
      }
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function doScriptureSearch(q: string): Promise<void> {
    const seq = ++scriptureSeq;
    if (!q.trim()) {
      scriptureResults = [];
      scriptureOpen = false;
      scriptureIdx = -1;
      scriptureLoading = false;
      return;
    }
    try {
      const matches = await api.searchScripture(q.trim());
      if (seq !== scriptureSeq) return;
      scriptureResults = matches;
      scriptureOpen = matches.length > 0;
      scriptureIdx = -1;
    } catch (e) {
      if (seq !== scriptureSeq) return;
      scriptureResults = [];
      scriptureOpen = false;
      errorMsg = String(e);
    } finally {
      if (seq === scriptureSeq) scriptureLoading = false;
    }
  }

  function looksLikeReference(q: string): boolean {
    return /\d/.test(q.trim());
  }

  async function fetchApiScripture(q: string): Promise<void> {
    scriptureBusy = true;
    scriptureStatus = null;
    scriptureLoading = true;
    try {
      const matches = await api.lookupApiScripture(q.trim());
      scriptureResults = matches;
      scriptureOpen = matches.length > 0;
      scriptureIdx = -1;
      if (matches.length === 0) {
        scriptureStatus = "bible-api.com returned no verses for that reference.";
      }
    } catch (e) {
      scriptureResults = [];
      scriptureOpen = false;
      errorMsg = String(e);
    } finally {
      scriptureBusy = false;
      scriptureLoading = false;
    }
  }

  async function importOpenlpFile(): Promise<void> {
    try {
      errorMsg = null;
      scriptureStatus = null;
      const picked = await open({
        multiple: false,
        filters: [
          { name: "OpenLP / Zefania Bible XML", extensions: ["xml"] },
        ],
      });
      if (!picked || Array.isArray(picked)) return;
      scriptureBusy = true;
      const result = await api.importOpenlpBible(picked);
      scriptureStatus = `Imported ${result.books} books (${result.verses} verses). ${result.totalBooks} books searchable.`;
      if (scriptureQuery.trim()) {
        await doScriptureSearch(scriptureQuery);
      }
    } catch (e) {
      errorMsg = String(e);
    } finally {
      scriptureBusy = false;
    }
  }

  function onScriptureInput(e: Event): void {
    const value = (e.target as HTMLInputElement).value;
    scriptureQuery = value;
    scriptureIdx = -1;
    if (scriptureTimer) clearTimeout(scriptureTimer);
    if (!value.trim()) {
      scriptureResults = [];
      scriptureOpen = false;
      scriptureLoading = false;
      return;
    }
    scriptureLoading = true;
    scriptureTimer = setTimeout(() => {
      void doScriptureSearch(value);
    }, 150);
  }

  function selectScripture(match: ScriptureMatch): void {
    scriptureQuery = "";
    scriptureResults = [];
    scriptureOpen = false;
    scriptureIdx = -1;
    void run(() => api.addSlide(match.reference, match.text));
  }

  function onScriptureKeydown(e: KeyboardEvent): void {
    if (!scriptureOpen || scriptureResults.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      scriptureIdx = (scriptureIdx + 1) % scriptureResults.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      scriptureIdx =
        scriptureIdx <= 0 ? scriptureResults.length - 1 : scriptureIdx - 1;
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (scriptureIdx >= 0 && scriptureIdx < scriptureResults.length) {
        selectScripture(scriptureResults[scriptureIdx]);
      }
    } else if (e.key === "Escape") {
      scriptureOpen = false;
      scriptureIdx = -1;
    }
  }

  // Scripture browse — FreeShow-inspired full panel
  async function loadBibles(): Promise<void> {
    try {
      browseError = null;
      const list = await api.listBibles();
      bibles = list;
      if (!selectedBibleId && list.length > 0) {
        selectedBibleId = list[0].id;
        await loadBooks(selectedBibleId);
      }
    } catch (e) {
      browseError = String(e);
    }
  }

  async function loadBooks(bibleId: string): Promise<void> {
    try {
      browseLoading = true;
      browseError = null;
      bibleBooks = await api.getBookList(bibleId);
      selectedBook = null;
      chapterNumbers = [];
      selectedChapter = null;
      chapterVerses = [];
    } catch (e) {
      browseError = String(e);
    } finally {
      browseLoading = false;
    }
  }

  async function loadChaptersForBook(bibleId: string, book: string): Promise<void> {
    selectedBook = book;
    selectedChapter = null;
    chapterVerses = [];
    browseLoading = true;
    browseError = null;
    try {
      const nums = await api.listChapters(bibleId, book);
      chapterNumbers = nums;
    } catch (e) {
      browseError = String(e);
      chapterNumbers = [];
    } finally {
      browseLoading = false;
    }
  }

  async function loadChapterVerses(bibleId: string, book: string, chapter: number): Promise<void> {
    try {
      browseLoading = true;
      browseError = null;
      const verses = await api.getChapter(bibleId, book, chapter);
      chapterVerses = verses;
      selectedChapter = chapter;
      use("browse");
    } catch (e) {
      browseError = String(e);
      chapterVerses = [];
    } finally {
      browseLoading = false;
    }
  }

  function onBrowseBibleChange(e: Event): void {
    const id = (e.target as HTMLSelectElement).value;
    selectedBibleId = id;
    void loadBooks(id);
  }

  function onBrowseBookSelect(book: string): void {
    if (!selectedBibleId) return;
    void loadChaptersForBook(selectedBibleId, book);
  }

  function onBrowseChapterSelect(ch: number): void {
    if (!selectedBibleId || !selectedBook) return;
    void loadChapterVerses(selectedBibleId, selectedBook, ch);
  }

  function insertBrowseVerse(v: ChapterVerse): void {
    if (!selectedBook || selectedChapter == null) return;
    const ref = `${selectedBook} ${selectedChapter}:${v.verse}`;
    use("browse");
    void run(() => api.addSlide(ref, v.text));
  }

  // Drag-and-drop — native HTML5, no library
  function onPlaylistDragStart(e: DragEvent, slide: Slide, index: number): void {
    draggedSlideId = slide.id;
    dragType = "playlist-reorder";
    isDragging = true;
    dragPayload = { type: "playlist-reorder", slideId: slide.id, fromIndex: index };
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", JSON.stringify(dragPayload));
      // Use transparent drag image for cleaner indicator
    }
    // Add dragging class via data attribute
    (e.currentTarget as HTMLElement).classList.add("dragging");
  }

  function onPlaylistDragOver(e: DragEvent, index: number): void {
    // External OS files take precedence — show copy cursor even though isDragging is false
    if (isExternalFileDrag(e)) {
      handleExternalDragOver(e, index);
      return;
    }
    e.preventDefault();
    if (!isDragging) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    if (rect.height === 0) return;
    const mid = rect.top + rect.height / 2;
    const overIndex = e.clientY < mid ? index : index + 1;
    dragOverIndex = overIndex;
    if (e.dataTransfer) e.dataTransfer.dropEffect = dragType === "playlist-reorder" ? "move" : "copy";
  }

  function onPlaylistDragLeave(e: DragEvent): void {
    // Only clear if leaving the list container, not a child
    const related = e.relatedTarget as HTMLElement | null;
    if (!related || !(e.currentTarget as HTMLElement).contains(related)) {
      // Keep indicator if still over playlist, otherwise clear on drop
    }
  }

  function onPlaylistDrop(e: DragEvent, dropIndex?: number): void {
    e.preventDefault();
    e.stopPropagation();
    // External OS files — use the existing media import pipeline (hash+copy+thumb)
    // and create a new slide with that file as the background, same result as
    // the “Add media” button. Must not silently fail on unsupported types.
    if ((e.dataTransfer?.files?.length ?? 0) > 0) {
      const target = dropIndex ?? dragOverIndex ?? project?.slides.length ?? 0;
      void handleExternalFiles(e.dataTransfer!.files, target);
      dragOverIndex = null;
      isDragging = false;
      externalDragActive = false;
      return;
    }
    if (!project) {
      errorMsg = "View not loaded yet";
      dragOverIndex = null;
      isDragging = false;
      return;
    }
    let targetIdx = dropIndex ?? dragOverIndex ?? project.slides.length;
    const len = project.slides.length;
    targetIdx = Math.max(0, Math.min(targetIdx, len));
    const raw = e.dataTransfer?.getData("text/plain");
    let payload: any = null;
    try { payload = raw ? JSON.parse(raw) : dragPayload; } catch { payload = dragPayload; }

    if (!payload || !payload.type) {
      if (!payload) errorMsg = "Drop failed: unknown payload";
      dragOverIndex = null;
      isDragging = false;
      return;
    }
    // Any recognised drop onto the playlist counts as "used drag-and-drop" —
    // the inline hint hides itself from here on.
    use("dragdrop");

    if (payload.type === "playlist-reorder" && payload.slideId) {
      const fromIdx = project.slides.findIndex((s) => s.id === payload.slideId);
      if (fromIdx !== -1 && fromIdx !== targetIdx && targetIdx !== fromIdx + 1) {
        const ids = project.slides.map((s) => s.id);
        const [moved] = ids.splice(fromIdx, 1);
        if (moved === undefined) return;
        const insertAt = targetIdx > fromIdx ? targetIdx - 1 : targetIdx;
        ids.splice(insertAt, 0, moved);
        void api.reorderSlides(ids).then((s) => (appState = s)).catch((err: unknown) => (errorMsg = String(err)));
      }
    } else if (payload.type === "library-song" && payload.songId) {
      void (async () => {
        try {
          const beforeLen = project.slides.length;
          const s = await api.addSongToPlaylist(payload.songId);
          appState = s;
          if (targetIdx < beforeLen) {
            const addedCount = s.project.slides.length - beforeLen;
            if (addedCount <= 0) return;
            const ids = s.project.slides.map((x) => x.id);
            const newIds = ids.slice(-addedCount);
            if (newIds.length === 0) return;
            const remaining = ids.slice(0, -addedCount);
            remaining.splice(targetIdx, 0, ...newIds);
            const s2 = await api.reorderSlides(remaining);
            appState = s2;
          }
        } catch (err: unknown) { errorMsg = String(err); }
      })();
    } else if (payload.type === "library-verse" && payload.songId && payload.slideId) {
      const song = library?.songs.find((x) => x.id === payload.songId);
      let verse: any | undefined;
      if (song) {
        if (song.blocks && Object.keys(song.blocks).length > 0) {
          verse = Object.values(song.blocks).find((x: any) => x.id === payload.slideId);
        } else {
          verse = (song as any).slides?.find((x: any) => x.id === payload.slideId);
        }
      }
      if (!verse) {
        errorMsg = "Verse not found";
        return;
      }
      void (async () => {
        try {
          const s = await api.addSlide(verse.title, verse.body);
          appState = s;
          if (targetIdx < (s.project.slides.length - 1)) {
            const ids = s.project.slides.map((x) => x.id);
            const moved = ids.pop();
            if (!moved) return;
            ids.splice(targetIdx, 0, moved);
            const s2 = await api.reorderSlides(ids);
            appState = s2;
          }
        } catch (err: unknown) { errorMsg = String(err); }
      })();
    } else if (payload.type === "scripture" && payload.reference && payload.text !== undefined) {
      void (async () => {
        try {
          const s = await api.addSlide(payload.reference, payload.text);
          appState = s;
          const newId = s.project.slides.at(-1)?.id;
          if (newId && targetIdx < s.project.slides.length - 1) {
            const ids = s.project.slides.map((x) => x.id);
            ids.splice(ids.indexOf(newId), 1);
            ids.splice(targetIdx, 0, newId);
            const s2 = await api.reorderSlides(ids);
            appState = s2;
          }
        } catch (err: unknown) { errorMsg = String(err); }
      })();
    }

    dragOverIndex = null;
    isDragging = false;
    draggedSlideId = null;
    dragType = null;
    dragPayload = null;
  }

  function onPlaylistDragEnd(e: DragEvent): void {
    (e.currentTarget as HTMLElement).classList.remove("dragging");
    dragOverIndex = null;
    isDragging = false;
    draggedSlideId = null;
    dragType = null;
    dragPayload = null;
  }

  function onLibrarySongDragStart(e: DragEvent, song: LibrarySong): void {
    isDragging = true;
    dragType = "library-song";
    dragPayload = { type: "library-song", songId: song.id };
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "copy";
      e.dataTransfer.setData("text/plain", JSON.stringify(dragPayload));
    }
  }

  function onLibraryVerseDragStart(e: DragEvent, song: LibrarySong, verse: { id: string }): void {
    isDragging = true;
    dragType = "library-verse";
    dragPayload = { type: "library-verse", songId: song.id, slideId: verse.id };
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "copy";
      e.dataTransfer.setData("text/plain", JSON.stringify(dragPayload));
    }
  }

  function onScriptureDragStart(e: DragEvent, ref: string, text: string): void {
    isDragging = true;
    dragType = "scripture";
    dragPayload = { type: "scripture", reference: ref, text };
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "copy";
      e.dataTransfer.setData("text/plain", JSON.stringify(dragPayload));
    }
  }

  // — External OS file drag-and-drop helpers (image/video → new slide with media background) —
  function isExternalFileDrag(e: DragEvent): boolean {
    const types = Array.from(e.dataTransfer?.types ?? []);
    if (types.includes("Files")) return true;
    return (e.dataTransfer?.files?.length ?? 0) > 0;
  }

  function getFileExt(name: string): string {
    const dot = name.lastIndexOf(".");
    return dot >= 0 ? name.slice(dot + 1).toLowerCase() : "";
  }

  async function handleExternalFiles(
    files: FileList | File[] | string[],
    targetIdx: number,
  ): Promise<void> {
    const asArray = Array.isArray(files) ? files : Array.from(files as FileList);
    const paths: string[] = [];
    const unsupported: string[] = [];
    for (const entry of asArray as any[]) {
      // Tauri file-drop gives string paths; HTML5 DataTransfer gives File with .path
      if (typeof entry === "string") {
        const name = entry.split(/[\/\\]/).pop() ?? entry;
        const ext = getFileExt(name);
        if (!ALLOWED_EXTS.has(ext)) unsupported.push(name);
        else paths.push(entry);
        continue;
      }
      const f = entry as File & { path?: string };
      const name: string = (f as any).name ?? "file";
      const ext = getFileExt(name);
      if (!ALLOWED_EXTS.has(ext)) {
        unsupported.push(name);
        continue;
      }
      const p: string | undefined = (f as any).path;
      if (p && p.length > 0) paths.push(p);
      else {
        // No filesystem path exposed — fall back to name, will be rejected clearly by backend
        // but surface an inline message here so it isn't silent.
        unsupported.push(`${name} (no filesystem path — use Add media button)`);
      }
    }

    if (unsupported.length > 0) {
      const msg = `Unsupported file type: ${unsupported.join(", ")}. Supported: ${Array.from(ALLOWED_EXTS).join(", ")}`;
      errorMsg = msg;
      externalDragError = msg;
      setTimeout(() => (externalDragError = null), 6000);
      if (paths.length === 0) return;
    } else {
      externalDragError = null;
    }

    if (!project) {
      errorMsg = "View not loaded yet";
      return;
    }
    const len = project.slides.length;
    let insertIdx = Math.max(0, Math.min(targetIdx, len));
    importingMedia = true;
    try {
      for (const p of paths) {
        try {
          const asset = await api.importMedia(p);
          const base = p.split(/[\/\\]/).pop()?.replace(/\.[^/.]+$/, "") ?? "Media";
          const createdState = await api.addSlide(base, "");
          // last slide is the newly created one
          const newId = createdState.project.slides.at(-1)?.id;
          if (!newId) throw new Error("failed to create slide for media");
          const updated = await api.updateSlide(newId, {
            background: asset.background,
          });
          appState = updated;
          // Move to insertion point if not already at end
          if (insertIdx < (appState?.project.slides.length ?? 1) - 1) {
            const ids = appState!.project.slides.map((x) => x.id);
            const moved = ids.pop();
            if (moved) {
              ids.splice(insertIdx, 0, moved);
              const reordered = await api.reorderSlides(ids);
              appState = reordered;
            }
          }
          insertIdx++;
          errorMsg = null;
          use("dragdrop");
        } catch (err) {
          errorMsg = String(err);
        }
      }
    } finally {
      importingMedia = false;
      dragOverIndex = null;
      isDragging = false;
      externalDragActive = false;
      dragPayload = null;
      draggedSlideId = null;
      dragType = null;
    }
  }

  function handleExternalDragOver(e: DragEvent, index?: number): void {
    if (!isExternalFileDrag(e)) return;
    e.preventDefault();
    externalDragActive = true;
    if (typeof index === "number") {
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      if (rect.height > 0) {
        const mid = rect.top + rect.height / 2;
        dragOverIndex = e.clientY < mid ? index : index + 1;
      } else {
        dragOverIndex = index;
      }
    } else {
      // hovering the list container — will resolve to end
      if (dragOverIndex === null) dragOverIndex = project?.slides.length ?? 0;
    }
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
  }

  function handleExternalDragLeave(e: DragEvent): void {
    const related = e.relatedTarget as HTMLElement | null;
    if (!related || !(e.currentTarget as HTMLElement).contains(related)) {
      externalDragActive = false;
      // Don't clear dragOverIndex here — leave it for the next dragover to recompute
    }
  }

  // — Library song-file drag-and-drop (.pro / .cho / .usr) — no cloud —
  function isSongFileDrag(e: DragEvent): boolean {
    const types = Array.from(e.dataTransfer?.types ?? []);
    if (types.includes("Files")) {
      // Check at least one file looks like a song file; if we can't inspect names yet, assume Files drag might be song
      // For dragover we conservatively return true when Files is present — actual filtering happens on drop
      return true;
    }
    return (e.dataTransfer?.files?.length ?? 0) > 0;
  }

  function handleLibraryDragOver(e: DragEvent): void {
    // Allow any Files drag over Library — we validate extensions on drop and report clearly if unsupported
    if (!Array.from(e.dataTransfer?.types ?? []).includes("Files") && (e.dataTransfer?.files?.length ?? 0) === 0) return;
    e.preventDefault();
    libraryDragActive = true;
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
  }

  function handleLibraryDragLeave(e: DragEvent): void {
    const related = e.relatedTarget as HTMLElement | null;
    if (!related || !(e.currentTarget as HTMLElement).contains(related)) {
      libraryDragActive = false;
    }
  }

  async function handleLibraryFiles(files: FileList | File[] | string[]): Promise<void> {
    const asArray = Array.isArray(files) ? files : Array.from(files as FileList);
    const songPaths: string[] = [];
    const unsupported: string[] = [];
    for (const entry of asArray as any[]) {
      let name: string;
      let path: string | undefined;
      if (typeof entry === "string") {
        path = entry;
        name = entry.split(/[\/\\]/).pop() ?? entry;
      } else {
        const f = entry as File & { path?: string };
        name = (f as any).name ?? "file";
        path = (f as any).path;
        if (!path) {
          unsupported.push(`${name} (no filesystem path — use the Library drop zone from the desktop)`);
          continue;
        }
      }
      const ext = getFileExt(name);
      if (!SONG_EXTS.has(ext)) {
        // Also allow .txt that may be USR/CHO — SONG_EXTS includes txt, so this is truly unsupported
        unsupported.push(name);
        continue;
      }
      if (path) songPaths.push(path);
    }
    if (unsupported.length > 0) {
      const msg = `Unsupported file type: ${unsupported.join(", ")}. Supported: ${Array.from(SONG_EXTS).join(", ")} (ProPresenter .pro, ChordPro .cho, CCLI USR .usr/.txt)`;
      errorMsg = msg;
      libraryDragError = msg;
      setTimeout(() => (libraryDragError = null), 6000);
      if (songPaths.length === 0) {
        libraryDragActive = false;
        return;
      }
    } else {
      libraryDragError = null;
    }
    if (songPaths.length === 0) {
      libraryDragActive = false;
      return;
    }
    // Import sequentially so errors are reported per-file and library state stays consistent
    for (const p of songPaths) {
      try {
        const lib = await api.importSongFile(p);
        library = lib;
        errorMsg = null;
        libraryDragError = null;
        use("songs");
      } catch (err) {
        const msg = String(err);
        errorMsg = msg;
        libraryDragError = msg;
        setTimeout(() => (libraryDragError = null), 6000);
      }
    }
    libraryDragActive = false;
  }

  function commitName(id: string, value: string): void {
    if (draftId !== id || !selected || selected.id !== id) return;
    void api
      .updateSlide(id, { name: value })
      .then((s) => (appState = s))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function commitTitle(id: string, value: string): void {
    if (draftId !== id || !selected || selected.id !== id) return;
    void api
      .updateSlide(id, { title: value })
      .then((s) => (appState = s))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function commitBody(id: string, value: string): void {
    if (draftId !== id || !selected || selected.id !== id) return;
    void api
      .updateSlide(id, { body: value })
      .then((s) => (appState = s))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function commitAutoAdvance(id: string, value: string): void {
    if (draftId !== id || !selected || selected.id !== id) return;
    const trimmed = value.trim();
    if (trimmed === "") {
      void api
        .updateSlide(id, { autoAdvanceSecs: null })
        .then((s) => (appState = s))
        .catch((e: unknown) => (errorMsg = String(e)));
      return;
    }
    const n = Number(trimmed);
    if (!Number.isFinite(n) || n < 1 || n > 86400) {
      errorMsg = "Auto-advance must be a number between 1 and 86400 seconds (blank = off)";
      return;
    }
    void api
      .updateSlide(id, { autoAdvanceSecs: Math.floor(n) })
      .then((s) => (appState = s))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  // Lightweight title-case: "amazing grace - how sweet the sound" -> "Amazing Grace - How Sweet the Sound"
  // Keeps small words lowercased unless first word. Handles hyphen/apostrophe.
  function toTitleCase(s: string): string {
    const small = new Set([
      "a", "an", "and", "as", "at", "but", "by", "for", "if", "in", "nor", "of", "on", "or", "per", "the", "to", "vs", "via",
    ]);
    return s
      .trim()
      .split(/\s+/)
      .map((w, i) => {
        if (!w) return w;
        const lower = w.toLowerCase();
        if (i !== 0 && small.has(lower)) return lower;
        // Preserve hyphen/apostrophe capitalisation: "o'neill" -> "O'Neill", "self-giving" -> "Self-Giving"
        return lower
          .split("-")
          .map((part) =>
            part
              .split("'")
              .map((p) => (p ? p.charAt(0).toUpperCase() + p.slice(1) : p))
              .join("'"),
          )
          .join("-");
      })
      .join(" ");
  }

  function applyTitleCase(): void {
    if (!draftId || !selected || selected.id !== draftId) return;
    const formatted = toTitleCase(draftTitle);
    if (formatted === draftTitle) return;
    draftTitle = formatted;
    if (titleTimer) {
      clearTimeout(titleTimer);
      titleTimer = null;
    }
    commitTitle(draftId, formatted);
  }

  function onNameInput(value: string): void {
    draftName = value;
    if (!draftId) return;
    const id = draftId;
    if (nameTimer) clearTimeout(nameTimer);
    nameTimer = setTimeout(() => commitName(id, value), 180);
  }

  function flushName(): void {
    if (nameTimer) clearTimeout(nameTimer);
    nameTimer = null;
    if (draftId) commitName(draftId, draftName);
  }

  function onTitleInput(value: string): void {
    draftTitle = value;
    if (!draftId) return;
    const id = draftId;
    if (titleTimer) clearTimeout(titleTimer);
    titleTimer = setTimeout(() => commitTitle(id, value), 180);
  }

  function onBodyInput(value: string): void {
    draftBody = value;
    if (!draftId) return;
    const id = draftId;
    if (bodyTimer) clearTimeout(bodyTimer);
    bodyTimer = setTimeout(() => commitBody(id, value), 180);
  }

  function flushTitle(): void {
    if (titleTimer) clearTimeout(titleTimer);
    titleTimer = null;
    if (draftId) commitTitle(draftId, draftTitle);
  }

  function flushBody(): void {
    if (bodyTimer) clearTimeout(bodyTimer);
    bodyTimer = null;
    if (draftId) commitBody(draftId, draftBody);
  }

  function onAutoAdvanceInput(value: string): void {
    draftAutoAdvance = value;
    if (!draftId) return;
    const id = draftId;
    if (autoAdvanceTimer) clearTimeout(autoAdvanceTimer);
    autoAdvanceTimer = setTimeout(() => commitAutoAdvance(id, value), 350);
  }

  function flushAutoAdvance(): void {
    if (autoAdvanceTimer) clearTimeout(autoAdvanceTimer);
    autoAdvanceTimer = null;
    if (draftId) commitAutoAdvance(draftId, draftAutoAdvance);
  }

  function setColor(slide: Slide, color: string): void {
    void run(() =>
      api.updateSlide(slide.id, { background: { type: "solid", color } }),
    );
  }

  function fileUrl(path: string): string {
    if (!path) return "";
    try {
      return convertFileSrc(path);
    } catch {
      return "";
    }
  }

  // Pick an image/video, import it into the managed media cache, then assign
  // it as the slide's background (replacing the solid color, not layering).
  async function addMedia(slide: Slide): Promise<void> {
    try {
      errorMsg = null;
      const picked = await open({ multiple: false, filters: MEDIA_FILTERS });
      if (typeof picked !== "string") return;
      importingMedia = true;
      try {
        const asset = await api.importMedia(picked);
        appState = await api.updateSlide(slide.id, {
          background: asset.background,
        });
      } finally {
        importingMedia = false;
      }
    } catch (e) {
      importingMedia = false;
      errorMsg = String(e);
    }
  }

  // Live camera picker — enumerate UVC webcams / capture cards via the
  // webview's media APIs. No Rust involvement; the slide stores only which
  // device to use ({type:'live_camera', deviceId, label}).
  let showCameraPicker = $state(false);
  let cameraDevices = $state<MediaDeviceInfo[]>([]);
  let cameraLoading = $state(false);
  let cameraError = $state<string | null>(null);

  async function refreshCameras(): Promise<void> {
    cameraLoading = true;
    cameraError = null;
    try {
      if (!navigator.mediaDevices?.enumerateDevices) {
        throw new Error("unsupported");
      }
      const devices = await navigator.mediaDevices.enumerateDevices();
      cameraDevices = devices.filter((d) => d.kind === "videoinput");
      if (cameraDevices.length === 0) {
        cameraError =
          "No cameras found — plug in a webcam or capture card, then Refresh.";
      }
    } catch {
      cameraError = "Camera enumeration is unavailable in this window.";
      cameraDevices = [];
    } finally {
      cameraLoading = false;
    }
  }

  function openCameraPicker(): void {
    showCameraPicker = true;
    void refreshCameras();
  }

  // Browsers hide device labels until camera permission is granted — one
  // explicit gesture triggers the prompt, then we re-enumerate for names.
  async function enableCameraAccess(): Promise<void> {
    cameraLoading = true;
    cameraError = null;
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        video: true,
        audio: false,
      });
      stream.getTracks().forEach((t) => t.stop());
      await refreshCameras();
    } catch {
      cameraError =
        "Camera access denied — allow camera access for MakrStudio, then try again.";
    } finally {
      cameraLoading = false;
    }
  }

  function selectCamera(d: MediaDeviceInfo): void {
    if (!selected) return;
    showCameraPicker = false;
    void run(() =>
      api.updateSlide(selected.id, {
        background: {
          type: "live_camera",
          deviceId: d.deviceId || null,
          label: d.label || "Camera",
        },
      }),
    );
  }

  function deleteSlide(slide: Slide): void {
    const wasSelected = selectedId === slide.id;
    void (async () => {
      try {
        errorMsg = null;
        appState = await api.deleteSlide(slide.id);
        if (wasSelected) selectedId = null;
      } catch (e) {
        errorMsg = String(e);
      }
    })();
  }

  function onDisplayChange(e: Event): void {
    const target = e.target as HTMLSelectElement;
    if (target.value === "") return;
    const index = Number(target.value);
    if (!Number.isFinite(index)) return;
    void api
      .setOutputDisplay(index)
      .then((d) => (displays = d))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function onStageDisplayChange(e: Event): void {
    const target = e.target as HTMLSelectElement;
    if (target.value === "") return;
    const index = Number(target.value);
    if (!Number.isFinite(index)) return;
    void api
      .setStageDisplay(index)
      .then((d) => (displays = d))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function onTransitionChange(e: Event): void {
    const value = (e.target as HTMLSelectElement).value as "cut" | "fade";
    void api
      .setTransition(value)
      .then((s) => (appState = s))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function toggleStage(): void {
    use("stage");
    void api.toggleStage().catch((e: unknown) => (errorMsg = String(e)));
  }

  function addToPlaylist(song: LibrarySong): void {
    use("songs");
    void api
      .addSongToPlaylist(song.id)
      .then((s) => (appState = s))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function addLibrarySong(): void {
    pendingSongTitle = "";
    showAddSongTitleModal = true;
  }

  function handleAddSongTitleConfirm(title: string): void {
    const trimmed = title.trim();
    if (!trimmed) return;
    pendingSongTitle = trimmed;
    showAddSongTitleModal = false;
    pendingSongBody = "";
    showSongEditor = true;
  }

  function handleAddSongBodyConfirm(body: string): void {
    const title = pendingSongTitle;
    showAddSongBodyModal = false;
    pendingSongTitle = "";
    use("songs");
    void api
      .addLibrarySong(title, body)
      .then((l) => (library = l))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function handleSongEditorConfirm(
    title: string,
    slides: { title: string; body: string; positioning?: { vAlign: string; hAlign: string }; groupId?: string; groupLabel?: string }[],
  ): void {
    showSongEditor = false;
    pendingSongTitle = "";
    pendingSongBody = "";
    use("songs");
    void api
      .addLibrarySong(title, undefined, undefined, slides as any)
      .then((l) => (library = l))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function handleSongEditorBack(): void {
    showSongEditor = false;
    showAddSongTitleModal = true;
  }

  function handleAddSongCancel(): void {
    showAddSongTitleModal = false;
    showAddSongBodyModal = false;
    showSongEditor = false;
    pendingSongTitle = "";
    pendingSongBody = "";
  }

  function deleteSong(song: LibrarySong): void {
    void api
      .deleteLibrarySong(song.id)
      .then((l) => (library = l))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  // Master-block helpers — library.json now stores blocks + arrangement (v2)
  function getSongBlockCount(song: LibrarySong): number {
    if (song.blocks && Object.keys(song.blocks).length > 0) return Object.keys(song.blocks).length;
    return song.slides?.length ?? 0;
  }
  function getSongArrangementCount(song: LibrarySong): number {
    if (song.arrangement && song.arrangement.length > 0) return song.arrangement.length;
    return getSongBlockCount(song);
  }
  function getBlocksArray(song: LibrarySong): import("../lib/types").LibrarySlide[] {
    if (song.blocks && Object.keys(song.blocks).length > 0) return Object.values(song.blocks);
    return song.slides ?? [];
  }
  async function setArrangement(song: LibrarySong, newArr: string[]): Promise<void> {
    try {
      errorMsg = null;
      const lib = await api.setSongArrangement(song.id, newArr);
      library = lib;
    } catch (e) {
      errorMsg = String(e);
    }
  }
  function moveArrangement(song: LibrarySong, from: number, to: number): void {
    const arr = [...(song.arrangement ?? [])];
    if (from < 0 || from >= arr.length || to < 0 || to >= arr.length) return;
    const [moved] = arr.splice(from, 1);
    arr.splice(to, 0, moved);
    void setArrangement(song, arr);
  }
  function duplicateArrangement(song: LibrarySong, idx: number): void {
    const arr = [...(song.arrangement ?? [])];
    if (idx < 0 || idx >= arr.length) return;
    arr.splice(idx + 1, 0, arr[idx]);
    void setArrangement(song, arr);
  }
  function removeFromArrangement(song: LibrarySong, idx: number): void {
    const arr = [...(song.arrangement ?? [])];
    if (arr.length <= 1) {
      errorMsg = "Arrangement must have at least one block";
      return;
    }
    arr.splice(idx, 1);
    void setArrangement(song, arr);
  }
  function addBlockToArrangement(song: LibrarySong, key: string): void {
    if (!key) return;
    const arr = [...(song.arrangement ?? []), key];
    void setArrangement(song, arr);
  }

  function toggleFullscreen(): void {
    void api.toggleOutputFullscreen().catch((e: unknown) => (errorMsg = String(e)));
  }

  function showOutput(): void {
    void api
      .showOutput()
      .then((s) => (appState = s))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function onOutputLookChange(e: Event): void {
    const value = (e.target as HTMLSelectElement).value;
    void api
      .setOutputLook(value === "" ? null : value)
      .then((s) => (appState = s))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function onStageLookChange(e: Event): void {
    const value = (e.target as HTMLSelectElement).value;
    void api
      .setStageLook(value === "" ? null : value)
      .then((s) => (appState = s))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function openHub(): void {
    showHub = true;
    void refreshPlaylists();
  }

  async function handleHubCreate(
    presetId: string,
    opts: { title: string; aspect: string; theme: string; transition: string },
  ): Promise<void> {
    try {
      errorMsg = null;
      welcomeDismissed = true;
      appState = await api.newProjectFromPreset(presetId, opts.title, opts.aspect, opts.theme, opts.transition);
      selectedId = appState.project.slides[0]?.id ?? null;
      showHub = false;
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function newProject(): Promise<void> {
    // Legacy fallback — now routed through hub
    openHub();
  }

  // View Hub — create from a saved playlist (blank view + load, reusing existing backend)
  async function handleHubCreateFromPlaylist(
    playlistId: string,
    opts: { title: string; aspect: string; theme: string; transition: string },
  ): Promise<void> {
    try {
      errorMsg = null;
      welcomeDismissed = true;
      // Create a blank view shell, then populate it from the saved playlist.
      appState = await api.newProjectFromPreset("blank", opts.title, opts.aspect, opts.theme, opts.transition);
      appState = await api.loadTemplate(playlistId);
      const slides = appState.project.slides;
      selectedId = slides[0]?.id ?? null;
      showHub = false;
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function refreshPlaylists(): Promise<void> {
    try {
      errorMsg = null;
      templates = await api.listTemplates();
    } catch (e) {
      errorMsg = String(e);
    }
  }

  // Save-as-Playlist (reuse existing save_template backend — the "save it for next service" loop)
  function openSavePlaylist(): void {
    showSavePlaylistModal = true;
  }

  async function handleSavePlaylistConfirm(name: string): Promise<void> {
    const trimmed = name.trim();
    if (!trimmed) return;
    try {
      errorMsg = null;
      templates = await api.saveTemplate(trimmed);
      showSavePlaylistModal = false;
    } catch (e) {
      errorMsg = String(e);
    }
  }

  function isTextInputFocused(): boolean {
    const el = document.activeElement as HTMLElement | null;
    if (!el) return false;
    const tag = el.tagName.toLowerCase();
    if (tag === "input" || tag === "textarea" || tag === "select") return true;
    if (el.isContentEditable) return true;
    return false;
  }

  function handleGlobalKeydown(e: KeyboardEvent): void {
    const isK = e.key.toLowerCase() === "k";
    const mod = e.ctrlKey || e.metaKey;
    if (mod && isK) {
      e.preventDefault();
      globalSearchOpen = !globalSearchOpen;
      if (globalSearchOpen) use("shortcuts");
      return;
    }
    if (e.key === "Escape" && globalSearchOpen) {
      e.preventDefault();
      globalSearchOpen = false;
      return;
    }
    if (e.key === "Escape" && tourActive) {
      e.preventDefault();
      endTour();
      return;
    }
    if ((e.key === "ArrowRight" || e.key === "ArrowLeft") && !isTextInputFocused()) {
      // Reuse existing next/previous logic — same path as triggers/UI clicks
      if (e.key === "ArrowRight") {
        e.preventDefault();
        use("shortcuts");
        void api.nextSlide().then((s) => (appState = s)).catch((err: unknown) => (errorMsg = String(err)));
      } else {
        e.preventDefault();
        use("shortcuts");
        void api.prevSlide().then((s) => (appState = s)).catch((err: unknown) => (errorMsg = String(err)));
      }
    }
  }

  async function sendStageMessage(): Promise<void> {
    const msg = stageMessageDraft.trim();
    if (!msg) {
      errorMsg = "Stage message must not be empty";
      return;
    }
    let dur: number | null = null;
    const d = stageMessageDuration.trim();
    if (d !== "") {
      const n = Number(d);
      if (!Number.isFinite(n) || n < 1 || n > 3600) {
        errorMsg = "Duration must be 1–3600 seconds or blank (manual clear)";
        return;
      }
      dur = Math.floor(n);
    }
    try {
      errorMsg = null;
      use("stage");
      appState = await api.setStageMessage(msg, dur);
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function clearStageMessage(): Promise<void> {
    try {
      errorMsg = null;
      appState = await api.clearStageMessage();
      stageMessageDraft = "";
      stageMessageDuration = "";
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function setOverlay(): Promise<void> {
    const text = overlayTextDraft.trim();
    const bg = overlayBackgroundDraft;
    if (!text && !bg) {
      errorMsg = "Overlay must have text or image";
      return;
    }
    try {
      errorMsg = null;
      appState = await api.setOverlay(text, bg);
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function showOverlay(): Promise<void> {
    try {
      errorMsg = null;
      if (!appState?.overlay) {
        await setOverlay();
      } else {
        appState = await api.setOverlayVisible(true);
      }
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function hideOverlay(): Promise<void> {
    try {
      errorMsg = null;
      appState = await api.setOverlayVisible(false);
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function clearOverlay(): Promise<void> {
    try {
      errorMsg = null;
      appState = await api.clearOverlay();
      overlayTextDraft = "";
      overlayBackgroundDraft = null;
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function pickOverlayImage(): Promise<void> {
    try {
      errorMsg = null;
      const picked = await open({ multiple: false, filters: MEDIA_FILTERS });
      if (typeof picked !== "string") return;
      overlayImporting = true;
      try {
        const asset = await api.importMedia(picked);
        overlayBackgroundDraft = asset.background;
      } finally {
        overlayImporting = false;
      }
    } catch (e) {
      overlayImporting = false;
      errorMsg = String(e);
    }
  }

  function removeOverlayBackground(): void {
    overlayBackgroundDraft = null;
  }

  onMount(() => {
    let unSub: () => void = () => {};
    let unAuto: () => void = () => {};
    let unLib: () => void = () => {};
    let unAck: () => void = () => {};
    let unFileDrop: (() => void) | null = null;
    let unFileDrop2: (() => void) | null = null;
    let cancelled = false;

    // Tauri OS file-drop fallback — some platforms/window managers deliver
    // desktop drops via `tauri://drag-drop` rather than HTML5 DataTransfer.files.
    // This ensures the same media pipeline (hash+copy+thumb → new slide) runs,
    // and also handles song-file drops (.pro/.cho/.usr) onto the Library.
    async function handleTauriPaths(paths: string[]): Promise<void> {
      if (!Array.isArray(paths) || paths.length === 0) return;
      const songPaths: string[] = [];
      const mediaPaths: string[] = [];
      const unsupported: string[] = [];
      for (const p of paths) {
        const ext = getFileExt(p.split(/[\/\\]/).pop() ?? p);
        if (SONG_EXTS.has(ext)) songPaths.push(p);
        else if (ALLOWED_EXTS.has(ext)) mediaPaths.push(p);
        else unsupported.push(p.split(/[\/\\]/).pop() ?? p);
      }
      if (unsupported.length > 0) {
        const msg = `Unsupported file type: ${unsupported.join(", ")}. Supported: ${[...SONG_EXTS].join(", ")} (songs) or ${[...ALLOWED_EXTS].join(", ")} (media)`;
        errorMsg = msg;
        externalDragError = msg;
        libraryDragError = msg;
      }
      if (songPaths.length > 0) {
        await handleLibraryFiles(songPaths as any);
      }
      if (mediaPaths.length > 0) {
        const target = dragOverIndex ?? project?.slides.length ?? 0;
        await handleExternalFiles(mediaPaths as any, target);
      }
      dragOverIndex = null;
      externalDragActive = false;
      libraryDragActive = false;
    }
    void (async () => {
      try {
        unFileDrop = await listen("tauri://drag-drop", (event: any) => {
          const raw = event.payload as any;
          const paths: string[] = Array.isArray(raw) ? raw : (raw?.paths ?? []);
          void handleTauriPaths(paths);
        });
        unFileDrop2 = await listen("tauri://file-drop", (event: any) => {
          const raw = event.payload as any;
          const paths: string[] = Array.isArray(raw) ? raw : (raw?.paths ?? []);
          void handleTauriPaths(paths);
        });
      } catch {
        // listen not available in browser preview — HTML5 path still works
      }
    })();

    void (async () => {
      const sub = await subscribeState((s) => {
        if (!cancelled) appState = s;
      });
      if (cancelled) { sub(); return; }
      unSub = sub;
      const autoSub = await subscribeAutosave((e) => {
        if (cancelled) return;
        savedLabel =
          e.status === "saved" ? `Saved ${formatAt(e.at)}` : `Autosave failed: ${e.message ?? "unknown"}`;
      });
      if (cancelled) { autoSub(); return; }
      unAuto = autoSub;
      const libSub = await subscribeLibrary((l) => {
        if (!cancelled) library = l;
      });
      if (cancelled) { libSub(); return; }
      unLib = libSub;
      try {
        const ackSub = await subscribeAck((u) => {
          if (!cancelled) ack = u;
        });
        if (cancelled) { ackSub(); return; }
        unAck = ackSub;
      } catch {
        // ack events unavailable (browser preview) — indicators stay idle.
      }
      try {
        const s = await api.getState();
        if (!cancelled) {
          appState = s;
          selectedId = s.project.live ?? s.project.slides[0]?.id ?? null;
          // Boot hub — Affinity-style launcher
          showHub = true;
          void refreshPlaylists();
        }
      } catch (e) {
        if (!cancelled) errorMsg = String(e);
      }
      if (!cancelled) {
        api.listDisplays().then((d) => { if (!cancelled) displays = d; }).catch((e: unknown) => { if (!cancelled) errorMsg = String(e); });
        api.getLibrary().then((l) => { if (!cancelled) library = l; }).catch((e: unknown) => { if (!cancelled) errorMsg = String(e); });
        void loadBibles();
        api.getBiblesFolder().then((p) => { if (!cancelled) biblesFolder = p; }).catch(() => {});
      }
    })();

    window.addEventListener("keydown", handleGlobalKeydown);
    // 1s ticker so "Confirmed Ns ago" ages live without new events.
    const ackClock = window.setInterval(() => {
      nowMs = Date.now();
    }, 1000);
    return () => {
      cancelled = true;
      unSub();
      unAuto();
      unLib();
      unAck();
      window.clearInterval(ackClock);
      if (unFileDrop) unFileDrop();
      if (unFileDrop2) unFileDrop2();
      window.removeEventListener("keydown", handleGlobalKeydown);
      if (scriptureTimer) clearTimeout(scriptureTimer);
      if (nameTimer) clearTimeout(nameTimer);
      if (titleTimer) clearTimeout(titleTimer);
      if (bodyTimer) clearTimeout(bodyTimer);
      if (autoAdvanceTimer) clearTimeout(autoAdvanceTimer);
    };
  });
</script>

{#if appState === null}
  <div class="loading-shell">
    <div class="spinner" aria-hidden="true"></div>
    <p>Starting MakrStudio…</p>
    {#if errorMsg}
      <p class="loading-error">{errorMsg}</p>
    {/if}
  </div>
{:else}
<div class="shell">
  <header class="topbar">
    <h1>MakrStudio</h1>
    <span class="project-name">{project?.name ?? "No view"}</span>
    <span class="live-indicator" class:live={!!project?.live}>
      {#if project?.live}LIVE{:else}OFFLINE{/if}
    </span>
    <span class="spacer"></span>
    <button
      class="ghost search-trigger"
      title="Search songs, Bibles & media (Ctrl+K)"
      onclick={() => (globalSearchOpen = true)}
    >
      ⌕ Search
      <span class="kbd">Ctrl+K</span>
    </button>
    <span class="saved-label">{savedLabel}</span>
    <button class="ghost" onclick={() => newProject()}>New view</button>
    <button class="ghost" onclick={() => clearOutput()}>Clear output</button>
    <button class="ghost help-trigger" title="Help — guided tour and keyboard shortcuts" onclick={() => (helpOpen = true)}>
      ? Help
    </button>
    <button class="ghost" title="Settings" onclick={() => (settingsOpen = true)}>
      &#9881; Settings
    </button>
  </header>

  {#if notice}
    <div class="notice">
      <span>{notice.message}</span>
      {#if notice.at}<span class="notice-at">({notice.at})</span>{/if}
      <button class="ghost" onclick={() => (noticeDismissed = true)}>Dismiss</button>
    </div>
  {/if}

  {#if errorMsg}
    <div class="error">Error: {errorMsg}</div>
  {/if}

  <div class="body">
        <aside class="sidebar">
      {#if welcome}
        <div class="welcome">
          <div>
            <strong>Welcome to MakrStudio</strong>
            <p>Add your first slide to get started — it will appear live on the output when you're ready.</p>
          </div>
          <button
            class="welcome-dismiss"
            title="Dismiss"
            onclick={() => (welcomeDismissed = true)}
          >
            &times;
          </button>
        </div>
        {/if}

      <div class="workspace-switch">
        <button class="ws-btn" class:active={centralView === "slides"} onclick={() => (centralView = "slides")} title="Slide grid — primary workspace">Slides</button>
        <button class="ws-btn" class:active={centralView === "looks"} onclick={() => { centralView = "looks"; use("looks"); }} title="Look editor — background, text style, layout">Looks</button>
      </div>
      {#if showHint(onboarding, "looks")}
        <p class="hint-line">Looks set fonts &amp; layout per screen — try the Looks tab.<button class="hint-x" title="Dismiss" aria-label="Dismiss Looks hint" onclick={() => dismiss("looks")}>×</button></p>
      {/if}

      <div class="sidebar-section playlist-section" class:has-content={(project?.slides.length ?? 0) > 0} class:tour-highlight={showTour && tourStep === 0}>
        <div class="section-title">Playlist</div>
        <ul
          class="slide-list"
          class:drag-active={isDragging || externalDragActive}
          class:external-drag={externalDragActive}
          ondragover={(e) => {
            if (isExternalFileDrag(e)) {
              handleExternalDragOver(e);
            } else {
              e.preventDefault();
              if (dragOverIndex === null) dragOverIndex = project?.slides.length ?? 0;
            }
          }}
          ondrop={(e) => onPlaylistDrop(e)}
          ondragleave={(e) => {
            const rt = e.relatedTarget as HTMLElement | null;
            if (!rt || !(e.currentTarget as HTMLElement).contains(rt)) {
              dragOverIndex = null;
              externalDragActive = false;
            }
          }}
        >
          {#each project?.slides ?? [] as slide, i (slide.id)}
            {#if dragOverIndex === i}
              <div class="drop-indicator" aria-hidden="true"></div>
            {/if}
            <li
              draggable="true"
              class:dragging={draggedSlideId === slide.id}
              class:drag-over={dragOverIndex === i}
              ondragstart={(e) => onPlaylistDragStart(e, slide, i)}
              ondragover={(e) => onPlaylistDragOver(e, i)}
              ondragend={onPlaylistDragEnd}
              ondrop={(e) => onPlaylistDrop(e, i)}
            >
              <button
                class:active={project?.live === slide.id}
                class="slide-entry"
                onclick={() => goLive(slide)}
                draggable="false"
              >
                <span
                  class="swatch"
                  class:camera={isLiveCamera(slide.background)}
                  style:background-color={slide.background.type === "solid"
                    ? slide.background.color
                    : "#000"}
                  style:background-image={isMedia(slide.background)
                    ? `url('${fileUrl(slide.background.thumb)}')`
                    : "none"}
                  style:background-size="cover"
                  style:background-position="center"
                  title={isLiveCamera(slide.background) ? `Live camera: ${slide.background.label || "camera"}` : undefined}
                >{#if isLiveCamera(slide.background)}<span aria-hidden="true">🎥</span>{/if}</span>
                <span class="slide-label">{slideDisplayName(slide)}</span>
                {#if slide.autoAdvanceSecs != null}<span class="auto-badge" title="Auto-advance after {slide.autoAdvanceSecs}s">↻ {slide.autoAdvanceSecs}s</span>{/if}
                {#if project?.live === slide.id}<span class="live-dot"></span>{/if}
              </button>
              <button
                class="delete"
                title="Delete slide"
                onclick={(e) => {
                  e.stopPropagation();
                  deleteSlide(slide);
                }}
              >
                &times;
              </button>
            </li>
          {/each}
          {#if dragOverIndex === (project?.slides.length ?? 0)}
            <div class="drop-indicator" aria-hidden="true"></div>
          {/if}
        </ul>
        <button class="add" onclick={() => addSlide()}>+ Add slide</button>
        {#if showHint(onboarding, "dragdrop")}
          <p class="hint-line">Drag songs, verses, or images here — or click to add. Drag slides to reorder.<button class="hint-x" title="Dismiss" aria-label="Dismiss drag-and-drop hint" onclick={() => dismiss("dragdrop")}>×</button></p>
        {/if}
        {#if showHint(onboarding, "shortcuts")}
          <p class="hint-line"><kbd>←</kbd> <kbd>→</kbd> advance live slides · <kbd>Ctrl+K</kbd> searches everything.<button class="hint-x" title="Dismiss" aria-label="Dismiss shortcuts hint" onclick={() => dismiss("shortcuts")}>×</button></p>
        {/if}
        <div class="template-actions">
          <button class="ghost template-btn" onclick={() => openSavePlaylist()} title="Save this View's playlist as a reusable Playlist">Save as Playlist</button>
        </div>
        <div
          class="external-drop-zone"
          role="region"
          aria-label="Drop media files here"
          class:drag-active={externalDragActive}
          ondragover={(e) => {
            if (isExternalFileDrag(e)) {
              e.preventDefault();
              externalDragActive = true;
              if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
              if (dragOverIndex === null) dragOverIndex = project?.slides.length ?? 0;
            }
          }}
          ondragleave={(e) => handleExternalDragLeave(e)}
          ondrop={(e) => {
            e.preventDefault();
            e.stopPropagation();
            externalDragActive = false;
            const target = dragOverIndex ?? project?.slides.length ?? 0;
            if ((e.dataTransfer?.files?.length ?? 0) > 0) {
              void handleExternalFiles(e.dataTransfer!.files, target);
            }
            dragOverIndex = null;
          }}
        >
          <span class="drop-zone-label">Drop images or videos here — creates a new slide</span>
          {#if externalDragError}
            <span class="drop-error" role="alert">{externalDragError}</span>
          {/if}
        </div>
      </div>

      <div class="sidebar-section scripture-section" class:has-content={scriptureOpen || scriptureQuery.trim().length > 0} class:active={scriptureOpen}>
        <div class="section-title scripture-title">Add Scripture</div>
        <div class="scripture-wrap">
          <input
            type="text"
            class="search"
            placeholder="e.g. John 3:16, psalm 23, jn 1"
            value={scriptureQuery}
            oninput={onScriptureInput}
            onkeydown={onScriptureKeydown}
            onfocus={() => {
              if (scriptureResults.length > 0) scriptureOpen = true;
            }}
            onblur={() => {
              setTimeout(() => {
                scriptureOpen = false;
              }, 150);
            }}
          />
          {#if scriptureLoading}
            <span class="scripture-loading" aria-hidden="true">
              <span class="media-spinner"></span>
            </span>
          {/if}
          {#if scriptureOpen}
            <ul class="scripture-list">
              {#each scriptureResults as match, i (match.reference)}
                <li>
                  <button
                    class:active={i === scriptureIdx}
                    class="scripture-entry"
                    draggable="true"
                    ondragstart={(e) => onScriptureDragStart(e, match.reference, match.text)}
                    onmousedown={(e) => {
                      e.preventDefault();
                      selectScripture(match);
                    }}
                    title="Drag to playlist • Click to add"
                  >
                    <span class="scripture-ref">{match.reference}</span>
                    <span class="scripture-preview">{match.text}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
          {#if scriptureQuery.trim() && !scriptureLoading && scriptureResults.length === 0 && looksLikeReference(scriptureQuery)}
            <button
              class="add scripture-fallback"
              disabled={scriptureBusy}
              onclick={() => fetchApiScripture(scriptureQuery)}
            >
              Look up “{scriptureQuery.trim()}” on bible-api.com
            </button>
          {/if}
        <button
          class="add scripture-import"
          disabled={scriptureBusy}
          onclick={() => void importOpenlpFile()}
        >
          Import OpenLP Bible…
        </button>
        {#if biblesFolder}
          <p class="bibles-folder-hint">Or place OpenLP XML files directly in:<br><code>{biblesFolder}</code></p>
        {/if}
        {#if scriptureStatus}
            <p class="scripture-status">{scriptureStatus}</p>
          {/if}
        </div>
      </div>

      <div class="browse-panel" class:tour-highlight={showTour && tourStep === 2}>
        <button class="browse-header" onclick={() => (browseCollapsed = !browseCollapsed)} aria-expanded={!browseCollapsed}>
          <span class="section-title" style="margin:0; border:none; padding:0;">Browse Scripture</span>
          <span class="browse-toggle">{browseCollapsed ? "▸ Show" : "▾ Hide"}</span>
        </button>
        {#if !browseCollapsed}
          <p class="browse-hint">Browsing as full-width panel below — click a verse to add as slide (drag secondary).</p>
        {:else if showHint(onboarding, "browse")}
          <p class="hint-line">Bible verses live here — Show to browse book → chapter → verse.<button class="hint-x" title="Dismiss" aria-label="Dismiss browse hint" onclick={() => dismiss("browse")}>×</button></p>
        {/if}
      </div>

      <div
        class="sidebar-section library-section"
        role="region"
        aria-label="Library — drop .pro/.cho/.usr files"
        class:has-content={librarySongs.length > 0 || librarySearch.trim().length > 0}
        class:library-drag-active={libraryDragActive}
        class:tour-highlight={showTour && tourStep === 2}
        ondragover={(e) => handleLibraryDragOver(e)}
        ondragleave={(e) => handleLibraryDragLeave(e)}
        ondrop={(e) => {
          e.preventDefault();
          e.stopPropagation();
          libraryDragActive = false;
          if ((e.dataTransfer?.files?.length ?? 0) > 0) void handleLibraryFiles(e.dataTransfer!.files);
        }}
      >
        <div class="section-title library-title">Library</div>
        <input
          type="text"
          class="search"
          placeholder="Search songs"
          bind:value={librarySearch}
        />
        <ul class="song-list">
          {#each librarySongs as song (song.id)}
            <li>
              <button
                class="song-entry"
                draggable="true"
                ondragstart={(e) => onLibrarySongDragStart(e, song)}
                onclick={() => addToPlaylist(song)}
                title="Drag to playlist to add • Click to add (uses arrangement)"
              >
                <span class="song-label">{song.title || "Untitled"}</span>
                <span class="song-count">{getSongArrangementCount(song)} {getSongArrangementCount(song) === 1 ? "slide" : "slides"} • {getSongBlockCount(song)} blocks</span>
              </button>
              <button
                class="delete"
                title="Delete song"
                onclick={(e) => {
                  e.stopPropagation();
                  deleteSong(song);
                }}
              >
                &times;
              </button>
            </li>
            {#each getBlocksArray(song) as verse (verse.id)}
              <li class="library-verse-row">
                <button
                  class="library-verse"
                  draggable="true"
                  ondragstart={(e) => onLibraryVerseDragStart(e, song, verse)}
                  onclick={() => void api.addSlide(verse.title, verse.body).then((s) => (appState = s)).catch((err: unknown) => (errorMsg = String(err)))}
                  title="Drag verse to playlist • Click to add as slide"
                >
                  <span class="verse-title">{verse.title || "Untitled verse"}</span>
                  <span class="verse-preview">{verse.body.slice(0, 60)}{verse.body.length > 60 ? "…" : ""}</span>
                </button>
              </li>
            {/each}
            {#if song.arrangement && song.arrangement.length > 0}
              <li class="arrangement-row">
                <span class="arrangement-label">Order:</span>
                <div class="chip-list">
                  {#each song.arrangement as blockKey, idx (blockKey + "-" + idx)}
                    <span class="chip" title={blockKey}>
                      {blockKey}
                      <button class="chip-btn" title="Move left" onclick={() => moveArrangement(song, idx, idx - 1)} disabled={idx === 0}>‹</button>
                      <button class="chip-btn" title="Move right" onclick={() => moveArrangement(song, idx, idx + 1)} disabled={idx === song.arrangement.length - 1}>›</button>
                      <button class="chip-btn" title="Duplicate" onclick={() => duplicateArrangement(song, idx)}>⧉</button>
                      <button class="chip-btn chip-remove" title="Remove from order" onclick={() => removeFromArrangement(song, idx)}>×</button>
                    </span>
                  {/each}
                </div>
                <div class="arrangement-actions">
                  <select
                    class="arrangement-add"
                    value=""
                    onchange={(e) => {
                      const v = (e.target as HTMLSelectElement).value;
                      if (v) {
                        addBlockToArrangement(song, v);
                        (e.target as HTMLSelectElement).value = "";
                      }
                    }}
                  >
                    <option value="">+ Add block…</option>
                    {#each Object.keys(song.blocks ?? {}) as key (key)}
                      <option value={key}>{key}</option>
                    {/each}
                  </select>
                </div>
              </li>
            {/if}
          {:else}
            <li class="empty">No songs yet. Add one below.</li>
          {/each}
        </ul>
        {#if librarySongs.length === 0 && showHint(onboarding, "songs")}
          <p class="hint-line">Songs live here — + Add song, or drop .pro / .cho / .usr files.<button class="hint-x" title="Dismiss" aria-label="Dismiss library hint" onclick={() => dismiss("songs")}>×</button></p>
        {/if}
        <button class="add" onclick={() => addLibrarySong()}>+ Add song</button>
        <div
          class="library-drop-zone"
          role="region"
          aria-label="Drop song files here"
          class:drag-active={libraryDragActive}
          ondragover={(e) => handleLibraryDragOver(e)}
          ondragleave={(e) => handleLibraryDragLeave(e)}
          ondrop={(e) => {
            e.preventDefault();
            e.stopPropagation();
            libraryDragActive = false;
            if ((e.dataTransfer?.files?.length ?? 0) > 0) void handleLibraryFiles(e.dataTransfer!.files);
          }}
        >
          <span class="drop-zone-label">Drop .pro / .cho / .usr here — adds to Library</span>
          {#if libraryDragError}
            <span class="drop-error" role="alert">{libraryDragError}</span>
          {/if}
        </div>
      </div>
    </aside>

    <main class="editor">
      {#if centralView === "looks"}
        <LookEditorView appState={appState} onUpdate={(s) => (appState = s)} onError={(m) => (errorMsg = m)} />
      {:else if showDetail && selected}
        <div class="detail-header">
          <button class="ghost" onclick={() => closeDetail()} title="Back to grid">← Grid</button>
          <span class="detail-title">{slideDisplayName(selected)} — Edit</span>
          <span class="spacer"></span>
          <button class="ghost" class:active={project?.live === selected.id} onclick={() => goLive(selected)} title="Go live on Output">Go Live</button>
          <button class="ghost" onclick={() => closeDetail()}>Done</button>
        </div>
        <div class="edit-window">
          <label>
            Slide name
            <input
              type="text"
              value={draftName}
              placeholder="e.g. Verse 1 — label under thumbnail (blank = follows Title)"
              spellcheck="true"
              lang="en"
              oninput={(e) => onNameInput((e.target as HTMLInputElement).value)}
              onblur={() => flushName()}
            />
            <span class="field-hint">Label shown in grid & playlist — not rendered on Output. Leave blank to follow Title.</span>
          </label>
          <label>
            Title — on-screen
            <div class="title-row">
              <input
                type="text"
                value={draftTitle}
                placeholder="Slide title"
                spellcheck="true"
                lang="en"
                oninput={(e) => onTitleInput((e.target as HTMLInputElement).value)}
                onblur={() => flushTitle()}
              />
              <button
                class="ghost title-case-btn"
                type="button"
                title="Title Case — e.g. 'amazing grace' → 'Amazing Grace'"
                onclick={() => applyTitleCase()}
              >
                Aa
              </button>
            </div>
            <span class="field-hint">Tip: “Aa” fixes caps before going live. Body spellcheck underlines typos.</span>
          </label>
          <label>
            Body
            <textarea
              rows="8"
              value={draftBody}
              placeholder="Slide body text"
              spellcheck="true"
              lang="en"
              oninput={(e) => onBodyInput((e.target as HTMLTextAreaElement).value)}
              onblur={() => flushBody()}
            ></textarea>
          </label>
          <div class="field">
            <span class="field-label">Background</span>
            <div class="swatches">
              {#each PALETTE as color}
                <button
                  class="swatch"
                  style:background-color={color}
                  class:selected={selected.background.type === "solid" &&
                    selected.background.color.toLowerCase() === color}
                  onclick={() => setColor(selected, color)}
                  title={color}
                ></button>
              {/each}
              <label class="custom-color">
                <input
                  type="color"
                  value={selected.background.type === "solid"
                    ? selected.background.color
                    : "#000000"}
                  oninput={(e) => setColor(selected, (e.target as HTMLInputElement).value)}
                />
                <span>Custom</span>
              </label>
              {#if isMedia(selected.background)}
                <span class="media-swatch-wrap">
                  <button
                    class="swatch media selected"
                    style:background-color="#000"
                    title={selected.background.type === "video"
                      ? `Video background${selected.background.durationMs != null ? ` \u00b7 ${Math.round(selected.background.durationMs / 1000)}s` : ""}`
                      : "Image background"}
                  >
                    <img
                      src={fileUrl(selected.background.thumb)}
                      alt=""
                      draggable="false"
                      onerror={(e) => {
                        (e.currentTarget as HTMLImageElement).style.display = "none";
                      }}
                    />
                  </button>
                  <button
                    class="media-remove"
                    title="Remove media background"
                    onclick={() => setColor(selected, PALETTE[0] ?? "#000000")}
                  >
                    &times;
                  </button>
                </span>
              {/if}
              {#if isLiveCamera(selected.background)}
                <span class="media-swatch-wrap">
                  <span
                    class="swatch media selected camera-badge"
                    style:background-color="#000"
                    title={`Live camera: ${selected.background.label || "camera"} — pick 🎥 below to change`}
                  ><span aria-hidden="true">🎥</span></span>
                  <span class="camera-name">{selected.background.label || "Live camera"}</span>
                  <button
                    class="media-remove"
                    title="Remove camera background"
                    onclick={() => setColor(selected, PALETTE[0] ?? "#000000")}
                  >
                    &times;
                  </button>
                </span>
              {/if}
              <button
                class="media-add"
                title="Add image or video background"
                onclick={() => addMedia(selected)}
                disabled={importingMedia}
              >
                {#if importingMedia}
                  <span class="media-spinner" aria-hidden="true"></span>
                {:else}
                  +
                {/if}
              </button>
              <button
                class="media-add"
                type="button"
                title="Use a live camera / capture card as background"
                onclick={() => openCameraPicker()}
              >
                <span aria-hidden="true">🎥</span>
              </button>
            </div>
            {#if showCameraPicker}
              <div class="camera-picker" role="dialog" aria-label="Choose a camera">
                <div class="camera-picker-head">
                  <span>Live camera background</span>
                  <button class="ghost camera-picker-close" title="Close" aria-label="Close camera picker" onclick={() => (showCameraPicker = false)}>&times;</button>
                </div>
                {#if cameraLoading}
                  <span class="media-spinner" aria-hidden="true"></span>
                {:else if cameraDevices.length > 0}
                  <ul class="camera-list">
                    {#each cameraDevices as d (d.deviceId)}
                      <li>
                        <button class="camera-entry" onclick={() => selectCamera(d)}>
                          <span aria-hidden="true">🎥</span>
                          <span>{d.label || "Camera (grant access to see its name)"}</span>
                        </button>
                      </li>
                    {/each}
                  </ul>
                  {#if cameraDevices.every((d) => !d.label)}
                    <button class="ghost" onclick={() => void enableCameraAccess()}>Enable camera access (shows device names)</button>
                  {/if}
                  <button class="ghost" onclick={() => void refreshCameras()}>Refresh</button>
                {:else}
                  <div class="camera-actions">
                    <button class="ghost" onclick={() => void enableCameraAccess()}>Enable camera access</button>
                    <button class="ghost" onclick={() => void refreshCameras()}>Refresh</button>
                  </div>
                {/if}
                {#if cameraError}
                  <p class="camera-error" role="alert">{cameraError}</p>
                {/if}
                <span class="field-hint">Muted, like recorded video — camera audio stays on your mixer, never through the app.</span>
              </div>
            {/if}
          </div>
          <label>
            Auto-advance
            <input
              type="number"
              min="1"
              max="86400"
              step="1"
              placeholder="e.g. 5 — blank = manual"
              value={draftAutoAdvance}
              oninput={(e) => onAutoAdvanceInput((e.target as HTMLInputElement).value)}
              onblur={() => flushAutoAdvance()}
            />
            <span class="field-hint">When live, advance to next slide after N seconds. Blank = manual.</span>
          </label>
        </div>
      {:else}
        <div class="grid-toolbar">
          <span class="section-title" style="margin:0">Slides — {project?.slides.length ?? 0}</span>
          <span class="spacer"></span>
          <button class="ghost" onclick={() => addSlide()}>+ Add slide</button>
        </div>
        {#if (project?.slides.length ?? 0) === 0}
          <div class="empty grid-empty">No slides yet. Add one to get started — it will appear here as a thumbnail.</div>
        {:else}
          <div
            class="slide-grid"
            role="region"
            aria-label="Slides grid"
            ondragover={(e) => {
              if (isExternalFileDrag(e)) handleExternalDragOver(e);
              else {
                e.preventDefault();
                if (dragOverIndex === null) dragOverIndex = project?.slides.length ?? 0;
              }
            }}
            ondrop={(e) => onPlaylistDrop(e)}
            ondragleave={(e) => {
              const rt = e.relatedTarget as HTMLElement | null;
              if (!rt || !(e.currentTarget as HTMLElement).contains(rt)) {
                dragOverIndex = null;
                externalDragActive = false;
              }
            }}
          >
            {#each project?.slides ?? [] as slide, i (slide.id)}
              {#if dragOverIndex === i}
                <div class="grid-drop-indicator" aria-hidden="true"></div>
              {/if}
              <div
                class="grid-cell"
                class:selected={selectedId === slide.id}
                class:live={project?.live === slide.id}
                draggable="true"
                role="group"
                aria-label={slideDisplayName(slide)}
                ondragstart={(e) => onPlaylistDragStart(e, slide, i)}
                ondragover={(e) => onPlaylistDragOver(e, i)}
                ondragend={onPlaylistDragEnd}
                ondrop={(e) => onPlaylistDrop(e, i)}
              >
                <button class="grid-thumb" onclick={() => openDetail(slide)} title="Click to edit — {slideDisplayName(slide)}">
                  <div class="grid-thumb-inner">
                    {#if outputPreviewLook}
                      <SlideRender
                        slide={slide}
                        look={outputPreviewLook}
                        showText={true}
                        showBackground={true}
                        overlay={null}
                      />
                    {:else}
                      <div class="grid-thumb-fallback">{slide.title || "Untitled"}</div>
                    {/if}
                  </div>
                  {#if project?.live === slide.id}
                    <span class="grid-live-badge">LIVE</span>
                  {/if}
                </button>
                <div class="grid-label" title={slideDisplayName(slide)}>{slideDisplayName(slide)}</div>
                <div class="grid-actions">
                  <button class="ghost grid-go-live" onclick={() => goLive(slide)} title="Go live">Go Live</button>
                  <button class="delete grid-delete" title="Delete slide" onclick={(e) => { e.stopPropagation(); deleteSlide(slide); }}>×</button>
                </div>
              </div>
            {/each}
            {#if dragOverIndex === (project?.slides.length ?? 0)}
              <div class="grid-drop-indicator" aria-hidden="true"></div>
            {/if}
          </div>
        {/if}
      {/if}
    </main>

    <aside class="sidebar output-panel" class:tour-highlight={showTour && tourStep === 1}>
      <div class="output-sticky-top">
        <div class="section-title">Output</div>

        <label>
          Display
          <select
            value={appState?.output.monitorIndex ?? ""}
            onchange={onDisplayChange}
            disabled={displays === null}
          >
            {#if displays === null}
              <option value="" disabled>Loading displays…</option>
            {:else if (displays?.length ?? 0) === 0}
              <option value="" disabled>No displays found</option>
            {/if}
            {#each displays ?? [] as d}
              <option value={d.index}>
                {d.name || `Display ${d.index + 1}`} &middot; {d.width}&times;{d.height}{d.primary
                  ? " (primary)"
                  : ""}
              </option>
            {/each}
          </select>
        </label>

        <button class="ghost" onclick={() => toggleFullscreen()}>
          {appState?.output.fullscreen ? "Exit fullscreen" : "Go fullscreen"}
        </button>

        <label>
          Transition
          <select
            value={project?.transition ?? "cut"}
            onchange={onTransitionChange}
          >
            <option value="cut">Cut</option>
            <option value="fade">Fade</option>
          </select>
        </label>

        <label>
          Output Look
          <select
            value={appState?.outputLookId ?? ""}
            onchange={onOutputLookChange}
          >
            <option value="">Auto (Main)</option>
            {#each appState?.looks ?? [] as lk (lk.id)}
              <option value={lk.id}>{lk.name}</option>
            {/each}
          </select>
        </label>

        <div class="preview-row">
          <div class="preview-box">
            {#if outputPreviewSlide && outputPreviewLook}
              <SlideRender
                slide={outputPreviewSlide}
                look={outputPreviewLook}
                showText={project?.showText ?? true}
                showBackground={project?.showBackground ?? true}
                overlay={appState?.overlay ?? null}
                enableCamera={true}
              />
            {:else}
              <div class="preview-empty">No slide</div>
            {/if}
          </div>
          <span class="on-air-badge" class:on={isOnAir} class:off={!isOnAir}>{isOnAir ? "ON AIR" : "OFF"}</span>
        </div>

        {#if appState?.output.visible}
          {@const outAge = ackAgeMs(ack?.output?.at)}
          {#if outAge === null}
            <p class="ack-line idle" title="The Output window confirms each applied state plus a heartbeat every 5s.">Waiting for Output confirmation…</p>
          {:else if outAge <= ACK_STALE_MS}
            <p class="ack-line ok" title={`Output confirmed at ${ack?.output?.at ?? "unknown time"}`}>✓ {ackLabel(outAge)}</p>
          {:else}
            <p class="ack-line stale" role="alert" title={`Last Output confirmation at ${ack?.output?.at ?? "unknown time"}`}>⚠ No confirmation for {Math.round(outAge / 1000)}s — Output may be frozen</p>
          {/if}
        {/if}

        <div class="output-status" class:live={!!(appState?.output.visible && project?.live)}>
          {#if appState?.output.visible}
            {#if project?.live}
              Live on display
              {appState?.output.monitorName ||
                (appState?.output.monitorIndex != null
                  ? `#${appState.output.monitorIndex}`
                  : "?")}
            {:else}
              Output is black (no live slide)
            {/if}
          {:else}
            <span class="not-shown">Not shown yet</span> — the output appears here the first time a slide goes live.
          {/if}
        </div>

        <div class="clear-row">
          <button class="ghost" onclick={() => clearOutput()} title="Clear both text and background (black)">Clear output</button>
          <button class="ghost" onclick={() => clearText()} title="Hide text, keep background">Clear text</button>
          <button class="ghost" onclick={() => clearBackground()} title="Hide background, keep text on black">Clear background</button>
        </div>

        {#if !appState?.output.visible}
          <button class="ghost show-output" onclick={() => showOutput()}>
            Show Output
          </button>
        {/if}
      </div>

      <div class="section-title stage-title">Stage Display</div>

      <button class="ghost" onclick={() => toggleStage()}>
        {appState?.stage.visible ? "Hide stage" : "Show stage"}
      </button>
      {#if !appState?.stage.visible && showHint(onboarding, "stage")}
        <p class="hint-line">For the platform: Show stage, then Send a message — Output never sees it.<button class="hint-x" title="Dismiss" aria-label="Dismiss stage hint" onclick={() => dismiss("stage")}>×</button></p>
      {/if}

      <label>
        Display
        <select
          value={appState?.stage.monitorIndex ?? ""}
          onchange={onStageDisplayChange}
          disabled={displays === null}
        >
          {#if displays === null}
            <option value="" disabled>Loading displays…</option>
          {:else if (displays?.length ?? 0) === 0}
            <option value="" disabled>No displays found</option>
          {/if}
          {#each displays ?? [] as d}
            <option value={d.index}>
              {d.name || `Display ${d.index + 1}`} &middot; {d.width}&times;{d.height}{d.primary
                ? " (primary)"
                : ""}
            </option>
          {/each}
        </select>
      </label>

      <div class="preview-row">
        <div class="preview-box">
          {#if stagePreviewSlide && stagePreviewLook}
            <SlideRender
              slide={stagePreviewSlide}
              look={stagePreviewLook}
              showText={project?.showText ?? true}
              showBackground={project?.showBackground ?? true}
              isStage={true}
            />
          {:else}
            <div class="preview-empty">No slide</div>
          {/if}
        </div>
        <span class="on-air-badge" class:on={isStageOnAir} class:off={!isStageOnAir}>{isStageOnAir ? "ON AIR" : "OFF"}</span>
      </div>

      {#if appState?.stage.visible}
        {@const stageAge = ackAgeMs(ack?.stage?.at)}
        {#if stageAge === null}
          <p class="ack-line idle" title="The Stage window confirms each applied state plus a heartbeat every 5s.">Waiting for Stage confirmation…</p>
        {:else if stageAge <= ACK_STALE_MS}
          <p class="ack-line ok" title={`Stage confirmed at ${ack?.stage?.at ?? "unknown time"}`}>✓ {ackLabel(stageAge)}</p>
        {:else}
          <p class="ack-line stale" role="alert" title={`Last Stage confirmation at ${ack?.stage?.at ?? "unknown time"}`}>⚠ No confirmation for {Math.round(stageAge / 1000)}s — Stage may be frozen</p>
        {/if}
      {/if}

      <div class="output-status">
        {#if appState?.stage.visible}
          Stage on display
          {appState.stage.monitorName ||
            (appState.stage.monitorIndex != null
              ? `#${appState.stage.monitorIndex}`
              : "?")}
        {:else}
          Stage is hidden
        {/if}
      </div>

      <label>
        Stage Look
        <select
          value={appState?.stageLookId ?? ""}
          onchange={onStageLookChange}
        >
          <option value="">Auto (Stage)</option>
          {#each appState?.looks ?? [] as lk (lk.id)}
            <option value={lk.id}>{lk.name}</option>
          {/each}
        </select>
      </label>

      <div class="stage-message-panel">
        <span class="field-label">Stage message — stage only (never Output)</span>
        {#if appState?.stageMessage}
          <div class="stage-message-current">
            <span class="stage-message-text">{appState.stageMessage}</span>
            <span class="live-dot" style:background="var(--semantic-error, #e11d48)"></span>
          </div>
        {/if}
        <div class="stage-message-row">
          <input
            type="text"
            placeholder="Nursery alert, countdown, note…"
            bind:value={stageMessageDraft}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void sendStageMessage();
              }
            }}
          />
          <input
            type="number"
            min="1"
            max="3600"
            step="1"
            placeholder="30s"
            bind:value={stageMessageDuration}
            title="Auto-clear after N seconds (blank = stay until Clear)"
            class="stage-message-duration"
          />
        </div>
        <div class="stage-message-actions">
          <button class="ghost" onclick={() => void sendStageMessage()} title="Send to Stage instantly">Send</button>
          <button class="ghost" onclick={() => void clearStageMessage()} title="Clear Stage banner">Clear</button>
        </div>
        <span class="field-hint">Red flashing banner on Stage only — never on Output. Optional duration auto-clears.</span>
      </div>

      <div class="stage-message-panel">
        <span class="field-label">Overlays — Output only (never Stage)</span>
        {#if appState?.overlay}
          <div class="stage-message-current">
            <span class="stage-message-text">{appState.overlay.text || "(image)"} — {appState.overlay.visible ? "Visible" : "Hidden"}</span>
            <span class="live-dot" style:background={appState.overlay.visible ? "var(--semantic-live, #1f9d6a)" : "var(--semantic-idle, #64748b)"}></span>
          </div>
        {/if}
        <div class="stage-message-row">
          <input
            type="text"
            placeholder="Lower-third text…"
            bind:value={overlayTextDraft}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void setOverlay();
              }
            }}
          />
        </div>
        {#if overlayBackgroundDraft && isMedia(overlayBackgroundDraft)}
          <div class="overlay-preview" style="display:flex; gap:8px; align-items:center; font-size:11px;">
            <span
              class="media-thumb"
              style:background-image={`url('${fileUrl(overlayBackgroundDraft.thumb)}')`}
              style:width="44px"
              style:height="28px"
              aria-hidden="true"
            ></span>
            <span>{overlayBackgroundDraft.type === "video" ? "Video" : "Image"} overlay ready</span>
            <button class="ghost" onclick={() => removeOverlayBackground()} title="Remove image">×</button>
          </div>
        {/if}
        <div class="stage-message-actions">
          <button class="ghost" onclick={() => void pickOverlayImage()} disabled={overlayImporting} title="Pick image/video for overlay">
            {overlayImporting ? "…" : "Image…"}
          </button>
          <button class="ghost" onclick={() => void setOverlay()} title="Set overlay (visible)">Set</button>
          <button class="ghost" onclick={() => void showOverlay()} title="Show overlay">Show</button>
          <button class="ghost" onclick={() => void hideOverlay()} title="Hide overlay">Hide</button>
          <button class="ghost" onclick={() => void clearOverlay()} title="Clear overlay">Clear</button>
        </div>
        <span class="field-hint">Lower-third / logo on Output only — background (z0), main (z1), overlay (z2). Video keeps playing when overlay toggles.</span>
      </div>
    </aside>
  </div>

  {#if !browseCollapsed}
    <div class="browse-dock" role="region" aria-label="Browse Scripture">
      <div class="browse-dock-left">
        <label>
          Translation
          <select value={selectedBibleId ?? ""} onchange={onBrowseBibleChange} disabled={bibles.length === 0}>
            {#each bibles as b}
              <option value={b.id}>{b.name} ({b.bookCount})</option>
            {/each}
          </select>
        </label>
        {#if browseError}
          <p class="browse-error">{browseError}</p>
        {/if}
        <div class="browse-books">
          {#each bibleBooks as book}
            <button class="browse-book" class:active={book === selectedBook} onclick={() => onBrowseBookSelect(book)}>{book}</button>
          {/each}
        </div>
      </div>
      <div class="browse-dock-middle">
        {#if selectedBook}
          <div class="browse-chapters">
            <span class="field-label">{selectedBook} — Chapters</span>
            <div class="chapter-grid">
              {#each chapterNumbers as ch}
                <button class="chapter-pill" class:active={ch === selectedChapter} onclick={() => onBrowseChapterSelect(ch)}>{ch}</button>
              {/each}
            </div>
          </div>
        {:else}
          <p class="browse-placeholder">Select a book to see chapters</p>
        {/if}
        {#if browseLoading}
          <span class="media-spinner" style="align-self:center; margin: 8px 0;"></span>
        {/if}
      </div>
      <div class="browse-dock-right">
        {#if chapterVerses.length > 0}
          <ul class="browse-verses">
            {#each chapterVerses as v}
              <li>
                <button
                  class="browse-verse"
                  draggable="true"
                  ondragstart={(e) => onScriptureDragStart(e, `${selectedBook} ${selectedChapter}:${v.verse}`, v.text)}
                  onclick={() => insertBrowseVerse(v)}
                >
                  <span class="verse-num">{v.verse}</span>
                  <span class="verse-text">{v.text}</span>
                </button>
              </li>
            {/each}
          </ul>
        {:else if selectedChapter}
          <p class="browse-placeholder">No verses</p>
        {:else if selectedBook}
          <p class="browse-placeholder">Select a chapter to see verses — click a verse to add as slide (drag secondary)</p>
        {:else}
          <p class="browse-placeholder">Select a book and chapter to browse verses. Click a verse to add as slide.</p>
        {/if}
      </div>
      <button class="browse-dock-close" onclick={() => (browseCollapsed = true)} title="Hide browse panel">× Hide</button>
    </div>
  {/if}
</div>
{/if}

{#if settingsOpen}
  <SettingsPanel app={appState} onclose={() => (settingsOpen = false)} />
{/if}

<Modal
  open={showAddSongTitleModal}
  title="Add song"
  label="Song title"
  placeholder="e.g. Amazing Grace"
  initialValue=""
  confirmLabel="Next"
  cancelLabel="Cancel"
  onConfirm={handleAddSongTitleConfirm}
  onCancel={handleAddSongCancel}
/>
<Modal
  open={showAddSongBodyModal}
  title="Add song"
  label="Lyrics / body text (optional)"
  placeholder="Enter lyrics, press Enter to save"
  initialValue=""
  confirmLabel="Add song"
  cancelLabel="Back"
  onConfirm={handleAddSongBodyConfirm}
  onCancel={() => {
    showAddSongBodyModal = false;
    showAddSongTitleModal = true;
  }}
/>

<SongEditorModal
  open={showSongEditor}
  initialTitle={pendingSongTitle}
  initialBody={pendingSongBody}
  onConfirm={handleSongEditorConfirm}
  onCancel={handleAddSongCancel}
  onBack={handleSongEditorBack}
/>

<ProjectHub
  open={showHub}
  recentName={project?.name ?? ""}
  playlists={templates}
  onClose={() => (showHub = false)}
  onCreate={handleHubCreate}
  onCreateFromPlaylist={handleHubCreateFromPlaylist}
/>

<Modal
  open={showSavePlaylistModal}
  title="Save as Playlist"
  label="Playlist name — e.g. Pre-Service Loop, Worship, Sermon"
  placeholder="e.g. Worship"
  initialValue=""
  confirmLabel="Save"
  cancelLabel="Cancel"
  onConfirm={handleSavePlaylistConfirm}
  onCancel={() => (showSavePlaylistModal = false)}
/>

<GlobalSearch open={globalSearchOpen} library={library} onClose={() => (globalSearchOpen = false)} />

{#if showTour}
  <GuidedTour
    step={tourStep}
    steps={TOUR_STEPS}
    onNext={() => (tourStep = Math.min(tourStep + 1, TOUR_STEPS.length - 1))}
    onBack={() => (tourStep = Math.max(tourStep - 1, 0))}
    onDone={() => endTour()}
    onSkip={() => endTour()}
  />
{/if}

<HelpModal open={helpOpen} onClose={() => (helpOpen = false)} onReplayTour={() => replayTour()} />

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .topbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    row-gap: 8px;
    gap: 12px;
    padding: 8px 14px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    /* Guard: don't make the whole header a drag zone — only the empty spacer is draggable
       when frameless. This prevents Windows 11 from intercepting clicks on the buttons inside. */
    -webkit-app-region: no-drag;
  }

  .topbar .spacer {
    -webkit-app-region: drag;
  }

  .topbar h1 {
    font-family: var(--font-display);
    font-size: clamp(13px, 1.1vw, 16px);
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    margin: 0;
  }

  .project-name {
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .spacer {
    flex: 1;
  }

  .saved-label {
    font-size: 12px;
    color: var(--text-dim);
  }

  /* Narrow windows: the view name yields first and non-critical chrome hides,
     so Search / New view / Clear output / Help / Settings never get pushed
     off-screen. */
  @media (max-width: 700px) {
    .topbar {
      gap: 8px;
    }
    .saved-label,
    .search-trigger .kbd {
      display: none;
    }
  }

  .search-trigger {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    padding: 6px 10px;
    border-color: var(--border);
  }
  .search-trigger .kbd {
    font-size: 10px;
    color: var(--text-dim);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 5px;
    background: var(--panel-2);
    font-family: var(--font-mono, monospace);
  }

  button {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel-2);
    color: var(--text);
    padding: 6px 12px;
  }

  .ghost {
    background: transparent;
  }

  .live-indicator {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding: 2px 8px;
    border-radius: 10px;
    background: transparent;
    color: var(--text-dim);
  }

  /* When live, the indicator lights up in the locked-in live green. */
  .live-indicator.live {
    background: var(--live-bg);
    color: #eafff5;
  }

  .notice,
  .error {
    padding: 8px 14px;
    font-size: 13px;
  }

  .notice {
    background: var(--warn-bg);
    border-bottom: 1px solid var(--border);
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .notice-at {
    color: var(--text-dim);
  }

  .error {
    background: var(--danger-bg);
    color: var(--danger-text);
  }

  /* Windows 11 DPI/snap guard: avoid hardcoded 280px sidebars which clip at 125%/150%
     scaling and during snap. Use viewport-relative clamp + flex so the center editor always
     has room and sidebars shrink proportionally. The center uses minmax(0,1fr) and every
     direct child gets min-width:0 so min-content can never force the row wider than the
     window (inner lists scroll instead of clipping). */
  .body {
    flex: 1 1 0;
    display: grid;
    grid-template-columns: clamp(200px, 18vw, 300px) minmax(0, 1fr) clamp(200px, 18vw, 300px);
    min-height: 0;
    overflow: hidden;
  }
  .body > * {
    min-width: 0;
  }

  @media (max-width: 1180px) {
    .body {
      grid-template-columns: clamp(180px, 20vw, 260px) minmax(0, 1fr) clamp(180px, 20vw, 260px);
    }
  }

  @media (max-width: 960px) {
    .body {
      grid-template-columns: minmax(150px, 24vw) minmax(0, 1fr) minmax(150px, 24vw);
    }
  }

  /* Graceful degrade below ~700px effective width (e.g. a 1024px window at 150%
     zoom, or a small laptop at 175%+): stack the three regions in DOM order and
     let the page scroll. Nothing is hidden, collapsed, or overlapped — Show
     Output, the live status badges, and Settings stay reachable via scroll.
     Inner lists keep bounded heights so they scroll in place instead of making
     the page infinitely tall. */
  @media (max-width: 700px) {
    .body {
      grid-template-columns: 1fr;
      overflow-y: auto;
      overflow-x: hidden;
    }
    .sidebar {
      overflow: visible;
    }
    .sidebar-section .slide-list,
    .sidebar-section .song-list,
    .sidebar-section .scripture-list {
      max-height: 38vh;
    }
    .slide-grid {
      grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    }
  }

  .sidebar {
    background: var(--panel);
    border-right: 1px solid var(--border);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    overflow: hidden;
    min-height: 0;
  }

  .workspace-switch {
    display: flex;
    gap: 6px;
    padding: 4px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .ws-btn {
    flex: 1;
    padding: 6px 10px;
    font-size: 12px;
    font-weight: 600;
    border-radius: 6px;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-dim);
  }
  .ws-btn.active {
    background: var(--panel);
    border-color: var(--accent);
    color: var(--text);
    box-shadow: 0 0 0 3px rgba(79,140,255,0.12);
  }

  .sidebar-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    overflow: hidden;
  }

  .sidebar-section.has-content {
    flex: 1 1 0;
    min-height: 120px;
  }

  .sidebar-section:not(.has-content) {
    flex: 0 0 auto;
  }

  .sidebar-section .slide-list,
  .sidebar-section .song-list,
  .sidebar-section .scripture-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .sidebar-section:not(.has-content) .slide-list,
  .sidebar-section:not(.has-content) .song-list {
    flex: 0 0 auto;
    max-height: 120px;
  }

  .sidebar-section.scripture-section.active .scripture-list {
    flex: 1;
    min-height: 120px;
  }

  /* Phase 1 — Output panel is the representative screen for the warm/bold
     design system. Only this panel is rethemed; the rest of the app keeps
     existing tokens. Chrome stays on Inter/system stack; Output slide text
     stays on system fallback per font-not-found failure mode. */
  .output-panel {
    border-right: none;
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    background: linear-gradient(
      180deg,
      var(--panel) 0%,
      var(--brand-green-900) 100%
    );
    overflow-y: auto;
  }
  .output-sticky-top {
    position: sticky;
    top: -16px;
    background: linear-gradient(180deg, var(--panel) 0%, var(--brand-green-900) 100%);
    z-index: 3;
    margin: -16px -16px 0;
    padding: 12px 16px 12px;
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  /* Semantic status — color carries meaning */
  .output-panel .output-status {
    font-size: 13px;
    line-height: 1.5;
    color: var(--semantic-neutral);
    background: var(--semantic-neutral-bg);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-3);
    transition:
      color var(--motion-normal) var(--ease-standard),
      background var(--motion-normal) var(--ease-standard),
      border-color var(--motion-normal) var(--ease-standard);
  }
  .output-panel .output-status:has(.not-shown) {
    color: var(--semantic-idle);
    background: var(--semantic-neutral-bg);
    border-color: var(--border);
  }
  /* Live — Output shown + slide live: green dot convention extended */
  .output-panel .output-status.live {
    color: var(--semantic-live);
    background: var(--semantic-live-bg);
    border-color: var(--semantic-live-border);
    box-shadow: var(--semantic-live-glow);
  }

  /* Preview thumbnail + ON AIR badge — uses existing SlideRender at small size */
  .preview-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    margin: 4px 0 8px;
  }
  .preview-box {
    flex: 1;
    aspect-ratio: 16 / 9;
    width: 100%;
    max-width: 280px;
    background: #000;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    position: relative;
    box-shadow: var(--shadow-soft);
  }
  .preview-box :global(.slide-render) {
    /* Scale down the 72px Look sizes to fit the ~280px preview; SlideRender is absolute inset:0 */
    transform: scale(0.42);
    transform-origin: top left;
    width: 238%;
    height: 238%;
  }
  .preview-empty {
    display: grid;
    place-items: center;
    height: 100%;
    min-height: 100px;
    color: var(--text-dim);
    font-size: 12px;
    background: var(--panel-2);
  }
  .on-air-badge {
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding: 6px 10px;
    border-radius: 999px;
    border: 1px solid transparent;
    line-height: 1;
    white-space: nowrap;
    transition:
      background var(--motion-normal) var(--ease-standard),
      color var(--motion-normal) var(--ease-standard),
      border-color var(--motion-normal) var(--ease-standard),
      box-shadow var(--motion-normal) var(--ease-standard);
  }
  .on-air-badge.on {
    background: var(--semantic-live-bg);
    color: var(--semantic-live);
    border-color: var(--semantic-live-border);
    box-shadow: var(--semantic-live-glow);
  }
  .on-air-badge.off {
    background: var(--semantic-error-bg);
    color: var(--semantic-error);
    border-color: var(--semantic-error-border);
  }

  /* Render-ack heartbeat indicator — quiet when healthy, loud only on stale.
     Shown only while the corresponding window exists. */
  .ack-line {
    margin: 0;
    font-size: 11px;
    line-height: 1.4;
  }
  .ack-line.ok {
    color: var(--text-dim);
  }
  .ack-line.idle {
    color: var(--text-dim);
    opacity: 0.75;
  }
  .ack-line.stale {
    color: var(--semantic-warning, #f7b538);
    background: var(--semantic-warning-bg, rgba(247, 181, 56, 0.12));
    border: 1px solid var(--semantic-warning, #f7b538);
    border-radius: 6px;
    padding: 5px 8px;
    font-weight: 600;
  }

  .clear-row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin: 4px 0;
  }
  .clear-row button {
    flex: 1 1 auto;
    min-width: 80px;
    font-size: 12px;
    padding: 6px 8px;
  }

  /* Buttons in Output panel — subtle warm/bold feedback */
  .output-panel button.ghost {
    border-radius: var(--radius-md);
    transition:
      background var(--motion-fast) var(--ease-standard),
      border-color var(--motion-fast) var(--ease-standard),
      color var(--motion-fast) var(--ease-standard),
      transform var(--motion-fast) var(--ease-standard),
      box-shadow var(--motion-fast) var(--ease-standard);
  }
  .output-panel button.ghost:hover {
    background: var(--panel-2);
    border-color: var(--brand-slate-400);
    transform: translateY(-1px);
  }
  .output-panel button.ghost:active {
    transform: translateY(0);
  }
  .output-panel button.show-output {
    border-color: var(--semantic-live-border);
    color: var(--semantic-live);
    background: var(--semantic-live-bg);
  }
  .output-panel button.show-output:hover {
    background: rgba(31, 157, 106, 0.2);
    box-shadow: var(--semantic-live-glow);
  }

  .section-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
    margin-bottom: 8px;
  }

  .stage-title {
    margin-top: 18px;
    padding-top: 18px;
    border-top: 1px solid var(--border);
  }

  .slide-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1 1 0;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-gutter: stable;
  }

  .slide-list li {
    display: flex;
    align-items: stretch;
    gap: 4px;
  }

  .slide-entry {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    background: var(--panel-2);
  }

  .slide-entry.active {
    border-color: var(--live);
  }

  .slide-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .swatch {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    border: 1px solid var(--border);
    flex: none;
  }

  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }

  .swatches .swatch {
    width: 26px;
    height: 26px;
    padding: 0;
  }

  .swatches .swatch.selected {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .custom-color {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .custom-color input[type="color"] {
    width: 30px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--panel-2);
  }

  .media-swatch-wrap {
    position: relative;
    display: inline-flex;
  }

  .swatches .swatch.media {
    overflow: hidden;
  }

  .swatches .swatch.media img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .media-remove {
    position: absolute;
    top: -7px;
    right: -7px;
    width: 17px;
    height: 17px;
    padding: 0;
    border-radius: 50%;
    background: var(--danger);
    border: 1px solid var(--bg);
    color: #fff;
    font-size: 11px;
    line-height: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .swatches .media-add {
    font-size: 18px;
    line-height: 1;
    color: var(--text-dim);
  }

  .media-add:disabled {
    cursor: progress;
  }

  .media-spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    animation: spin 0.9s linear infinite;
  }

  /* Live-camera picker + badges — same quiet chrome as the media swatches. */
  .swatch.camera,
  .camera-badge {
    display: grid;
    place-items: center;
    font-size: 10px;
  }
  .camera-name {
    font-size: 11px;
    color: var(--text-dim);
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    align-self: center;
  }
  .camera-picker {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
  }
  .camera-picker-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
  }
  .camera-picker-close {
    border: none;
    background: transparent;
    color: var(--text-dim);
    font-size: 16px;
    line-height: 1;
    padding: 0 2px;
    cursor: pointer;
  }
  .camera-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 160px;
    overflow-y: auto;
  }
  .camera-entry {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    font-size: 12px;
  }
  .camera-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .camera-error {
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--danger-text, #fda4af);
  }

  .live-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--semantic-live);
    flex: none;
    animation: live-pulse 1800ms var(--ease-out) infinite alternate;
    box-shadow: var(--semantic-live-glow);
  }

  @keyframes live-pulse {
    from {
      box-shadow: 0 0 0 0 rgba(31, 157, 106, 0.35);
    }
    to {
      box-shadow: 0 0 0 6px rgba(31, 157, 106, 0);
    }
  }

  .delete {
    background: transparent;
    border-color: transparent;
    color: var(--text-dim);
    padding: 0 10px;
  }

  .delete:hover {
    color: var(--danger);
  }

  .add {
    width: 100%;
    margin-top: 10px;
    background: transparent;
  }

  .library-title {
    margin-top: 18px;
    padding-top: 18px;
    border-top: 1px solid var(--border);
  }

  .search {
    width: 100%;
    margin-bottom: 10px;
  }

  .scripture-title {
    margin-top: 18px;
    padding-top: 18px;
    border-top: 1px solid var(--border);
  }

  .scripture-wrap {
    position: relative;
  }

  .scripture-loading {
    position: absolute;
    right: 10px;
    top: 9px;
  }

  .scripture-list {
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    max-height: 220px;
    overflow-y: auto;
  }

  .scripture-entry {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 2px;
    text-align: left;
    background: transparent;
    border: none;
    border-radius: 0;
    padding: 6px 10px;
  }

  .scripture-entry:hover,
  .scripture-entry.active {
    background: var(--panel);
  }

  .scripture-ref {
    font-weight: 600;
    color: var(--accent);
    font-size: 12px;
  }

  .scripture-preview {
    font-size: 12px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
  }

  .scripture-fallback,
  .scripture-import {
    margin-top: 6px;
    font-size: 12px;
  }

  .scripture-status {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--text-dim);
  }

  .bibles-folder-hint {
    margin: 8px 0 0;
    font-size: 10px;
    color: var(--text-dim);
    line-height: 1.4;
    word-break: break-all;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 6px 8px;
  }

  .bibles-folder-hint code {
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 10px;
    word-break: break-all;
  }

  .song-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1 1 0;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-gutter: stable;
  }

  .song-list li {
    display: flex;
    align-items: stretch;
    gap: 4px;
  }

  .song-entry {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    background: var(--panel-2);
    padding: 6px 10px;
  }

  .song-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .song-count {
    font-size: 11px;
    color: var(--text-dim);
    flex: none;
  }

  .song-list .empty {
    font-size: 12px;
    color: var(--text-dim);
    padding: 4px;
  }

  .editor {
    padding: 16px;
    overflow-y: auto;
  }

  .edit-window {
    max-width: 560px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .detail-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 0 12px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 16px;
    position: sticky;
    top: 0;
    background: var(--bg);
    z-index: 2;
  }
  .detail-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .grid-toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 0 12px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 12px;
    position: sticky;
    top: 0;
    background: var(--bg);
    z-index: 1;
  }
  .slide-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 14px;
    align-content: start;
    padding: 4px 2px 20px;
  }
  .grid-empty {
    text-align: center;
    padding: 40px 20px;
  }
  .grid-drop-indicator {
    grid-column: 1 / -1;
    height: 3px;
    background: var(--accent);
    border-radius: 2px;
    animation: drop-pulse 0.8s ease-in-out infinite alternate;
  }
  @keyframes drop-pulse {
    from { opacity: 0.5; }
    to { opacity: 1; }
  }
  .grid-cell {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 8px;
    transition: border-color var(--motion-fast, 150ms) var(--ease-standard, ease), transform var(--motion-fast) var(--ease-standard), box-shadow var(--motion-fast) var(--ease-standard);
  }
  .grid-cell.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px rgba(79,140,255,0.15);
  }
  .grid-cell.live {
    border-color: var(--live);
    box-shadow: 0 0 0 3px rgba(31,157,106,0.2);
  }
  .grid-cell:hover {
    transform: translateY(-2px);
    box-shadow: var(--shadow-soft, 0 4px 16px rgba(0,0,0,0.22));
  }
  .grid-thumb {
    position: relative;
    aspect-ratio: 16 / 9;
    background: #000;
    border-radius: 6px;
    overflow: hidden;
    border: 1px solid var(--border);
    padding: 0;
    width: 100%;
  }
  .grid-thumb-inner {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }
  .grid-thumb-inner :global(.slide-render) {
    transform: scale(0.32);
    transform-origin: top left;
    width: 312%;
    height: 312%;
  }
  .grid-thumb-fallback {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--text-dim);
    font-size: 11px;
    padding: 8px;
    text-align: center;
  }
  .grid-live-badge {
    position: absolute;
    top: 6px;
    right: 6px;
    background: var(--live);
    color: white;
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.06em;
    padding: 3px 6px;
    border-radius: 4px;
    line-height: 1;
  }
  .grid-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: center;
    padding: 2px 4px;
  }
  .grid-actions {
    display: flex;
    gap: 6px;
    justify-content: center;
  }
  .grid-go-live {
    font-size: 11px;
    padding: 4px 8px;
    flex: 1;
  }
  .grid-delete {
    font-size: 14px;
    padding: 4px 8px;
    line-height: 1;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    color: var(--text-dim);
  }

  input[type="text"],
  textarea,
  select {
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 10px;
    color: var(--text);
    width: 100%;
  }

  textarea {
    resize: vertical;
  }

  .field-label {
    font-size: 12px;
    color: var(--text-dim);
  }

  .empty {
    color: var(--text-dim);
    padding: 40px;
  }

  .output-status {
    font-size: 13px;
    color: var(--text-dim);
  }

  .not-shown {
    color: var(--text);
    font-weight: 600;
  }

  .show-output {
    border-color: var(--live);
    color: var(--live);
  }

  .welcome {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 12px;
    margin-bottom: 14px;
    background: var(--live-bg);
    border: 1px solid var(--color-green);
    border-radius: 8px;
    font-size: 13px;
    line-height: 1.5;
  }

  .welcome p {
    margin: 3px 0 0;
    color: var(--text-dim);
  }

  /* Contextual onboarding — quiet one-line hints. Muted text, no popups;
     each hides forever once its feature is used or its × is clicked. */
  .hint-line {
    margin: 0;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-dim);
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 8px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .hint-line kbd {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 4px;
    background: var(--panel);
    white-space: nowrap;
  }
  .hint-x {
    margin-left: auto;
    flex: none;
    border: none;
    background: transparent;
    color: var(--text-dim);
    padding: 0 2px;
    line-height: 1;
    cursor: pointer;
  }
  .hint-x:hover {
    color: var(--text);
  }

  /* Guided-tour target highlight — gentle accent outline on the panel the
     current step describes. The card itself never blocks the rest of the UI. */
  .tour-highlight {
    outline: 2px solid var(--accent, #4f8cff);
    outline-offset: -2px;
    border-radius: 8px;
  }

  .welcome-dismiss {
    background: transparent;
    border: none;
    color: var(--text-dim);
    padding: 0 2px;
    margin-left: auto;
    flex: none;
  }

  .loading-shell {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    color: var(--text-dim);
    font-size: 13px;
  }

  .spinner {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    border: 3px solid var(--border);
    border-top-color: var(--accent);
    animation: spin 0.9s linear infinite;
  }

  .loading-error {
    color: var(--danger);
    max-width: 60%;
    text-align: center;
  }

  /* Browse Scripture panel — FreeShow-inspired, collapsible */
  .browse-panel {
    margin-top: 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel-2);
    overflow: hidden;
  }
  .browse-header {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: transparent;
    border: none;
    padding: 10px 12px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-dim);
    cursor: pointer;
  }
  .browse-header:hover {
    background: var(--panel);
  }
  .browse-toggle {
    font-size: 12px;
    color: var(--text-dim);
  }
  .browse-error {
    color: var(--danger);
    font-size: 12px;
  }
  .browse-books {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 140px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    padding: 4px;
  }
  .browse-book {
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 6px 8px;
    font-size: 12px;
    color: var(--text);
  }
  .browse-book:hover {
    background: var(--panel-2);
  }
  .browse-book.active {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }
  .browse-chapters {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .chapter-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(36px, 1fr));
    gap: 4px;
  }
  .chapter-pill {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 6px 0;
    font-size: 12px;
    text-align: center;
  }
  .chapter-pill.active {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }
  .browse-verses {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 180px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    padding: 4px;
  }
  .browse-verse {
    display: flex;
    gap: 8px;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 6px 8px;
    width: 100%;
  }
  .browse-verse:hover {
    background: var(--panel-2);
    border-color: var(--border);
  }
  .verse-num {
    font-weight: 700;
    color: var(--accent);
    font-size: 11px;
    min-width: 18px;
  }
  .verse-text {
    font-size: 11px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .browse-hint {
    font-size: 10px;
    color: var(--text-dim);
    margin: 6px 0 0;
    line-height: 1.4;
    font-style: italic;
  }

  /* Bottom-docked full-width Browse panel — pushes main content up, not overlay.
     Columns are flexible (shrink proportionally) with viewport-capped minimums
     so the dock never forces a horizontal scrollbar until the window is truly
     tiny; below 960px it stacks vertically. */
  .browse-dock {
    display: flex;
    gap: 16px;
    padding: 16px;
    border-top: 1px solid var(--border);
    background: var(--panel);
    min-height: 180px;
    max-height: 42vh;
    overflow: auto;
    position: relative;
    flex: 0 0 auto;
    resize: vertical;
  }
  .browse-dock-left {
    flex: 0 1 220px;
    min-width: min(160px, 100%);
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-right: 1px solid var(--border);
    padding-right: 16px;
    overflow-y: auto;
  }
  .browse-dock-middle {
    flex: 0 1 280px;
    min-width: min(200px, 100%);
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-right: 1px solid var(--border);
    padding-right: 16px;
    overflow-y: auto;
  }
  .browse-dock-right {
    flex: 1 1 280px;
    min-width: min(240px, 100%);
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
  }
  .browse-dock .browse-books {
    max-height: none;
    flex: 1;
    min-height: 120px;
  }
  .browse-dock .browse-verses {
    max-height: none;
    flex: 1;
    min-height: 120px;
  }
  .browse-placeholder {
    color: var(--text-dim);
    font-size: 12px;
    padding: 12px;
    text-align: center;
    font-style: italic;
  }
  .browse-dock-close {
    position: absolute;
    top: 8px;
    right: 12px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 8px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .browse-dock-close:hover {
    background: var(--panel-2);
    color: var(--text);
  }

  @media (max-width: 960px) {
    .browse-dock {
      flex-direction: column;
      max-height: 50vh;
      overflow-y: auto;
    }
    .browse-dock-left,
    .browse-dock-middle,
    .browse-dock-right {
      width: 100%;
      min-width: 0;
      flex: none;
      border-right: none;
      border-bottom: 1px solid var(--border);
      padding-right: 0;
      padding-bottom: 12px;
    }
  }

  .library-verse-row {
    margin-left: 8px;
  }
  .library-verse {
    display: flex;
    flex-direction: column;
    gap: 2px;
    text-align: left;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 5px 8px;
    width: 100%;
    font-size: 11px;
  }
  .library-verse:hover {
    background: var(--panel);
  }
  .verse-title {
    font-weight: 600;
    color: var(--text);
    font-size: 11px;
  }
  .verse-preview {
    color: var(--text-dim);
    font-size: 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Drag-and-drop visual feedback */
  .slide-list.drag-active {
    background: rgba(79, 140, 255, 0.04);
    border-radius: 6px;
  }
  .slide-list li.dragging {
    opacity: 0.45;
    transform: scale(0.98);
  }
  .slide-list li.dragging .slide-entry {
    border-color: var(--accent);
    box-shadow: 0 2px 8px rgba(79, 140, 255, 0.25);
  }
  .drop-indicator {
    height: 3px;
    background: var(--accent);
    border-radius: 2px;
    margin: 4px 8px;
    animation: drop-pulse 600ms ease-in-out infinite alternate;
  }
  @keyframes drop-pulse {
    from { opacity: 0.6; }
    to { opacity: 1; }
  }
  .song-entry[draggable="true"],
  .scripture-entry[draggable="true"],
  .browse-verse[draggable="true"],
  .library-verse[draggable="true"] {
    cursor: grab;
  }
  .song-entry[draggable="true"]:active,
  .scripture-entry[draggable="true"]:active,
  .browse-verse[draggable="true"]:active,
  .library-verse[draggable="true"]:active {
    cursor: grabbing;
  }

  .template-actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .template-btn {
    flex: 1;
    font-size: 11px;
    padding: 6px 8px;
  }

  .field-hint {
    font-size: 11px;
    color: var(--text-dim);
    margin-top: 4px;
    display: block;
    font-style: italic;
  }

  .title-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .title-row input {
    flex: 1;
    min-width: 0;
  }
  .title-case-btn {
    flex: 0 0 auto;
    font-size: 12px;
    font-weight: 700;
    padding: 7px 10px;
    letter-spacing: 0.02em;
  }

  .auto-badge {
    font-size: 10px;
    font-weight: 700;
    color: var(--accent);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 1px 6px;
    margin-left: 6px;
    background: var(--panel-2);
  }

  .external-drop-zone {
    margin-top: 10px;
    border: 1.5px dashed var(--border);
    border-radius: 8px;
    padding: 10px 12px;
    text-align: center;
    background: var(--panel-2);
    color: var(--text-dim);
    font-size: 11px;
    transition:
      border-color 150ms ease,
      background 150ms ease,
      color 150ms ease;
  }
  .external-drop-zone.drag-active {
    border-color: var(--accent);
    background: rgba(79, 140, 255, 0.08);
    color: var(--text);
    box-shadow: 0 2px 10px rgba(79, 140, 255, 0.15);
  }
  .slide-list.external-drag {
    outline: 1.5px dashed var(--accent);
    outline-offset: 2px;
    background: rgba(79, 140, 255, 0.04);
  }
  .drop-zone-label {
    display: block;
  }
  .drop-error {
    display: block;
    margin-top: 6px;
    color: var(--semantic-error, #e11d48);
    font-size: 11px;
    font-weight: 600;
    word-break: break-word;
  }

  .library-drop-zone {
    margin-top: 10px;
    border: 1.5px dashed var(--border);
    border-radius: 8px;
    padding: 9px 10px;
    text-align: center;
    background: var(--panel-2);
    color: var(--text-dim);
    font-size: 10px;
    transition:
      border-color 150ms ease,
      background 150ms ease;
  }
  .library-drop-zone.drag-active {
    border-color: var(--accent);
    background: rgba(79, 140, 255, 0.08);
    color: var(--text);
  }
  .library-section.library-drag-active {
    outline: 1.5px dashed var(--accent);
    outline-offset: 2px;
    background: rgba(79, 140, 255, 0.03);
  }

  .arrangement-row {
    margin-top: 8px;
    padding: 8px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .arrangement-label {
    font-size: 10px;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 6px;
    display: block;
  }
  .chip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 4px 8px;
    font-size: 11px;
    font-weight: 500;
  }
  .chip-btn {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    width: 20px;
    height: 20px;
    font-size: 10px;
    line-height: 1;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .chip-btn:disabled {
    opacity: 0.3;
  }
  .chip-remove {
    color: var(--semantic-error, #e11d48);
    border-color: var(--semantic-error, #e11d48);
  }
  .arrangement-actions {
    margin-top: 6px;
  }
  .arrangement-add {
    font-size: 11px;
    padding: 4px 8px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text);
  }

  .stage-message-panel {
    margin-top: 12px;
    padding: 10px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .stage-message-current {
    display: flex;
    align-items: center;
    gap: 8px;
    background: rgba(225, 29, 72, 0.12);
    border: 1px solid var(--semantic-error, #e11d48);
    border-radius: 6px;
    padding: 6px 8px;
    font-size: 11px;
    color: var(--text);
  }
  .stage-message-text {
    flex: 1;
    font-weight: 600;
    word-break: break-word;
  }
  .stage-message-row {
    display: flex;
    gap: 6px;
  }
  .stage-message-row input[type="text"] {
    flex: 1;
    min-width: 0;
  }
  .stage-message-row input[type="number"] {
    width: 72px;
    flex: 0 0 72px;
  }
  .stage-message-duration {
    text-align: center;
  }
  .stage-message-actions {
    display: flex;
    gap: 6px;
  }
  .stage-message-actions button {
    flex: 1;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
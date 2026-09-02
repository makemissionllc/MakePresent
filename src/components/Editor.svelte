<script lang="ts">
  import { onMount } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, subscribeState, subscribeAutosave, subscribeLibrary } from "../lib/sync";
  import type { BibleInfo, ChapterVerse, ClientState, DisplayInfo, Library, LibrarySong, ScriptureMatch, Slide } from "../lib/types";
  import { isMedia } from "../lib/types";
  import SettingsPanel from "./SettingsPanel.svelte";
  import Modal from "./Modal.svelte";
  import SlideRender from "./SlideRender.svelte";
  import SongEditorModal from "./SongEditorModal.svelte";
  import ProjectHub from "../lib/components/ProjectHub.svelte";

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

  // Add song modal (reusable Modal, replaces window.prompt "localhost:1420 says")
  let showAddSongTitleModal = $state(false);
  let showAddSongBodyModal = $state(false);
  let showSongEditor = $state(false);
  let pendingSongTitle = $state("");
  let pendingSongBody = $state("");

  // Project Hub (Startup launcher)
  let showHub = $state(false);

  // Draft copies for responsive editing — typing updates these immediately
  // while the backend save is debounced so the input never resets mid-keystroke.
  let draftTitle = $state("");
  let draftBody = $state("");
  let draftId: string | null = $state(null);
  let titleTimer: ReturnType<typeof setTimeout> | null = null;
  let bodyTimer: ReturnType<typeof setTimeout> | null = null;

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

  // Keep draftTitle/draftBody in sync when selection changes — only when the
  // underlying slide identity changes, so mid-edit keystrokes are never clobbered.
  $effect(() => {
    const s = selected;
    if (!s) {
      draftTitle = "";
      draftBody = "";
      draftId = null;
      return;
    }
    if (draftId !== s.id) {
      draftId = s.id;
      draftTitle = s.title;
      draftBody = s.body;
      if (titleTimer) clearTimeout(titleTimer);
      if (bodyTimer) clearTimeout(bodyTimer);
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

  function clearOutput(): void {
    void run(() => api.clearOutput());
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
    if (!project) {
      errorMsg = "Project not loaded yet";
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
      const verse = song?.slides.find((x) => x.id === payload.slideId);
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
    void api.toggleStage().catch((e: unknown) => (errorMsg = String(e)));
  }

  function addToPlaylist(song: LibrarySong): void {
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

  onMount(() => {
    let unSub: () => void = () => {};
    let unAuto: () => void = () => {};
    let unLib: () => void = () => {};
    let cancelled = false;

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
        const s = await api.getState();
        if (!cancelled) {
          appState = s;
          selectedId = s.project.live ?? s.project.slides[0]?.id ?? null;
          // Boot hub — Affinity-style launcher
          showHub = true;
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

    return () => {
      cancelled = true;
      unSub();
      unAuto();
      unLib();
      if (scriptureTimer) clearTimeout(scriptureTimer);
      if (titleTimer) clearTimeout(titleTimer);
      if (bodyTimer) clearTimeout(bodyTimer);
    };
  });
</script>

{#if appState === null}
  <div class="loading-shell">
    <div class="spinner" aria-hidden="true"></div>
    <p>Starting MakePresent…</p>
    {#if errorMsg}
      <p class="loading-error">{errorMsg}</p>
    {/if}
  </div>
{:else}
<div class="shell">
  <header class="topbar">
    <h1>MakePresent</h1>
    <span class="project-name">{project?.name ?? "No project"}</span>
    <span class="live-indicator" class:live={!!project?.live}>
      {#if project?.live}LIVE{:else}OFFLINE{/if}
    </span>
    <span class="spacer"></span>
    <span class="saved-label">{savedLabel}</span>
    <button class="ghost" onclick={() => newProject()}>New project</button>
    <button class="ghost" onclick={() => clearOutput()}>Clear output</button>
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
            <strong>Welcome to MakePresent</strong>
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

      <div class="sidebar-section playlist-section" class:has-content={(project?.slides.length ?? 0) > 0}>
        <div class="section-title">Playlist</div>
        <ul
          class="slide-list"
          class:drag-active={isDragging}
          ondragover={(e) => { e.preventDefault(); if (dragOverIndex === null) dragOverIndex = project?.slides.length ?? 0; }}
          ondrop={(e) => onPlaylistDrop(e)}
          ondragleave={(e) => {
            const rt = e.relatedTarget as HTMLElement | null;
            if (!rt || !(e.currentTarget as HTMLElement).contains(rt)) dragOverIndex = null;
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
                  style:background-color={slide.background.type === "solid"
                    ? slide.background.color
                    : "#000"}
                  style:background-image={isMedia(slide.background)
                    ? `url('${fileUrl(slide.background.thumb)}')`
                    : "none"}
                  style:background-size="cover"
                  style:background-position="center"
                ></span>
                <span class="slide-label">{slide.title || "Untitled"}</span>
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

      <div class="browse-panel">
        <button class="browse-header" onclick={() => (browseCollapsed = !browseCollapsed)} aria-expanded={!browseCollapsed}>
          <span class="section-title" style="margin:0; border:none; padding:0;">Browse Scripture</span>
          <span class="browse-toggle">{browseCollapsed ? "▸ Show" : "▾ Hide"}</span>
        </button>
        {#if !browseCollapsed}
          <p class="browse-hint">Browsing as full-width panel below — click a verse to add as slide (drag secondary).</p>
        {/if}
      </div>

      <div class="sidebar-section library-section" class:has-content={librarySongs.length > 0 || librarySearch.trim().length > 0}>
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
                title="Drag to playlist to add • Click to add"
              >
                <span class="song-label">{song.title || "Untitled"}</span>
                <span class="song-count">{song.slides.length} {song.slides.length === 1 ? "slide" : "slides"}</span>
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
            {#each song.slides as verse (verse.id)}
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
          {:else}
            <li class="empty">No songs yet. Add one below.</li>
          {/each}
        </ul>
        <button class="add" onclick={() => addLibrarySong()}>+ Add song</button>
      </div>
    </aside>

    <main class="editor">
      {#if selected}
        <div class="edit-window">
          <label>
            Title
            <input
              type="text"
              value={draftTitle}
              placeholder="Slide title"
              oninput={(e) => onTitleInput((e.target as HTMLInputElement).value)}
              onblur={() => flushTitle()}
            />
          </label>
          <label>
            Body
            <textarea
              rows="8"
              value={draftBody}
              placeholder="Slide body text"
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
            </div>
          </div>
        </div>
      {:else}
        <div class="empty">No slide selected. Add a slide to get started.</div>
      {/if}
    </main>

    <aside class="sidebar output-panel">
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
            <SlideRender slide={outputPreviewSlide} look={outputPreviewLook} />
          {:else}
            <div class="preview-empty">No slide</div>
          {/if}
        </div>
        <span class="on-air-badge" class:on={isOnAir} class:off={!isOnAir}>{isOnAir ? "ON AIR" : "OFF"}</span>
      </div>

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

      {#if !appState?.output.visible}
        <button class="ghost show-output" onclick={() => showOutput()}>
          Show Output
        </button>
      {/if}

      <div class="section-title stage-title">Stage Display</div>

      <button class="ghost" onclick={() => toggleStage()}>
        {appState?.stage.visible ? "Hide stage" : "Show stage"}
      </button>

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
            <SlideRender slide={stagePreviewSlide} look={stagePreviewLook} />
          {:else}
            <div class="preview-empty">No slide</div>
          {/if}
        </div>
        <span class="on-air-badge" class:on={isStageOnAir} class:off={!isStageOnAir}>{isStageOnAir ? "ON AIR" : "OFF"}</span>
      </div>

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
  onClose={() => (showHub = false)}
  onCreate={handleHubCreate}
/>

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
  }

  .spacer {
    flex: 1;
  }

  .saved-label {
    font-size: 12px;
    color: var(--text-dim);
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
     has room and sidebars shrink proportionally. */
  .body {
    flex: 1 1 0;
    display: grid;
    grid-template-columns: clamp(220px, 18vw, 300px) minmax(320px, 1fr) clamp(220px, 18vw, 300px);
    min-height: 0;
    overflow: hidden;
  }

  @media (max-width: 1180px) {
    .body {
      grid-template-columns: clamp(200px, 20vw, 260px) minmax(280px, 1fr) clamp(200px, 20vw, 260px);
    }
  }

  @media (max-width: 960px) {
    .body {
      grid-template-columns: 220px 1fr 220px;
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

  /* Bottom-docked full-width Browse panel — pushes main content up, not overlay */
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
    width: 220px;
    min-width: 180px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-right: 1px solid var(--border);
    padding-right: 16px;
    overflow-y: auto;
  }
  .browse-dock-middle {
    width: 280px;
    min-width: 220px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-right: 1px solid var(--border);
    padding-right: 16px;
    overflow-y: auto;
  }
  .browse-dock-right {
    flex: 1;
    min-width: 300px;
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
    .browse-dock-middle {
      width: 100%;
      min-width: 0;
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

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
<script lang="ts">
  import { onMount } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, subscribeState, subscribeAutosave, subscribeLibrary } from "../lib/sync";
  import type { ClientState, DisplayInfo, Library, LibrarySong, ScriptureMatch, Slide } from "../lib/types";
  import { isMedia } from "../lib/types";
  import SettingsPanel from "./SettingsPanel.svelte";

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
    if (!q.trim()) {
      scriptureResults = [];
      scriptureOpen = false;
      scriptureIdx = -1;
      return;
    }
    try {
      const matches = await api.searchScripture(q.trim());
      scriptureResults = matches;
      scriptureOpen = matches.length > 0;
      scriptureIdx = -1;
    } catch (e) {
      scriptureResults = [];
      scriptureOpen = false;
      errorMsg = String(e);
    } finally {
      scriptureLoading = false;
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

  function updateTitle(slide: Slide, value: string): void {
    void run(() => api.updateSlide(slide.id, { title: value }));
  }

  function updateBody(slide: Slide, value: string): void {
    void run(() => api.updateSlide(slide.id, { body: value }));
  }

  function setColor(slide: Slide, color: string): void {
    void run(() =>
      api.updateSlide(slide.id, { background: { type: "solid", color } }),
    );
  }

  function fileUrl(path: string): string {
    return convertFileSrc(path);
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
    void run(() => api.deleteSlide(slide.id));
    if (selectedId === slide.id) selectedId = null;
  }

  function onDisplayChange(e: Event): void {
    const target = e.target as HTMLSelectElement;
    const index = Number(target.value);
    void api
      .setOutputDisplay(index)
      .then((d) => (displays = d))
      .catch((e: unknown) => (errorMsg = String(e)));
  }

  function onStageDisplayChange(e: Event): void {
    const target = e.target as HTMLSelectElement;
    const index = Number(target.value);
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
    const title = window.prompt("Song title");
    if (!title) return;
    const body = window.prompt("Lyrics / body text") ?? "";
    void api
      .addLibrarySong(title, body)
      .then((l) => (library = l))
      .catch((e: unknown) => (errorMsg = String(e)));
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

  async function newProject(): Promise<void> {
    try {
      welcomeDismissed = true;
      const s = await api.newProject();
      appState = s;
      selectedId = s.project.live ?? s.project.slides[0]?.id ?? null;
    } catch (e) {
      errorMsg = String(e);
    }
  }

  onMount(() => {
    let unSub: () => void = () => {};
    let unAuto: () => void = () => {};
    let unLib: () => void = () => {};

    void (async () => {
      unSub = await subscribeState((s) => {
        appState = s;
      });
      unAuto = await subscribeAutosave((e) => {
        savedLabel =
          e.status === "saved" ? `Saved ${formatAt(e.at)}` : `Autosave failed: ${e.message ?? "unknown"}`;
      });
      unLib = await subscribeLibrary((l) => {
        library = l;
      });
      try {
        const s = await api.getState();
        appState = s;
        selectedId = s.project.live ?? s.project.slides[0]?.id ?? null;
      } catch (e) {
        errorMsg = String(e);
      }
      api.listDisplays().then((d) => (displays = d)).catch((e: unknown) => (errorMsg = String(e)));
      api.getLibrary().then((l) => (library = l)).catch((e: unknown) => (errorMsg = String(e)));
    })();

    return () => {
      unSub();
      unAuto();
      unLib();
      if (scriptureTimer) clearTimeout(scriptureTimer);
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
    <span class="live-indicator"> {#if project?.live}LIVE{:else}OFFLINE{/if} </span>
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

      <div class="section-title">Playlist</div>
      <ul class="slide-list">
        {#each project?.slides ?? [] as slide (slide.id)}
          <li>
            <button
              class:active={project?.live === slide.id}
              class="slide-entry"
              onclick={() => goLive(slide)}
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
      </ul>
      <button class="add" onclick={() => addSlide()}>+ Add slide</button>

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
                  onmousedown={(e) => {
                    e.preventDefault();
                    selectScripture(match);
                  }}
                >
                  <span class="scripture-ref">{match.reference}</span>
                  <span class="scripture-preview">{match.text}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>

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
            <button class="song-entry" onclick={() => addToPlaylist(song)}>
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
        {:else}
          <li class="empty">No songs yet. Add one below.</li>
        {/each}
      </ul>
      <button class="add" onclick={() => addLibrarySong()}>+ Add song</button>
    </aside>

    <main class="editor">
      {#if selected}
        <div class="edit-window">
          <label>
            Title
            <input
              type="text"
              value={selected.title}
              placeholder="Slide title"
              oninput={(e) => updateTitle(selected, (e.target as HTMLInputElement).value)}
            />
          </label>
          <label>
            Body
            <textarea
              rows="8"
              value={selected.body}
              placeholder="Slide body text"
              oninput={(e) => updateBody(selected, (e.target as HTMLTextAreaElement).value)}
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
        >
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

      <div class="output-status">
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
        >
          {#each displays ?? [] as d}
            <option value={d.index}>
              {d.name || `Display ${d.index + 1}`} &middot; {d.width}&times;{d.height}{d.primary
                ? " (primary)"
                : ""}
            </option>
          {/each}
        </select>
      </label>

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
</div>
{/if}

{#if settingsOpen}
  <SettingsPanel app={appState} onclose={() => (settingsOpen = false)} />
{/if}

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .topbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 14px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
  }

  .topbar h1 {
    font-size: 15px;
    font-weight: 600;
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

  .notice,
  .error {
    padding: 8px 14px;
    font-size: 13px;
  }

  .notice {
    background: #4a3b17;
    border-bottom: 1px solid var(--border);
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .notice-at {
    color: var(--text-dim);
  }

  .error {
    background: #4a1f1f;
    color: #ffd9d9;
  }

  .body {
    flex: 1;
    display: grid;
    grid-template-columns: 280px 1fr 280px;
    min-height: 0;
  }

  .sidebar {
    background: var(--panel);
    border-right: 1px solid var(--border);
    padding: 12px;
    overflow-y: auto;
  }

  .output-panel {
    border-right: none;
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 12px;
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
    background: var(--live);
    flex: none;
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

  .song-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
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
    background: #17233d;
    border: 1px solid #2c4a7a;
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

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
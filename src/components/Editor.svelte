<script lang="ts">
  import { onMount } from "svelte";
  import { api, subscribeState, subscribeAutosave } from "../lib/sync";
  import type { ClientState, DisplayInfo, Slide } from "../lib/types";

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

  let appState = $state<ClientState | null>(null);
  let selectedId = $state<string | null>(null);
  let displays = $state<DisplayInfo[] | null>(null);
  let savedLabel = $state<string>("Not saved");
  let errorMsg = $state<string | null>(null);
  let noticeDismissed = $state(false);

  const project = $derived(appState?.project ?? null);
  const notice = $derived(
    appState?.notice && !noticeDismissed ? appState.notice : null,
  );
  const selected = $derived(
    project?.slides.find((s) => s.id === selectedId) ??
      project?.slides[0] ??
      null,
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

  function toggleFullscreen(): void {
    void api.toggleOutputFullscreen().catch((e: unknown) => (errorMsg = String(e)));
  }

  async function newProject(): Promise<void> {
    try {
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

    void (async () => {
      unSub = await subscribeState((s) => {
        appState = s;
      });
      unAuto = await subscribeAutosave((e) => {
        savedLabel =
          e.status === "saved" ? `Saved ${formatAt(e.at)}` : `Autosave failed: ${e.message ?? "unknown"}`;
      });
      try {
        const s = await api.getState();
        appState = s;
        selectedId = s.project.live ?? s.project.slides[0]?.id ?? null;
      } catch (e) {
        errorMsg = String(e);
      }
      api.listDisplays().then((d) => (displays = d)).catch((e: unknown) => (errorMsg = String(e)));
    })();

    return () => {
      unSub();
      unAuto();
    };
  });
</script>

<div class="shell">
  <header class="topbar">
    <h1>MakePresent</h1>
    <span class="project-name">{project?.name ?? "No project"}</span>
    <span class="live-indicator"> {#if project?.live}LIVE{:else}OFFLINE{/if} </span>
    <span class="spacer"></span>
    <span class="saved-label">{savedLabel}</span>
    <button class="ghost" onclick={() => newProject()}>New project</button>
    <button class="ghost" onclick={() => clearOutput()}>Clear output</button>
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

      <div class="output-status">
        {#if project?.live}
          Live on display
          {appState?.output.monitorName ||
            (appState?.output.monitorIndex != null
              ? `#${appState.output.monitorIndex}`
              : "?")}
        {:else}
          Output is black (no live slide)
        {/if}
      </div>
    </aside>
  </div>
</div>

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
</style>
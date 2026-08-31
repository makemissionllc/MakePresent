<script lang="ts">
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/sync";
  import type { ClientState, LogEntry, Look, LookPatch, TextPosition } from "../lib/types";

  interface Props {
    app: ClientState | null;
    onclose: () => void;
  }

  let { app: appState, onclose }: Props = $props();

  let tab = $state<"general" | "looks" | "logs">("general");
  let status = $state<{ kind: "ok" | "err"; text: string } | null>(null);
  let logs = $state<LogEntry[]>([]);
  let logsMsg = $state<string | null>(null);
  let lookErr = $state<string | null>(null);

  const looks = $derived(appState?.looks ?? []);
  let activeLookId = $state<string | null>(null);
  const activeLook = $derived.by(() => {
    if (!activeLookId) return looks[0] ?? null;
    return looks.find((l) => l.id === activeLookId) ?? looks[0] ?? null;
  });

  // Optimistic editing: local copy of the selected Look so every keystroke
  // re-renders the output immediately, then fire-and-forget the IPC.
  let draft: Look | null = $state(null);

  $effect(() => {
    if (tab !== "looks") return;
    draft = activeLook ? { ...activeLook } : null;
  });

  function commit(): void {
    if (!draft) return;
    scheduleCommit(draft);
  }

  function selectLook(id: string): void {
    activeLookId = id;
    if (commitTimer) clearTimeout(commitTimer);
  }

  function setDraft(field: keyof Look, value: unknown): void {
    lookErr = null;
    if (!draft) return;
    // Optimistic UI: update the shared appState.looks immediately so every
    // window (Output/Stage) reflects the change with zero IPC latency.
    const updated = { ...draft, [field]: value } as Look;
    draft = updated;
    if (appState) {
      appState = {
        ...appState,
        looks: appState.looks.map((l) => (l.id === updated.id ? updated : l)),
      };
    }
    // Only persist the really-committed patch, debounced per keystroke.
    void scheduleCommit(updated);
  }

  let commitTimer: ReturnType<typeof setTimeout> | null = null;
  function scheduleCommit(updated: Look): void {
    const patch: LookPatch = {
      name: updated.name,
      titleSize: updated.titleSize,
      bodySize: updated.bodySize,
      textColor: updated.textColor,
      showBackground: updated.showBackground,
      textPosition: updated.textPosition,
    };
    if (commitTimer) clearTimeout(commitTimer);
    commitTimer = setTimeout(() => {
      void api.upsertLook(updated.id, patch).catch((e: unknown) => (lookErr = String(e)));
    }, 200);
  }

  function addLook(): void {
    lookErr = null;
    void api
      .upsertLook(null, {
        name: `Look ${looks.length + 1}`,
        titleSize: 60,
        bodySize: 40,
        textColor: "#ffffff",
        showBackground: true,
        textPosition: "center",
      })
      .then((s) => {
        appState = s;
        const created = s.looks.at(-1);
        if (created) activeLookId = created.id;
      })
      .catch((e: unknown) => (lookErr = String(e)));
  }

  function deleteLook(): void {
    if (!activeLook) return;
    lookErr = null;
    void api
      .deleteLook(activeLook.id)
      .then((s) => {
        appState = s;
        activeLookId = null;
      })
      .catch((e: unknown) => (lookErr = String(e)));
  }

  function assignTo(target: "output" | "stage", id: string | null): void {
    lookErr = null;
    const fn = target === "output" ? api.setOutputLook : api.setStageLook;
    void fn(id).then((s) => (appState = s)).catch((e: unknown) => (lookErr = String(e)));
  }

  const summary = $derived(
    appState
      ? [
          {
            label: "Output display",
            value:
              appState.output.monitorName ||
              (appState.output.monitorIndex != null
                ? `Display #${appState.output.monitorIndex}`
                : "Auto (picked on first show)"),
          },
          {
            label: "Output fullscreen",
            value: appState.output.fullscreen ? "On" : "Off",
          },
          {
            label: "Stage display",
            value:
              appState.stage.monitorName ||
              (appState.stage.monitorIndex != null
                ? `Display #${appState.stage.monitorIndex}`
                : "Auto"),
          },
          {
            label: "Stage visible",
            value: appState.stage.visible ? "On" : "Off",
          },
          {
            label: "Default transition",
            value: appState.defaultTransition === "fade" ? "Fade" : "Cut",
          },
        ]
      : [],
  );

  async function run(fn: () => Promise<void>): Promise<void> {
    try {
      await fn();
    } catch (e) {
      status = { kind: "err", text: String(e) };
    }
  }

  function exportSettings(): void {
    void run(async () => {
      status = null;
      const path = await saveDialog({
        defaultPath: "makepresent-settings.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      const report = await api.exportSettings(path);
      status = {
        kind: "ok",
        text: `Exported settings to ${report.path} (${report.fields.length} fields).`,
      };
    });
  }

  function importSettings(): void {
    void run(async () => {
      status = null;
      const file = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!file) return;
      const path = Array.isArray(file) ? file[0] : file;
      const report = await api.importSettings(path);
      status = { kind: "ok", text: report.message };
    });
  }

  async function loadLogs(): Promise<void> {
    logsMsg = null;
    try {
      logs = await api.getLogs(300);
    } catch (e) {
      logsMsg = String(e);
    }
  }

  async function copyLogs(): Promise<void> {
    const text = logs
      .map((l) => `${l.at} ${l.level} ${l.message}`)
      .join("\n");
    try {
      await navigator.clipboard.writeText(text);
      logsMsg = "Copied the latest log entries to the clipboard.";
    } catch (_) {
      logsMsg = "Clipboard not available here — use Export log file instead.";
    }
  }

  function exportLogs(): void {
    void run(async () => {
      logsMsg = null;
      const path = await saveDialog({
        defaultPath: "makepresent-logs.txt",
        filters: [{ name: "Text", extensions: ["txt", "log"] }],
      });
      if (!path) return;
      await api.exportLogs(path);
      logsMsg = `Exported the log file to ${path}.`;
    });
  }

  $effect(() => {
    if (tab === "logs") void loadLogs();
  });
</script>

<div class="overlay">
  <button class="backdrop" aria-label="Close settings" tabindex="-1" onclick={onclose}></button>
  <div class="dialog" role="dialog" tabindex="-1" aria-label="Settings" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
    <header class="dialog-head">
      <h2>Settings</h2>
      <button class="close" title="Close" onclick={onclose}>&times;</button>
    </header>

    <nav class="tabs">
      <button class="tab" class:active={tab === "general"} onclick={() => (tab = "general")}>
        General
      </button>
      <button class="tab" class:active={tab === "looks"} onclick={() => (tab = "looks")}>
        Looks
      </button>
      <button class="tab" class:active={tab === "logs"} onclick={() => (tab = "logs")}>
        Logs
      </button>
    </nav>

    <div class="content">
      {#if tab === "general"}
        <div class="panel-general">
          <p class="hint">
            These are per-machine settings: display assignments, fullscreen,
            stage visibility, and the default transition. The project and
            library are <em>not</em> part of this export.
          </p>

          <ul class="summary">
            {#each summary as row (row.label)}
              <li>
                <span class="sum-label">{row.label}</span>
                <span class="sum-value">{row.value}</span>
              </li>
            {/each}
          </ul>

          <div class="actions">
            <button onclick={() => exportSettings()}>Export settings…</button>
            <button onclick={() => importSettings()}>Import settings…</button>
          </div>

          {#if status}
            <p class="status" class:err={status.kind === "err"}>{status.text}</p>
          {/if}
        </div>
      {:else if tab === "looks"}
        <div class="panel-looks">
          <p class="hint">
            Looks are named style profiles — font size, text colour, text
            position and whether the background is shown. Each output window
            (Main, Stage, future Stream) renders the same live slide but applies
            its assigned Look. They are stored with the project and update
            instantly while live.
          </p>

          {#if looks.length === 0}
            <p class="looks-empty">No Looks yet. Add one below.</p>
          {:else}
            <div class="looks-layout">
              <div class="looks-list">
                {#each looks as lk (lk.id)}
                  <button
                    class="look-pill"
                    class:active={lk.id === activeLook?.id}
                    onclick={() => selectLook(lk.id)}
                  >
                    <span
                      class="look-swatch"
                      style:background-color={lk.textColor}
                    ></span>
                    {lk.name}
                    {#if appState?.outputLookId === lk.id}
                      <span class="badge">Output</span>
                    {/if}
                    {#if appState?.stageLookId === lk.id}
                      <span class="badge stage">Stage</span>
                    {/if}
                  </button>
                {/each}
                <button class="add-look" onclick={() => addLook()}>+ Add look</button>
              </div>

              <div class="look-editor">
                {#if draft}
                  <label>
                    Name
                    <input
                      type="text"
                      value={draft.name}
                      oninput={(e) => setDraft("name", (e.target as HTMLInputElement).value)}
                    />
                  </label>
                  <div class="field-row">
                    <label>
                      Title size
                      <input
                        type="number"
                        min="16"
                        max="300"
                        value={draft.titleSize}
                        oninput={(e) =>
                          setDraft("titleSize", Number((e.target as HTMLInputElement).value))}
                      />
                    </label>
                    <label>
                      Body size
                      <input
                        type="number"
                        min="16"
                        max="300"
                        value={draft.bodySize}
                        oninput={(e) =>
                          setDraft("bodySize", Number((e.target as HTMLInputElement).value))}
                      />
                    </label>
                  </div>
                  <label>
                    Text colour
                    <span class="color-line">
                      <input
                        type="color"
                        value={draft.textColor}
                        oninput={(e) =>
                          setDraft("textColor", (e.target as HTMLInputElement).value)}
                      />
                      <code>{draft.textColor}</code>
                    </span>
                  </label>
                  <label class="check">
                    <input
                      type="checkbox"
                      checked={draft.showBackground}
                      onchange={(e) =>
                        setDraft("showBackground", (e.target as HTMLInputElement).checked)}
                    />
                    Show background
                  </label>
                  <label>
                    Text position
                    <select
                      value={draft.textPosition}
                      onchange={(e) =>
                        setDraft("textPosition", (e.target as HTMLSelectElement).value as TextPosition)}
                    >
                      <option value="top">Top</option>
                      <option value="center">Center</option>
                      <option value="bottom">Bottom</option>
                    </select>
                  </label>

                  <div class="assign-block">
                    <span class="assign-title">Assign to</span>
                    <label>
                      Main Output
                      <select
                        value={appState?.outputLookId ?? ""}
                        onchange={(e) =>
                          assignTo("output", (e.target as HTMLSelectElement).value || null)}
                      >
                        <option value="">Auto (Main)</option>
                        {#each looks as lk (lk.id)}
                          <option value={lk.id}>{lk.name}</option>
                        {/each}
                      </select>
                    </label>
                    <label>
                      Stage Display
                      <select
                        value={appState?.stageLookId ?? ""}
                        onchange={(e) =>
                          assignTo("stage", (e.target as HTMLSelectElement).value || null)}
                      >
                        <option value="">Auto (Stage)</option>
                        {#each looks as lk (lk.id)}
                          <option value={lk.id}>{lk.name}</option>
                        {/each}
                      </select>
                    </label>
                  </div>

                  <button class="danger" onclick={() => deleteLook()}>Delete this look</button>
                {:else}
                  <p class="looks-empty">Select or create a Look to edit its style.</p>
                {/if}
              </div>
            </div>
          {/if}

          {#if lookErr}
            <p class="status err">{lookErr}</p>
          {/if}
        </div>
      {:else}
        <div class="panel-logs">
          <div class="log-actions">
            <button onclick={() => loadLogs()}>Refresh</button>
            <button onclick={() => copyLogs()}>Copy to clipboard</button>
            <button onclick={() => exportLogs()}>Export log file…</button>
          </div>
          {#if logsMsg}
            <p class="status">{logsMsg}</p>
          {/if}
          <pre class="log-view" tabindex="-1">
{#each logs as entry (entry.at + entry.level + entry.message)}
  <span>{entry.at} {entry.level} {entry.message}</span>
{/each}
          </pre>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    border: none;
    border-radius: 0;
    cursor: pointer;
  }

  .dialog {
    position: relative;
    margin: 0 auto;
    top: 50%;
    transform: translateY(-50%);
    width: min(680px, 92vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 18px 60px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }

  .dialog-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
  }

  .dialog-head h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: clamp(13px, 1.1vw, 16px);
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }

  .close {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 6px;
    width: 28px;
    height: 28px;
    line-height: 1;
  }

  .tabs {
    display: flex;
    gap: 4px;
    padding: 10px 14px 0;
    border-bottom: 1px solid var(--border);
  }

  .tab {
    background: transparent;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 6px 6px 0 0;
    padding: 8px 14px;
    color: var(--text-dim);
  }

  .tab.active {
    background: var(--panel-2);
    border-color: var(--border);
    color: var(--text);
  }

  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 18px;
  }

  .hint {
    font-size: 12.5px;
    color: var(--text-dim);
    line-height: 1.5;
    margin: 0 0 14px;
  }

  .summary {
    list-style: none;
    margin: 0 0 16px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }

  .summary li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 9px 12px;
    font-size: 13px;
    border-bottom: 1px solid var(--border);
  }

  .summary li:last-child {
    border-bottom: none;
  }

  .sum-label {
    color: var(--text-dim);
  }

  .sum-value {
    color: var(--text);
    text-align: right;
  }

  .actions,
  .log-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .status {
    font-size: 13px;
    margin: 12px 0 0;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--live-bg);
    color: #c9f4d8;
  }

  .status.err {
    background: var(--danger-bg);
    color: var(--danger-text);
  }

  .log-view {
    margin: 12px 0 0;
    padding: 10px;
    background: #101218;
    border: 1px solid var(--border);
    border-radius: 8px;
    max-height: 46vh;
    overflow-y: auto;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.55;
    color: var(--text);
    white-space: pre-wrap;
    word-break: break-word;
    display: flex;
    flex-direction: column;
  }

  .panel-looks {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .looks-empty {
    color: var(--text-dim);
    font-size: 13px;
    margin: 0;
  }

  .looks-layout {
    display: grid;
    grid-template-columns: 200px 1fr;
    gap: 16px;
  }

  .looks-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .look-pill {
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 10px;
    font-size: 13px;
    color: var(--text);
  }

  .look-pill.active {
    border-color: var(--accent);
    outline: 1px solid var(--accent);
  }

  .look-swatch {
    width: 14px;
    height: 14px;
    border-radius: 3px;
    border: 1px solid var(--border);
    flex: none;
  }

  .badge {
    margin-left: auto;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1px 6px;
  }

  .badge.stage {
    color: var(--accent);
  }

  .add-look {
    background: transparent;
  }

  .look-editor {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel-2);
  }

  .field-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .color-line {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .color-line input[type="color"] {
    width: 34px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--panel-2);
  }

  .color-line code {
    font-size: 12px;
    color: var(--text-dim);
  }

  .check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }

  .assign-block {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }

  .assign-title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-dim);
  }

  .danger {
    border-color: var(--danger);
    color: var(--danger);
    background: transparent;
  }

  .panel-looks label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .panel-looks label.check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    color: var(--text);
  }

  .panel-looks input[type="text"],
  .panel-looks input[type="number"],
  .panel-looks select {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 10px;
    color: var(--text);
    width: 100%;
  }

  .panel-looks button {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    color: var(--text);
    padding: 6px 12px;
  }
</style>
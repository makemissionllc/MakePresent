<script lang="ts">
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/sync";
  import type { ClientState, LogEntry } from "../lib/types";

  interface Props {
    app: ClientState | null;
    onclose: () => void;
  }

  let { app: appState, onclose }: Props = $props();

  let tab = $state<"general" | "logs">("general");
  let status = $state<{ kind: "ok" | "err"; text: string } | null>(null);
  let logs = $state<LogEntry[]>([]);
  let logsMsg = $state<string | null>(null);

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
    font-size: 15px;
    font-weight: 600;
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
    background: #12351f;
    color: #c9f4d8;
  }

  .status.err {
    background: #4a1f1f;
    color: #ffd9d9;
  }

  .log-view {
    margin: 12px 0 0;
    padding: 10px;
    background: #101218;
    border: 1px solid var(--border);
    border-radius: 8px;
    max-height: 46vh;
    overflow-y: auto;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 12px;
    line-height: 1.55;
    color: var(--text);
    white-space: pre-wrap;
    word-break: break-word;
    display: flex;
    flex-direction: column;
  }
</style>
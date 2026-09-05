<script lang="ts">
  import { onDestroy } from "svelte";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import {
    api,
    subscribeMidiMessage,
    subscribeNdiMonitorStatus,
    subscribeNdiPreview,
    subscribeNdiSources,
  } from "../lib/sync";
  import type {
    AudioDeviceInfo,
    AudioStateView,
    BoxGeometry,
    ClientState,
    LogEntry,
    Look,
    LookPatch,
    MidiDeviceInfo,
    MidiMessageView,
    NdiMonitorStatus,
    NdiPreviewFrame,
    NdiSourceInfo,
    Positioning,
    StageNetworkInfo,
    TextPosition,
    Trigger,
    TriggerAction,
  } from "../lib/types";

  interface Props {
    app: ClientState | null;
    onclose: () => void;
  }

  let { app: appState, onclose }: Props = $props();

  let tab = $state<"general" | "looks" | "triggers" | "network" | "audio" | "logs">("general");
  let status = $state<{ kind: "ok" | "err"; text: string } | null>(null);
  let logs = $state<LogEntry[]>([]);
  let logsMsg = $state<string | null>(null);
  let lookErr = $state<string | null>(null);

  // Trigger (MIDI/OSC) panel state.
  let midiDevices = $state<MidiDeviceInfo[]>([]);
  let midiDevicesMsg = $state<string | null>(null);
  let midiMsgs = $state<MidiMessageView[]>([]);
  let midiMsgsMsg = $state<string | null>(null);
  let oscPortDraft = $state<number>(9000);
  let oscAddressDraft = $state<string>("/makepresent/goto");
  let draftTrigger: Trigger | null = $state(null);
  let draftActionKind = $state<"next_slide" | "prev_slide" | "jump_to" | "clear_output">(
    "next_slide",
  );
  let draftJumpIndex = $state(2);
  let draftErr = $state<string | null>(null);

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
      titleFont: updated.titleFont,
      bodyFont: updated.bodyFont,
      textColor: updated.textColor,
      showBackground: updated.showBackground,
      textPosition: updated.textPosition,
      positioning: updated.positioning,
      titleBox: updated.titleBox,
      bodyBox: updated.bodyBox,
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
        titleFont: "sans-serif",
        bodyFont: "sans-serif",
        textColor: "#ffffff",
        showBackground: true,
        textPosition: "center",
        positioning: "auto",
        titleBox: { x: 5, y: 10, width: 90, height: 20, zIndex: 1 },
        bodyBox: { x: 5, y: 35, width: 90, height: 45, zIndex: 1 },
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

  function assignTo(target: "output" | "stage" | "ndi", id: string | null): void {
    lookErr = null;
    const fn =
      target === "output"
        ? api.setOutputLook
        : target === "stage"
          ? api.setStageLook
          : api.setNdiLook;
    void fn(id).then((s) => (appState = s)).catch((e: unknown) => (lookErr = String(e)));
  }

  // ---------------------------------------------------------------------------
  // Bounding-box template editor (FreeShow-style)
  // ---------------------------------------------------------------------------

  // Which role's box the pointer is currently moving/resizing ("title"|"body").
  let boxDrag: { role: "title" | "body"; mode: "move" | "resize" } | null = $state(null);
  let canvasRef = $state<HTMLDivElement | null>(null);

  function boxOf(role: "title" | "body"): BoxGeometry {
    if (!draft) return { x: 5, y: 10, width: 90, height: 20, zIndex: 1 };
    return role === "title" ? draft.titleBox : draft.bodyBox;
  }

  function updateBox(role: "title" | "body", next: BoxGeometry): void {
    if (!draft) return;
    setDraft(role === "title" ? "titleBox" : "bodyBox", next);
  }

  function setPositioning(p: Positioning): void {
    if (!draft) return;
    setDraft("positioning", p);
  }

  function onBoxPointerDown(e: PointerEvent, role: "title" | "body", mode: "move" | "resize") {
    e.preventDefault();
    e.stopPropagation();
    boxDrag = { role, mode };
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
  }

  function onCanvasPointerMove(e: PointerEvent): void {
    if (!boxDrag || !draft) return;
    const el = canvasRef;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const w = Math.max(1, rect.width);
    const h = Math.max(1, rect.height);
    const px = (e.clientX - rect.left) / w;
    const py = (e.clientY - rect.top) / h;

    let cur = boxOf(boxDrag.role);
    let next: BoxGeometry;
    if (boxDrag.mode === "move") {
      next = { ...cur, x: clamp(px * 100 - cur.width / 2), y: clamp(py * 100 - cur.height / 2) };
    } else {
      // Resize from the bottom-right corner: keep the top-left anchored.
      next = {
        ...cur,
        width: clamp(px * 100 - cur.x, 5, 100 - cur.x),
        height: clamp(py * 100 - cur.y, 5, 100 - cur.y),
      };
    }
    updateBox(boxDrag.role, next);
  }

  function endBoxDrag(e: PointerEvent): void {
    if (!boxDrag) return;
    const el = e.currentTarget as HTMLElement;
    if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
    boxDrag = null;
  }

  function clamp(v: number, lo = 0, hi = 100): number {
    return Math.min(hi, Math.max(lo, v));
  }

  function n0(v: number): string {
    return Math.round(v).toString();
  }

  function setNdiEnabled(enabled: boolean): void {
    lookErr = null;
    void api
      .setNdiEnabled(enabled)
      .then((s) => (appState = s))
      .catch((e: unknown) => (lookErr = String(e)));
  }

  // NDI receive confidence monitor — low-rate (~2 fps) preview of a network
  // camera, fully independent from the NDI broadcast toggle above. Frames
  // arrive over a dedicated event (never the state broadcast); the badge
  // shows LIVE only while frames are fresh, STALE otherwise.
  let ndiMonOn = $state(false);
  let ndiMonSources = $state<NdiSourceInfo[]>([]);
  let ndiMonStatus = $state<NdiMonitorStatus>({
    state: "off",
    source: null,
    message: "Monitor off.",
  });
  let ndiMonFrame = $state<NdiPreviewFrame | null>(null);
  let ndiMonErr = $state<string | null>(null);
  let ndiMonUnlisten: Array<() => void> = [];
  let ndiMonPoll: ReturnType<typeof setInterval> | null = null;
  const ndiMonBadge = $derived(
    ndiMonStatus.state === "live"
      ? "LIVE"
      : ndiMonStatus.state === "stale"
        ? "STALE"
        : ndiMonStatus.state.toUpperCase(),
  );

  async function setNdiMonitorOn(on: boolean): Promise<void> {
    ndiMonErr = null;
    if (!on) {
      stopNdiMonitor();
      return;
    }
    try {
      ndiMonSources = await api.startNdiScan();
      ndiMonStatus = await api.ndiMonitorStatus();
      ndiMonUnlisten.push(await subscribeNdiSources((s) => (ndiMonSources = s)));
      ndiMonUnlisten.push(
        await subscribeNdiMonitorStatus((s) => (ndiMonStatus = s)),
      );
      ndiMonUnlisten.push(await subscribeNdiPreview((f) => (ndiMonFrame = f)));
      // Event-driven updates are primary; a slow poll is the backstop so a
      // missed event can never leave the picker permanently empty.
      if (ndiMonPoll) clearInterval(ndiMonPoll);
      ndiMonPoll = setInterval(() => {
        void api
          .listNdiSources()
          .then((s) => (ndiMonSources = s))
          .catch(() => {});
      }, 2500);
      ndiMonOn = true;
    } catch (e: unknown) {
      stopNdiMonitor();
      ndiMonErr = String(e);
    }
  }

  function connectNdiSource(name: string): void {
    ndiMonErr = null;
    // Drop the previous source's frame immediately — it must never read as
    // the newly selected source's picture while connecting.
    ndiMonFrame = null;
    void api
      .connectNdiSource(name)
      .then((s) => (ndiMonStatus = s))
      .catch((e: unknown) => (ndiMonErr = String(e)));
  }

  function stopNdiMonitor(): void {
    for (const u of ndiMonUnlisten) {
      try {
        u();
      } catch {
        /* already gone */
      }
    }
    ndiMonUnlisten = [];
    if (ndiMonPoll) {
      clearInterval(ndiMonPoll);
      ndiMonPoll = null;
    }
    // Fully independent teardown: receiver, then the finder thread. The NDI
    // *broadcast* above is untouched by either call.
    void api.disconnectNdiSource().catch(() => {});
    void api.stopNdiScan().catch(() => {});
    ndiMonOn = false;
    ndiMonFrame = null;
    ndiMonStatus = { state: "off", source: null, message: "Monitor off." };
  }

  // The monitor exists for this panel: closing Settings releases the NDI
  // receiver instead of holding a camera connection with no viewer.
  onDestroy(stopNdiMonitor);

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
          {
            label: "NDI broadcast",
            value: appState.broadcast.enabled ? `On (${appState.broadcast.sourceName})` : "Off",
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

  // --- Triggers (MIDI + OSC) panel logic ---

  let unSubscribeMidi: (() => void) | null = null;

  $effect(() => {
    if (tab !== "triggers") return;
    void refreshMidiDevices();
    oscPortDraft = appState?.oscPort ?? 9000;
  });

  $effect(() => {
    void subscribeMidiMessage((msg) => {
      midiMsgs = [...midiMsgs.slice(-19), msg];
    }).then((un) => {
      unSubscribeMidi = un;
    });
    return () => {
      unSubscribeMidi?.();
      unSubscribeMidi = null;
    };
  });

  async function refreshMidiDevices(): Promise<void> {
    midiDevicesMsg = null;
    try {
      midiDevices = await api.listMidiDevices();
    } catch (e) {
      midiDevicesMsg = String(e);
    }
  }

  function setMidiEnabled(enabled: boolean): void {
    void api
      .setMidiEnabled(enabled)
      .then((s) => (appState = s))
      .catch((e: unknown) => (draftErr = String(e)));
  }

  function setMidiDevice(e: Event): void {
    const id = (e.target as HTMLSelectElement).value;
    if (!id) return;
    void api
      .setMidiDevice(id)
      .then((s) => (appState = s))
      .catch((err: unknown) => (draftErr = String(err)));
  }

  function setOscEnabled(enabled: boolean): void {
    void api
      .setOscEnabled(enabled)
      .then((s) => (appState = s))
      .catch((e: unknown) => (draftErr = String(e)));
  }

  function setOscPortValue(): void {
    const port = Math.round(oscPortDraft) || 0;
    if (port <= 0 || port > 65535) {
      draftErr = "OSC port must be between 1 and 65535.";
      return;
    }
    void api
      .setOscPort(port)
      .then((s) => (appState = s))
      .catch((e: unknown) => (draftErr = String(e)));
  }

  function triggerFromMessage(msg: MidiMessageView): Trigger | null {
    if (msg.data1 == null) return null;
    if (msg.kind === "note_on" || msg.kind === "note_off") {
      return { kind: "midi_note", channel: msg.channel, note: msg.data1 };
    }
    if (msg.kind === "cc") {
      return {
        kind: "midi_control",
        channel: msg.channel,
        controller: msg.data1,
        value: msg.data2 ?? null,
      };
    }
    if (msg.kind === "program") {
      return { kind: "midi_program", channel: msg.channel, program: msg.data1 };
    }
    return null;
  }

  function captureMessageDraft(msg: MidiMessageView): void {
    const trigger = triggerFromMessage(msg);
    if (!trigger) {
      draftErr = "This message can't be used as a trigger.";
      return;
    }
    draftErr = null;
    draftTrigger = trigger;
  }

  function captureOscDraft(address: string): void {
    const trimmed = address.trim();
    if (!trimmed.startsWith("/") || trimmed.length < 2) {
      draftErr = "Enter an OSC address that starts with '/'.";
      return;
    }
    draftErr = null;
    draftTrigger = { kind: "osc_address", address: trimmed };
  }

  function buildAction(): TriggerAction {
    if (draftActionKind === "jump_to") {
      const index = Math.max(1, Math.round(draftJumpIndex) || 1);
      return { kind: "jump_to", index };
    }
    return { kind: draftActionKind };
  }

  function addDraftMapping(): void {
    if (!draftTrigger) {
      draftErr = "Capture a MIDI message or enter an OSC address first.";
      return;
    }
    const action = buildAction();
    const label = describeTrigger(draftTrigger);
    void api
      .addTrigger(draftTrigger, action, label)
      .then((s) => {
        appState = s;
        draftTrigger = null;
        draftErr = null;
      })
      .catch((e: unknown) => (draftErr = String(e)));
  }

  function describeTrigger(t: Trigger): string {
    if (t.kind === "midi_note") return `Note ${t.note} (ch ${t.channel})`;
    if (t.kind === "midi_control")
      return `CC ${t.controller}${t.value != null ? ` = ${t.value}` : ""} (ch ${t.channel})`;
    if (t.kind === "midi_program") return `Program ${t.program} (ch ${t.channel})`;
    return t.address;
  }

  function describeDraft(): string {
    return draftTrigger ? describeTrigger(draftTrigger) : "no trigger selected";
  }

  function deleteMapping(id: string): void {
    void api
      .deleteTrigger(id)
      .then((s) => (appState = s))
      .catch((e: unknown) => (draftErr = String(e)));
  }

  function toggleMapping(id: string, enabled: boolean): void {
    void api
      .setTriggerEnabled(id, enabled)
      .then((s) => (appState = s))
      .catch((e: unknown) => (draftErr = String(e)));
  }

  // --- Network (stage over LAN) panel state ---

  let networkInfo = $state<StageNetworkInfo | null>(null);
  let networkErr = $state<string | null>(null);
  let networkMsg = $state<string | null>(null);
  let networkPortDraft = $state<number>(1426);
  let networkPinDraft = $state<string>("");

  // --- Audio (backing track) ---
  let audioDevices = $state<AudioDeviceInfo[]>([]);
  let audioDevicesMsg = $state<string | null>(null);
  let audioVolumeDraft = $state<number>(1.0);
  let audioMsg = $state<string | null>(null);
  let audioErr = $state<string | null>(null);

  $effect(() => {
    if (tab !== "network") return;
    void refreshNetworkInfo();
  });

  async function refreshNetworkInfo(): Promise<void> {
    networkErr = null;
    try {
      networkInfo = await api.getStageNetworkInfo();
      networkPortDraft = networkInfo.port;
      networkPinDraft = networkInfo.pin;
    } catch (e) {
      networkErr = String(e);
    }
  }

  function setNetworkEnabled(enabled: boolean): void {
    networkErr = null;
    networkMsg = null;
    void api
      .setStageNetworkEnabled(enabled)
      .then((s) => {
        appState = s;
        void refreshNetworkInfo();
      })
      .catch((e: unknown) => (networkErr = String(e)));
  }

  function setNetworkPort(): void {
    const port = Math.round(networkPortDraft) || 0;
    if (port <= 0 || port > 65535) {
      networkErr = "Port must be between 1 and 65535.";
      return;
    }
    networkErr = null;
    networkMsg = null;
    void api
      .setStageNetworkPort(port)
      .then((s) => {
        appState = s;
        networkMsg = "Port updated. The server restarts to apply it.";
        void refreshNetworkInfo();
      })
      .catch((e: unknown) => (networkErr = String(e)));
  }

  function setNetworkPin(): void {
    networkErr = null;
    networkMsg = null;
    void api
      .setStageNetworkPin(networkPinDraft)
      .then((s) => {
        appState = s;
        networkMsg = "PIN updated and applied to the live server.";
      })
      .catch((e: unknown) => (networkErr = String(e)));
  }

  async function copyStageUrl(): Promise<void> {
    const url = networkInfo?.urls[0];
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      networkMsg = "Copied the Stage URL to the clipboard.";
    } catch (_) {
      networkMsg = "Clipboard not available here — copy the URL manually.";
    }
  }

  // --- Audio (single backing track, not tied to slides) ---
  async function refreshAudioDevices(): Promise<void> {
    audioDevicesMsg = null;
    try {
      audioDevices = await api.listAudioDevices();
    } catch (e) {
      audioDevicesMsg = String(e);
    }
  }

  function loadAudioFile(): void {
    void (async () => {
      audioErr = null;
      audioMsg = null;
      try {
        const file = await openDialog({
          multiple: false,
          directory: false,
          filters: [{ name: "Audio", extensions: ["mp3", "wav", "flac", "ogg", "m4a", "aac", "wma"] }],
        });
        if (!file) return;
        const path = Array.isArray(file) ? file[0] : (file as string);
        const st = await api.loadAudio(path);
        appState = st;
        audioMsg = `Loaded "${path.split(/[\/\\]/).pop()}"`;
      } catch (e) {
        audioErr = String(e);
      }
    })();
  }

  function playAudio(): void {
    audioErr = null;
    void api.playAudio().then((s) => (appState = s)).catch((e: unknown) => (audioErr = String(e)));
  }
  function pauseAudio(): void {
    audioErr = null;
    void api.pauseAudio().then((s) => (appState = s)).catch((e: unknown) => (audioErr = String(e)));
  }
  function stopAudio(): void {
    audioErr = null;
    void api.stopAudio().then((s) => (appState = s)).catch((e: unknown) => (audioErr = String(e)));
  }
  function setAudioVolume(v: number): void {
    const vol = Math.max(0, Math.min(1.5, v));
    audioVolumeDraft = vol;
    void api
      .setAudioVolume(vol)
      .then((s) => (appState = s))
      .catch((e: unknown) => (audioErr = String(e)));
  }
  function setAudioDevice(e: Event): void {
    const id = (e.target as HTMLSelectElement).value;
    const deviceId = id === "" ? null : id;
    audioErr = null;
    void api
      .setAudioDevice(deviceId)
      .then((s) => (appState = s))
      .catch((e: unknown) => (audioErr = String(e)));
  }

  $effect(() => {
    if (tab !== "audio") return;
    void refreshAudioDevices();
    // Sync draft volume with current state
    audioVolumeDraft = appState?.audio?.volume ?? 1.0;
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
      <button class="tab" class:active={tab === "triggers"} onclick={() => (tab = "triggers")}>
        Triggers
      </button>
      <button class="tab" class:active={tab === "network"} onclick={() => (tab = "network")}>
        Network
      </button>
      <button class="tab" class:active={tab === "audio"} onclick={() => (tab = "audio")}>
        Audio
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
            stage visibility, and the default transition. The current view and
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

          <div class="bcast-block">
            <div class="bcast-title">
              <span class="assign-title">NDI broadcast</span>
              <label class="check">
                <input
                  type="checkbox"
                  checked={appState?.broadcast.enabled ?? false}
                  onchange={(e) => setNdiEnabled((e.target as HTMLInputElement).checked)}
                />
                Enabled
              </label>
            </div>
            <p class="hint">
              Publishes the live slide as an NDI source
              (<code>{appState?.broadcast.sourceName}</code>) on your local
              network so a video switcher can cut to it. Requires the free
              <a href="https://ndi.video" target="_blank" rel="noopener noreferrer">
                NDI® SDK
              </a>
              installed on this machine; it is loaded at runtime and the app
              keeps working normally if it is absent. Assign a Look to the NDI
              feed under <em>Looks</em>.
            </p>
          </div>

          <div class="bcast-block">
            <div class="bcast-title">
              <span class="assign-title">
                NDI camera monitor <span class="muted-note">(preview ~2 fps)</span>
              </span>
              <label class="check">
                <input
                  type="checkbox"
                  checked={ndiMonOn}
                  onchange={(e) =>
                    void setNdiMonitorOn((e.target as HTMLInputElement).checked)}
                />
                Enabled
              </label>
            </div>
            <p class="hint">
              Watches a network camera at low rate — an "is it alive and
              pointed at the right thing" check, not live video. Fully
              separate from NDI broadcast above; needs the same free NDI® SDK.
            </p>
            {#if ndiMonOn}
              {#if ndiMonErr}
                <p class="status err">{ndiMonErr}</p>
              {/if}
              <div class="ndimon-row">
                <span class="ndimon-badge" data-s={ndiMonStatus.state}>
                  {ndiMonBadge}
                </span>
                <span class="ndimon-msg">{ndiMonStatus.message}</span>
              </div>
              {#if ndiMonSources.length === 0}
                <p class="hint">
                  Scanning for NDI sources… discovery takes a few seconds.
                </p>
              {:else}
                <div class="ndimon-sources" role="listbox" aria-label="NDI sources">
                  {#each ndiMonSources as s (s.name)}
                    <button
                      class:on={ndiMonStatus.source === s.name}
                      onclick={() => connectNdiSource(s.name)}
                      title={s.url || s.name}
                    >
                      {s.name}
                    </button>
                  {/each}
                </div>
              {/if}
              {#if ndiMonFrame}
                <div
                  class="ndimon-preview"
                  data-s={ndiMonStatus.state === "live" ? "live" : "stale"}
                >
                  <img
                    src="data:image/jpeg;base64,{ndiMonFrame.jpegBase64}"
                    alt={ndiMonStatus.source
                      ? `NDI preview of ${ndiMonStatus.source}`
                      : "NDI preview"}
                  />
                  {#if ndiMonStatus.state !== "live"}
                    <div class="ndimon-staleveil">
                      STALE — last frame, not live
                    </div>
                  {/if}
                </div>
              {/if}
            {/if}
          </div>

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
            (Main, Stage, NDI feed) renders the same live slide but applies
            its assigned Look. They are stored with the current view and update
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
                    {#if appState?.ndiLookId === lk.id}
                      <span class="badge ndi">NDI</span>
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

                  <div class="field-row">
                    <label>
                      Title font
                      <input
                        type="text"
                        placeholder="Druk Wide, Helvetica Neue Bold…"
                        value={draft.titleFont}
                        oninput={(e) =>
                          setDraft("titleFont", (e.target as HTMLInputElement).value)}
                      />
                    </label>
                    <label>
                      Body font
                      <input
                        type="text"
                        placeholder="Helvetica Neue Bold…"
                        value={draft.bodyFont}
                        oninput={(e) =>
                          setDraft("bodyFont", (e.target as HTMLInputElement).value)}
                      />
                    </label>
                  </div>

                  <div class="positioning-row">
                    <span class="assign-title">Layout</span>
                    <label class="check">
                      <input
                        type="radio"
                        name="positioning"
                        checked={draft.positioning === "auto"}
                        onchange={() => setPositioning("auto")}
                      />
                      Auto flow
                    </label>
                    <label class="check">
                      <input
                        type="radio"
                        name="positioning"
                        checked={draft.positioning === "absolute"}
                        onchange={() => setPositioning("absolute")}
                      />
                      Bounding boxes
                    </label>
                  </div>

                  {#if draft.positioning === "absolute"}
                    <div class="box-editor">
                      <div
                        class="box-canvas"
                        role="presentation"
                        bind:this={canvasRef}
                        onpointermove={onCanvasPointerMove}
                        onpointerup={endBoxDrag}
                        onpointercancel={endBoxDrag}
                      >
                        <div
                          class="box title"
                          role="button"
                          tabindex="0"
                          style:left={`${draft.titleBox.x}%`}
                          style:top={`${draft.titleBox.y}%`}
                          style:width={`${draft.titleBox.width}%`}
                          style:height={`${draft.titleBox.height}%`}
                          style:z-index={draft.titleBox.zIndex}
                          onpointerdown={(e) => onBoxPointerDown(e, "title", "move")}
                        >
                          <span class="box-label">Title</span>
                          <span
                            class="handle"
                            role="button"
                            tabindex="0"
                            aria-label="Resize title box"
                            onpointerdown={(e) => onBoxPointerDown(e, "title", "resize")}
                          ></span>
                        </div>
                        <div
                          class="box body"
                          role="button"
                          tabindex="0"
                          style:left={`${draft.bodyBox.x}%`}
                          style:top={`${draft.bodyBox.y}%`}
                          style:width={`${draft.bodyBox.width}%`}
                          style:height={`${draft.bodyBox.height}%`}
                          style:z-index={draft.bodyBox.zIndex}
                          onpointerdown={(e) => onBoxPointerDown(e, "body", "move")}
                        >
                          <span class="box-label">Body</span>
                          <span
                            class="handle"
                            role="button"
                            tabindex="0"
                            aria-label="Resize body box"
                            onpointerdown={(e) => onBoxPointerDown(e, "body", "resize")}
                          ></span>
                        </div>
                      </div>
                      <div class="box-fields">
                        <div class="box-field">
                          <span class="assign-title">Title</span>
                          <code>X {n0(draft.titleBox.x)} · Y {n0(draft.titleBox.y)} · W {n0(draft.titleBox.width)} · H {n0(draft.titleBox.height)}</code>
                        </div>
                        <div class="box-field">
                          <span class="assign-title">Body</span>
                          <code>X {n0(draft.bodyBox.x)} · Y {n0(draft.bodyBox.y)} · W {n0(draft.bodyBox.width)} · H {n0(draft.bodyBox.height)}</code>
                        </div>
                      </div>
                    </div>
                  {/if}

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
                    <label>
                      NDI Feed
                      <select
                        value={appState?.ndiLookId ?? ""}
                        onchange={(e) =>
                          assignTo("ndi", (e.target as HTMLSelectElement).value || null)}
                      >
                        <option value="">Auto (first Look)</option>
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
      {:else if tab === "logs"}
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
      {:else if tab === "triggers"}
        <div class="panel-triggers">
          {#if draftErr}
            <p class="status err">{draftErr}</p>
          {/if}

          <section class="trigger-section">
            <div class="section-head">
              <h3>MIDI input</h3>
              <label class="switch">
                <input
                  type="checkbox"
                  checked={appState?.midiEnabled ?? false}
                  onchange={(e) => setMidiEnabled(e.currentTarget.checked)}
                />
                <span>{appState?.midiEnabled ? "On" : "Off"}</span>
              </label>
            </div>
            {#if appState?.midiEnabled}
              <div class="row">
                <select
                  class="device-select"
                  value={appState?.midiDeviceId ?? ""}
                  onchange={setMidiDevice}
                >
                  <option value="" disabled>— choose a device —</option>
                  {#each midiDevices as dev (dev.id)}
                    <option value={dev.id}>{dev.name}</option>
                  {/each}
                </select>
                <button onclick={() => void refreshMidiDevices()}>Refresh</button>
              </div>
              {#if midiDevicesMsg}
                <p class="status err">{midiDevicesMsg}</p>
              {/if}
              {#if !midiDevices.length}
                <p class="hint">No MIDI input devices detected.</p>
              {/if}

              <div class="monitor">
                <h4>Live monitor</h4>
                <p class="hint">
                  Press a button / key on your controller to see its message, then
                  “Use as trigger”.
                </p>
                <ul class="midi-list">
                  {#each midiMsgs as msg (msg.data + msg.channel)}
                    <li>
                      <span class="midi-desc">{msg.describe}</span>
                      <button onclick={() => captureMessageDraft(msg)}>Use as trigger</button>
                    </li>
                  {/each}
                  {#if !midiMsgs.length}
                    <li class="empty">No messages yet…</li>
                  {/if}
                </ul>
              </div>
            {/if}
          </section>

          <section class="trigger-section">
            <div class="section-head">
              <h3>OSC</h3>
              <label class="switch">
                <input
                  type="checkbox"
                  checked={appState?.oscEnabled ?? false}
                  onchange={(e) => setOscEnabled(e.currentTarget.checked)}
                />
                <span>{appState?.oscEnabled ? "On" : "Off"}</span>
              </label>
            </div>
            {#if appState?.oscEnabled}
              <div class="row">
                <label>
                  Port
                  <input
                    type="number"
                    min="1"
                    max="65535"
                    bind:value={oscPortDraft}
                    onchange={setOscPortValue}
                  />
                </label>
                <button onclick={setOscPortValue}>Apply port</button>
              </div>
              <div class="row">
                <input
                  type="text"
                  bind:value={oscAddressDraft}
                  placeholder="/makepresent/next"
                  class="osc-address"
                />
                <button onclick={() => captureOscDraft(oscAddressDraft)}>
                  Use as trigger
                </button>
              </div>
              <p class="hint">
                Bare address maps to next; append a number for jump, e.g.
                “/makepresent/goto/5”.
              </p>
            {/if}
          </section>

          <section class="trigger-section">
            <h3>New mapping</h3>
            <p class="hint">Now choose what this trigger should do.</p>
            <div class="action-picker">
              <label>
                <input
                  type="radio"
                  name="draftAction"
                  value="next_slide"
                  checked={draftActionKind === "next_slide"}
                  onchange={() => (draftActionKind = "next_slide")}
                />
                Next slide
              </label>
              <label>
                <input
                  type="radio"
                  name="draftAction"
                  value="prev_slide"
                  checked={draftActionKind === "prev_slide"}
                  onchange={() => (draftActionKind = "prev_slide")}
                />
                Previous slide
              </label>
              <label>
                <input
                  type="radio"
                  name="draftAction"
                  value="jump_to"
                  checked={draftActionKind === "jump_to"}
                  onchange={() => (draftActionKind = "jump_to")}
                />
                Jump to
                {#if draftActionKind === "jump_to"}
                  <input
                    type="number"
                    min="1"
                    bind:value={draftJumpIndex}
                    class="jump"
                  />
                {/if}
              </label>
              <label>
                <input
                  type="radio"
                  name="draftAction"
                  value="clear_output"
                  checked={draftActionKind === "clear_output"}
                  onchange={() => (draftActionKind = "clear_output")}
                />
                Clear output
              </label>
            </div>
            <div class="row draft-row">
              <span>Trigger: <strong>{describeDraft()}</strong></span>
              <button onclick={addDraftMapping} disabled={!draftTrigger}>
                Add mapping
              </button>
            </div>
          </section>

          <section class="trigger-section">
            <h3>Mappings</h3>
            {#if !(appState?.triggers.length ?? 0)}
              <p class="empty">No mappings yet.</p>
            {/if}
            <ul class="mapping-list">
              {#each appState?.triggers ?? [] as map (map.id)}
                <li>
                  <label class="switch">
                    <input
                      type="checkbox"
                      checked={map.enabled}
                      onchange={(e) => toggleMapping(map.id, e.currentTarget.checked)}
                    />
                    <span>{map.label ?? "mapping"}</span>
                  </label>
                  <button class="danger" onclick={() => deleteMapping(map.id)}>
                    Delete
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        </div>
      {:else if tab === "network"}
        <div class="panel-network">
          <p class="hint">
            Broadcast the live Stage Display over your local network so
            performers can view it on a phone, tablet or laptop. Open the URL
            on any device on the same Wi-Fi, then enter the PIN. Slides update
            live and reconnect automatically.
          </p>

          {#if networkErr}
            <p class="status err">{networkErr}</p>
          {/if}
          {#if networkMsg}
            <p class="status">{networkMsg}</p>
          {/if}

          <section class="net-section">
            <div class="section-head">
              <h3>Stage server</h3>
              <label class="switch">
                <input
                  type="checkbox"
                  checked={networkInfo?.enabled ?? false}
                  onchange={(e) => setNetworkEnabled(e.currentTarget.checked)}
                />
                <span>{networkInfo?.enabled ? "On" : "Off"}</span>
              </label>
            </div>

            {#if networkInfo?.enabled}
              <div class="net-urls">
                {#each networkInfo.urls as url (url)}
                  <code class="net-url">{url}/stage</code>
                {/each}
                {#if !networkInfo.urls.length}
                  <p class="hint">No network interface found — check your connection.</p>
                {/if}
              </div>
              <div class="row">
                <button onclick={() => void copyStageUrl()}>
                  Copy URL
                </button>
                <button onclick={() => void refreshNetworkInfo()}>Refresh addresses</button>
              </div>
            {/if}
          </section>

          <section class="net-section">
            <h3>Port</h3>
            <p class="hint">
              The TCP port the server listens on. If the app sets the network
              source it is already using, change this to a free port.
            </p>
            <div class="row">
              <label>
                Port
                <input
                  type="number"
                  min="1"
                  max="65535"
                  bind:value={networkPortDraft}
                />
              </label>
              <button onclick={setNetworkPort}>Apply port</button>
            </div>
          </section>

          <section class="net-section">
            <h3>PIN</h3>
            <p class="hint">
              Devices must enter this PIN to view the Stage. Leave it blank to
              allow any viewer on the network.
            </p>
            <div class="row">
              <input
                type="text"
                bind:value={networkPinDraft}
                placeholder="e.g. 2471"
                class="pin-input"
              />
              <button onclick={setNetworkPin}>Apply PIN</button>
            </div>
          </section>
        </div>
      {:else if tab === "audio"}
        <div class="panel-audio">
          <p class="hint">
            Single backing track — independent utility, not tied to slides. ONE track at a time,
            routable to a specific output device, plays on its own thread (never blocks UI or Output).
            Video backgrounds remain <strong>muted</strong> at all times (Phase 4: &lt;video muted&gt;) and never conflict.
          </p>
          {#if audioErr}
            <p class="status err">{audioErr}</p>
          {/if}
          {#if audioMsg}
            <p class="status">{audioMsg}</p>
          {/if}

          <section class="audio-section">
            <h3>Output device</h3>
            <p class="hint">Choose which speaker/headphones the track plays through, independent of system default. Stored in Settings.</p>
            <div class="row">
              <select value={appState?.audio?.deviceId ?? ""} onchange={setAudioDevice}>
                <option value="">System default</option>
                {#each audioDevices as dev (dev.id)}
                  <option value={dev.id}>{dev.name} {dev.isDefault ? "(default)" : ""}</option>
                {/each}
              </select>
              <button onclick={() => void refreshAudioDevices()}>Refresh</button>
            </div>
            {#if audioDevicesMsg}
              <p class="status err">{audioDevicesMsg}</p>
            {/if}
            {#if audioDevices.length === 0}
              <p class="hint">No output devices found — check system audio.</p>
            {/if}
          </section>

          <section class="audio-section">
            <h3>Track</h3>
            <div class="row">
              <button onclick={loadAudioFile}>Load track…</button>
              <span class="audio-path">{appState?.audio?.currentPath?.split(/[\/\\]/).pop() ?? "No track loaded"}</span>
              <span class="audio-status">{appState?.audio?.status ?? "stopped"}</span>
            </div>
            <div class="row">
              <button onclick={playAudio} disabled={appState?.audio?.status === "playing"}>Play</button>
              <button onclick={pauseAudio} disabled={appState?.audio?.status !== "playing"}>Pause</button>
              <button onclick={stopAudio} disabled={appState?.audio?.status === "stopped" || !appState?.audio?.currentPath}>Stop</button>
            </div>
          </section>

          <section class="audio-section">
            <h3>Volume</h3>
            <div class="row">
              <input
                type="range"
                min="0"
                max="1.5"
                step="0.05"
                value={audioVolumeDraft}
                oninput={(e) => setAudioVolume(Number((e.target as HTMLInputElement).value))}
              />
              <span>{Math.round(audioVolumeDraft * 100)}%</span>
            </div>
          </section>

          <p class="hint">Decode/playback runs entirely on its own thread — never blocks the main thread or introduces Windows deadlock patterns. Not tied to slides/playlist.</p>
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
    display: flex;
    align-items: center;
    justify-content: center;
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
    width: min(680px, 92vw);
    max-height: 90vh;
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
    /* Six tabs overflow narrow dialogs (dialog is overflow:hidden) — scroll
       the tab strip instead of clipping Audio/Logs out of reach. */
    overflow-x: auto;
  }

  .tab {
    background: transparent;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 6px 6px 0 0;
    padding: 8px 14px;
    color: var(--text-dim);
    flex: none;
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

  .bcast-block {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 16px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel-2);
  }

  .bcast-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .bcast-title .assign-title {
    margin: 0;
  }

  .bcast-title label.check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    color: var(--text);
  }

  .bcast-block .hint {
    margin: 0;
  }

  .bcast-block code {
    color: var(--text);
  }

  .bcast-block a {
    color: var(--accent);
  }

  .muted-note {
    font-weight: normal;
    font-size: 12px;
    color: var(--semantic-idle, #64748b);
  }

  /* NDI receive confidence monitor — LIVE/STALE reuse the design-system
     semantic tokens (no new colors invented). */
  .ndimon-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .ndimon-badge {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    padding: 3px 10px;
    border-radius: 999px;
    border: 1px solid var(--semantic-neutral, #94a3b8);
    color: var(--semantic-idle, #64748b);
    background: var(--semantic-neutral-bg, rgba(148, 163, 184, 0.08));
    white-space: nowrap;
  }

  .ndimon-badge[data-s="live"] {
    color: var(--semantic-live, #1f9d6a);
    background: var(--semantic-live-bg, rgba(31, 157, 106, 0.14));
    border-color: var(--semantic-live-border, rgba(31, 157, 106, 0.32));
    box-shadow: var(--semantic-live-glow, 0 0 12px rgba(31, 157, 106, 0.35));
  }

  .ndimon-badge[data-s="stale"] {
    color: var(--semantic-warning, #f7b538);
    background: var(--semantic-warning-bg, rgba(247, 181, 56, 0.14));
    border-color: var(--semantic-warning-border, rgba(247, 181, 56, 0.32));
  }

  .ndimon-badge[data-s="error"] {
    color: var(--semantic-error, #e11d48);
    background: var(--semantic-error-bg, rgba(225, 29, 72, 0.12));
    border-color: var(--semantic-error-border, rgba(225, 29, 72, 0.28));
  }

  .ndimon-msg {
    font-size: 12px;
    color: var(--text);
    opacity: 0.85;
  }

  .ndimon-sources {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .ndimon-sources button.on {
    border-color: var(--semantic-live, #1f9d6a);
    color: var(--semantic-live, #1f9d6a);
  }

  .ndimon-preview {
    position: relative;
    max-width: 420px;
  }

  .ndimon-preview img {
    display: block;
    width: 100%;
    border-radius: 8px;
    border: 2px solid var(--border);
  }

  .ndimon-preview[data-s="live"] img {
    border-color: var(--semantic-live-border, rgba(31, 157, 106, 0.32));
    box-shadow: var(--semantic-live-glow, 0 0 12px rgba(31, 157, 106, 0.35));
  }

  .ndimon-preview[data-s="stale"] img {
    border-color: var(--semantic-warning, #f7b538);
  }

  .ndimon-staleveil {
    position: absolute;
    left: 8px;
    bottom: 8px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    padding: 3px 10px;
    border-radius: 999px;
    color: var(--semantic-warning, #f7b538);
    background: rgba(0, 0, 0, 0.72);
    border: 1px solid var(--semantic-warning, #f7b538);
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

  /* Narrow dialogs (92vw on small windows / high zoom): stack the Looks list
     above the editor instead of squeezing both into 200px + scraps. */
  @media (max-width: 560px) {
    .looks-layout {
      grid-template-columns: 1fr;
    }
    .looks-list {
      flex-direction: row;
      flex-wrap: wrap;
    }
    .look-pill {
      flex: 1 1 140px;
    }
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

  .badge.ndi {
    color: var(--warn);
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

  .positioning-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }

  .box-editor {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .box-canvas {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    background:
      linear-gradient(135deg, #16232f 0%, #0d141c 100%);
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    touch-action: none;
    user-select: none;
  }

  .box-canvas::after {
    content: "";
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(to right, rgba(255, 255, 255, 0.05) 1px, transparent 1px),
      linear-gradient(to bottom, rgba(255, 255, 255, 0.05) 1px, transparent 1px);
    background-size: 10% 10%;
    pointer-events: none;
  }

  .box {
    position: absolute;
    box-sizing: border-box;
    cursor: move;
    border: 1.5px dashed rgba(255, 255, 255, 0.55);
    border-radius: 6px;
    display: flex;
    align-items: flex-start;
    justify-content: flex-start;
    padding: 6px;
  }

  .box.title {
    background: rgba(97, 175, 239, 0.16);
    border-color: #61afef;
  }

  .box.body {
    background: rgba(152, 195, 121, 0.16);
    border-color: #98c379;
  }

  .box-label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgba(255, 255, 255, 0.85);
    pointer-events: none;
  }

  .handle {
    position: absolute;
    right: -5px;
    bottom: -5px;
    width: 12px;
    height: 12px;
    border-radius: 3px;
    background: #fff;
    border: 1.5px solid #000;
    cursor: nwse-resize;
  }

  .box-fields {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .box-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .box-field code {
    font-size: 11px;
    color: var(--text-dim);
    white-space: nowrap;
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

  .panel-triggers {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 4px;
    overflow-y: auto;
  }

  .trigger-section {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .section-head h3 {
    margin: 0;
    font-size: 14px;
  }

  .trigger-section h4 {
    margin: 0 0 6px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .hint {
    font-size: 12px;
    color: var(--text-dim);
    margin: 0;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .device-select {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 10px;
    color: var(--text);
    min-width: 220px;
    flex: 1;
  }

  .osc-address {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 10px;
    color: var(--text);
    flex: 1;
    min-width: 200px;
    font-family: monospace;
  }

  .row input[type="number"] {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    color: var(--text);
    width: 90px;
  }

  .switch {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text);
    cursor: pointer;
    user-select: none;
  }

  .monitor {
    border-top: 1px solid var(--border);
    padding-top: 10px;
  }

  .midi-list,
  .mapping-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .midi-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    font-size: 13px;
  }

  .midi-desc {
    font-family: monospace;
    color: var(--text);
  }

  .midi-list li.empty,
  .empty {
    color: var(--text-dim);
    font-size: 13px;
  }

  .action-picker {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    align-items: center;
  }

  .action-picker label {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text);
    cursor: pointer;
  }

  .action-picker input.jump {
    width: 70px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 6px;
    color: var(--text);
  }

  .draft-row {
    justify-content: space-between;
    font-size: 13px;
    color: var(--text);
  }

  .mapping-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .panel-triggers button {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    color: var(--text);
    padding: 6px 12px;
    cursor: pointer;
  }

  .panel-triggers button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .panel-triggers button.danger {
    border-color: rgba(200, 60, 60, 0.5);
    color: #e07a7a;
  }

  .panel-network {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 4px;
    overflow-y: auto;
  }

  .panel-network .net-section {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .panel-network .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .panel-network .section-head h3,
  .panel-network .net-section h3 {
    margin: 0;
    font-size: 14px;
  }

  .net-urls {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .net-url {
    display: inline-block;
    padding: 8px 10px;
    background: #101218;
    border: 1px solid var(--border);
    border-radius: 6px;
    font-family: monospace;
    font-size: 13px;
    color: var(--text);
    word-break: break-all;
  }

  .panel-network button {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    color: var(--text);
    padding: 6px 12px;
  }

  .panel-network .row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .panel-network .row label {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .panel-network .row input[type="number"] {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    color: var(--text);
    width: 90px;
  }

  .panel-network .pin-input {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 10px;
    color: var(--text);
    flex: 1;
    min-width: 120px;
    font-family: monospace;
    font-size: 15px;
    letter-spacing: 0.2em;
  }

  .panel-network .switch {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text);
    cursor: pointer;
    user-select: none;
  }

  .panel-network .hint {
    font-size: 12px;
    color: var(--text-dim);
    margin: 0;
  }
</style>
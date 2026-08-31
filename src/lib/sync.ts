import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AutosaveEvent,
  ClientState,
  DisplayInfo,
  Background,
  ExportReport,
  ImportReport,
  Library,
  LogEntry,
  LookPatch,
  MediaAsset,
  MidiDeviceInfo,
  MidiMessageView,
  ScriptureMatch,
  Transition,
  Trigger,
  TriggerAction,
} from "./types";

export function subscribeState(cb: (state: ClientState) => void): Promise<UnlistenFn> {
  return listen<ClientState>("state", (event) => cb(event.payload));
}

export function subscribeLibrary(cb: (library: Library) => void): Promise<UnlistenFn> {
  return listen<Library>("library", (event) => cb(event.payload));
}

export function subscribeAutosave(
  cb: (event: AutosaveEvent) => void,
): Promise<UnlistenFn> {
  return listen<AutosaveEvent>("autosave", (event) => cb(event.payload));
}

export function subscribeMidiMessage(
  cb: (msg: MidiMessageView) => void,
): Promise<UnlistenFn> {
  return listen<MidiMessageView>("midi-message", (event) => cb(event.payload));
}

export const api = {
  getState: () => invoke<ClientState>("get_state"),

  setLiveSlide: (slideId: string) =>
    invoke<ClientState>("set_live_slide", { slideId }),

  clearOutput: () => invoke<ClientState>("clear_output"),

  newProject: () => invoke<ClientState>("new_project"),

  addSlide: (title?: string, body?: string) =>
    invoke<ClientState>("add_slide", { title, body }),

  updateSlide: (
    slideId: string,
    patch: { title?: string; body?: string; background?: Background },
  ) => invoke<ClientState>("update_slide", { slideId, ...patch }),

  deleteSlide: (slideId: string) =>
    invoke<ClientState>("delete_slide", { slideId }),

  listDisplays: () => invoke<DisplayInfo[]>("list_displays"),

  setOutputDisplay: (index: number) =>
    invoke<DisplayInfo[]>("set_output_display", { index }),

  toggleOutputFullscreen: () =>
    invoke<boolean>("toggle_output_fullscreen"),

  showOutput: () => invoke<ClientState>("show_output"),

  setStageDisplay: (index: number) =>
    invoke<DisplayInfo[]>("set_stage_display", { index }),

  toggleStage: () => invoke<boolean>("toggle_stage"),

  getLibrary: () => invoke<Library>("get_library"),

  addLibrarySong: (title: string, body?: string, background?: Background) =>
    invoke<Library>("add_library_song", { title, body, background }),

  deleteLibrarySong: (songId: string) =>
    invoke<Library>("delete_library_song", { songId }),

  addSongToPlaylist: (songId: string) =>
    invoke<ClientState>("add_song_to_playlist", { songId }),

  setTransition: (transition: Transition) =>
    invoke<ClientState>("set_transition", { transition }),

  upsertLook: (lookId: string | null, patch: LookPatch) =>
    invoke<ClientState>("upsert_look", { lookId, patch }),

  deleteLook: (lookId: string) =>
    invoke<ClientState>("delete_look", { lookId }),

  setOutputLook: (lookId: string | null) =>
    invoke<ClientState>("set_output_look", { lookId }),

  setStageLook: (lookId: string | null) =>
    invoke<ClientState>("set_stage_look", { lookId }),

  setNdiLook: (lookId: string | null) =>
    invoke<ClientState>("set_ndi_look", { lookId }),

  setNdiEnabled: (enabled: boolean) =>
    invoke<ClientState>("set_ndi_enabled", { enabled }),

  importMedia: (path: string) => invoke<MediaAsset>("import_media", { path }),

  exportSettings: (path: string) =>
    invoke<ExportReport>("export_settings", { path }),

  importSettings: (path: string) =>
    invoke<ImportReport>("import_settings", { path }),

  getLogs: (limit?: number) => invoke<LogEntry[]>("get_logs", { limit }),

  exportLogs: (path: string) =>
    invoke<string>("export_logs_to", { path }),

  searchScripture: (query: string) =>
    invoke<ScriptureMatch[]>("search_scripture", { query }),

  listMidiDevices: () => invoke<MidiDeviceInfo[]>("list_midi_devices"),

  setMidiEnabled: (enabled: boolean) =>
    invoke<ClientState>("set_midi_enabled", { enabled }),

  setMidiDevice: (deviceId: string) =>
    invoke<ClientState>("set_midi_device", { deviceId }),

  setOscEnabled: (enabled: boolean) =>
    invoke<ClientState>("set_osc_enabled", { enabled }),

  setOscPort: (port: number) => invoke<ClientState>("set_osc_port", { port }),

  addTrigger: (trigger: Trigger, action: TriggerAction, label?: string) =>
    invoke<ClientState>("add_trigger", { trigger, action, label }),

  deleteTrigger: (triggerId: string) =>
    invoke<ClientState>("delete_trigger", { triggerId }),

  setTriggerEnabled: (triggerId: string, enabled: boolean) =>
    invoke<ClientState>("set_trigger_enabled", { triggerId, enabled }),
};
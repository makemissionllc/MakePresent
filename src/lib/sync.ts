import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AutosaveEvent,
  ClientState,
  DisplayInfo,
  Background,
} from "./types";

export function subscribeState(cb: (state: ClientState) => void): Promise<UnlistenFn> {
  return listen<ClientState>("state", (event) => cb(event.payload));
}

export function subscribeAutosave(
  cb: (event: AutosaveEvent) => void,
): Promise<UnlistenFn> {
  return listen<AutosaveEvent>("autosave", (event) => cb(event.payload));
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
};
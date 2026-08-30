export interface Background {
  type: "solid";
  color: string;
}

export interface Slide {
  id: string;
  title: string;
  body: string;
  background: Background;
}

export interface Project {
  schemaVersion: number;
  id: string;
  name: string;
  slides: Slide[];
  live: string | null;
  modifiedAt: string;
}

export interface Notice {
  kind: string;
  message: string;
  at: string | null;
}

export interface OutputView {
  monitorIndex: number | null;
  monitorName: string | null;
  fullscreen: boolean;
}

export interface ClientState {
  project: Project;
  notice: Notice | null;
  output: OutputView;
}

export interface DisplayInfo {
  index: number;
  name: string;
  width: number;
  height: number;
  x: number;
  y: number;
  primary: boolean;
  current: boolean;
}

export interface AutosaveEvent {
  status: "saved" | "error";
  at?: string;
  message?: string;
}
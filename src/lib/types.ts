export interface Background {
  type: "solid";
  color: string;
}

export interface Slide {
  id: string;
  libraryId: string | null;
  librarySlideId: string | null;
  title: string;
  body: string;
  background: Background;
}

export type Transition = "cut" | "fade";

export interface Project {
  schemaVersion: number;
  id: string;
  name: string;
  slides: Slide[];
  live: string | null;
  transition: Transition;
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

export interface StageView {
  visible: boolean;
  monitorIndex: number | null;
  monitorName: string | null;
}

export interface ClientState {
  project: Project;
  notice: Notice | null;
  output: OutputView;
  stage: StageView;
  current: Slide | null;
  next: Slide | null;
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

export interface LibrarySlide {
  id: string;
  title: string;
  body: string;
}

export interface LibrarySong {
  id: string;
  title: string;
  defaultBackground: Background;
  slides: LibrarySlide[];
}

export interface Library {
  schemaVersion: number;
  songs: LibrarySong[];
}

export interface AutosaveEvent {
  status: "saved" | "error";
  at?: string;
  message?: string;
}
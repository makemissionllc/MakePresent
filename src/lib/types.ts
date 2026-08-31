export interface SolidBackground {
  type: "solid";
  color: string;
}

export interface ImageBackground {
  type: "image";
  path: string;
  hash: string;
  thumb: string;
}

export interface VideoBackground {
  type: "video";
  path: string;
  hash: string;
  thumb: string;
  durationMs: number | null;
}

export type Background = SolidBackground | ImageBackground | VideoBackground;

export interface MediaAsset {
  background: Background;
  kind: "image" | "video";
  fileName: string;
  hash: string;
  durationMs: number | null;
}

export function isMedia(bg: Background): bg is ImageBackground | VideoBackground {
  return bg.type === "image" || bg.type === "video";
}

export interface Slide {
  id: string;
  libraryId: string | null;
  librarySlideId: string | null;
  title: string;
  body: string;
  background: Background;
}

export type TextPosition = "top" | "center" | "bottom";

export interface Look {
  id: string;
  name: string;
  titleSize: number;
  bodySize: number;
  textColor: string;
  showBackground: boolean;
  textPosition: TextPosition;
}

export type Transition = "cut" | "fade";

export interface Project {
  schemaVersion: number;
  id: string;
  name: string;
  slides: Slide[];
  looks: Look[];
  live: string | null;
  transition: Transition;
  modifiedAt: string;
}

export interface LookPatch {
  name?: string;
  titleSize?: number;
  bodySize?: number;
  textColor?: string;
  showBackground?: boolean;
  textPosition?: TextPosition;
}

export interface Notice {
  kind: string;
  message: string;
  at: string | null;
}

export interface OutputView {
  visible: boolean;
  monitorIndex: number | null;
  monitorName: string | null;
  fullscreen: boolean;
}

export interface StageView {
  visible: boolean;
  monitorIndex: number | null;
  monitorName: string | null;
}

export interface BroadcastView {
  enabled: boolean;
  sourceName: string;
}

export interface ClientState {
  project: Project;
  notice: Notice | null;
  output: OutputView;
  stage: StageView;
  broadcast: BroadcastView;
  firstRun: boolean;
  defaultTransition: Transition;
  current: Slide | null;
  next: Slide | null;
  onDeck: Slide | null;
  looks: Look[];
  outputLookId: string | null;
  stageLookId: string | null;
  ndiLookId: string | null;
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

export interface ExportReport {
  path: string;
  fields: string[];
}

export interface ImportReport {
  changedFields: string[];
  message: string;
}

export interface LogEntry {
  at: string;
  level: string;
  message: string;
}

export interface ScriptureMatch {
  book: string;
  chapter: number;
  verse: number;
  reference: string;
  text: string;
}
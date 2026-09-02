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

export type Positioning = "auto" | "absolute";

export interface BoxGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
  zIndex: number;
}

export interface Look {
  id: string;
  name: string;
  titleSize: number;
  bodySize: number;
  titleFont: string;
  bodyFont: string;
  textColor: string;
  showBackground: boolean;
  textPosition: TextPosition;
  positioning: Positioning;
  titleBox: BoxGeometry;
  bodyBox: BoxGeometry;
}

export type Transition = "cut" | "fade";

export interface Project {
  schemaVersion: number;
  id: string;
  name: string;
  slides: Slide[];
  looks: Look[];
  live: string | null;
  showText: boolean;
  showBackground: boolean;
  selected?: string | null;
  aspectRatio?: string;
  transition: Transition;
  modifiedAt: string;
}

export interface LookPatch {
  name?: string;
  titleSize?: number;
  bodySize?: number;
  titleFont?: string;
  bodyFont?: string;
  textColor?: string;
  showBackground?: boolean;
  textPosition?: TextPosition;
  positioning?: Positioning;
  titleBox?: BoxGeometry;
  bodyBox?: BoxGeometry;
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
  midiEnabled: boolean;
  midiDeviceId: string | null;
  oscEnabled: boolean;
  oscPort: number;
  triggers: TriggerMapping[];
  stageNetworkEnabled: boolean;
  stageNetworkPort: number;
}

export interface StageNetworkInfo {
  bindHost: string;
  urls: string[];
  port: number;
  enabled: boolean;
  pin: string;
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

export interface SlidePositioning {
  vAlign: "top" | "center" | "bottom";
  hAlign: "left" | "center" | "right";
}

export interface LibrarySlide {
  id: string;
  title: string;
  body: string;
  positioning?: SlidePositioning | null;
  groupId?: string | null;
  groupLabel?: string | null;
}

export interface LibrarySong {
  id: string;
  title: string;
  defaultBackground: Background;
  slides: LibrarySlide[];
}

export interface ServicePresetItem {
  title: string;
  type: "slide" | "song" | "scripture";
  referenceId?: string;
  content?: string;
}

export interface ServicePreset {
  id: string;
  name: string;
  category: "Sunday Service" | "Midweek" | "Youth" | "Custom";
  description: string;
  defaultAspect: "16:9" | "4:3" | "Vertical";
  playlistItems: ServicePresetItem[];
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

export interface ScriptureImportResult {
  books: number;
  verses: number;
  totalBooks: number;
}

export interface BibleInfo {
  id: string;
  name: string;
  bookCount: number;
}

export interface ChapterVerse {
  verse: number;
  text: string;
}

export type Trigger =
  | { kind: "midi_note"; channel: number; note: number }
  | { kind: "midi_control"; channel: number; controller: number; value: number | null }
  | { kind: "midi_program"; channel: number; program: number }
  | { kind: "osc_address"; address: string };

export type TriggerAction =
  | { kind: "next_slide" }
  | { kind: "prev_slide" }
  | { kind: "jump_to"; index: number }
  | { kind: "clear_output" };

export interface TriggerMapping {
  id: string;
  trigger: Trigger;
  action: TriggerAction;
  enabled: boolean;
  label?: string | null;
}

export interface MidiDeviceInfo {
  id: string;
  name: string;
}

export interface MidiMessageView {
  channel: number;
  kind: string;
  data: string;
  describe: string;
  data1: number | null;
  data2: number | null;
}
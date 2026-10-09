use crate::logging::Level;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;
use tauri::{Emitter, Manager};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;
pub const LIBRARY_SCHEMA_VERSION: u32 = 2;
/// Debounce window for autosave. Every edit is persisted well within 2 seconds.
pub const AUTOSAVE_DEBOUNCE_MS: u64 = 1200;
pub const MAX_SNAPSHOTS: usize = 50;

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// ---------------------------------------------------------------------------
// Domain model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Background {
    Solid { color: String },
    /// Full-bleed image background (managed copy inside the app data dir,
    /// cached and thumbnailed by content hash).
    Image { path: String, hash: String, thumb: String },
    /// Full-bleed looping video background (audio is out of scope this phase).
    Video {
        path: String,
        hash: String,
        thumb: String,
        #[serde(default)]
        duration_ms: Option<u64>,
    },
    /// Live camera / capture-card input (UVC webcam or an HDMI capture card
    /// presenting as one). Rendered by the webview via getUserMedia — no Rust
    /// video processing happens for this variant, and it holds no file
    /// reference, so the media-cache verifier skips it. `device_id` is the
    /// browser media-device id for exact matching; `label` is the human name
    /// shown in the UI and the fallback match when ids rotate across restarts.
    LiveCamera {
        #[serde(default, rename = "deviceId")]
        device_id: Option<String>,
        #[serde(default)]
        label: String,
    },
}

impl Default for Background {
    fn default() -> Self {
        Background::Solid {
            color: "#123a5c".to_string(),
        }
    }
}

/// How the Output (and Stage) switch from one live slide to the next.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    #[default]
    Cut,
    Fade,
    Wipe,
    Push,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SlideKind {
    #[default]
    Generic,
    Song,
    Scripture,
}

/// Whether a slide uses its own background or the item/kind defaults.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundMode {
    Inherit,
    #[default]
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slide {
    pub id: String,
    /// Rust-owned playlist grouping identity. Missing on legacy projects.
    #[serde(default)]
    pub item_id: Option<String>,
    /// Shared item label so renaming does not overwrite slide labels.
    #[serde(default)]
    pub item_name: Option<String>,
    /// When this playlist slide was added from the library, the source song id.
    #[serde(default)]
    pub library_id: Option<String>,
    /// The source verse/section id within that song.
    #[serde(default)]
    pub library_slide_id: Option<String>,
    /// Display name for playlist/grid labels — distinct from `title` which is the
    /// on-screen rendered title text. `None` means “follow title” (legacy slides
    /// and new slides before an explicit name is set show `title` as label).
    #[serde(default)]
    pub name: Option<String>,
    /// Kind tag inferred from creation origin — Scripture (Add Scripture/Browse), Song (Library), Generic (manual + Add slide).
    /// Stored as metadata, not user-editable. `Generic` is the default for legacy slides and manual slides.
    #[serde(default)]
    pub kind: SlideKind,
    pub title: String,
    pub body: String,
    pub background: Background,
    /// Legacy slides remain Custom by default; new text slides are Inherit.
    #[serde(default)]
    pub background_mode: BackgroundMode,
    /// Optional per-slide auto-advance timer: when Some(n) and this slide is
    /// live, the backend automatically advances to the next playlist item after
    /// n seconds. None / 0 means no auto-advance. Stored per slide so templates
    /// and persistence cover it.
    #[serde(default)]
    pub auto_advance_secs: Option<u64>,
}

impl Slide {
    /// Display label for playlist/grid: explicit `name` if set and non-empty,
    /// otherwise the on-screen `title`, otherwise "Untitled".
    pub fn display_name(&self) -> String {
        if let Some(n) = &self.name {
            let t = n.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
        let t = self.title.trim();
        if !t.is_empty() {
            return t.to_string();
        }
        "Untitled".to_string()
    }
}

/// Where the slide text is placed within its frame.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextPosition {
    Top,
    #[default]
    Center,
    Bottom,
}

/// How a text block is placed within its frame.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Positioning {
    /// Text flows naturally (centred / top / bottom) and fills available space.
    #[default]
    Auto,
    /// Text is placed in an explicit bounding box (FreeShow-style) using the
    /// per-role geometry stored on the Look.
    Absolute,
}

/// A single draggable text box's geometry, in percent of the frame (0-100).
/// `width`/`height` are the box extent; `x`/`y` are the top-left corner.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoxGeometry {
    #[serde(default = "default_box_x")]
    pub x: f32,
    #[serde(default = "default_box_y")]
    pub y: f32,
    #[serde(default = "default_box_width")]
    pub width: f32,
    #[serde(default = "default_box_height")]
    pub height: f32,
    #[serde(default = "default_box_z")]
    pub z_index: u32,
}

fn default_box_x() -> f32 {
    5.0
}
fn default_box_y() -> f32 {
    10.0
}
fn default_box_width() -> f32 {
    90.0
}
fn default_box_height() -> f32 {
    20.0
}
fn default_box_z() -> u32 {
    1
}

impl Default for BoxGeometry {
    fn default() -> Self {
        Self {
            x: default_box_x(),
            y: default_box_y(),
            width: default_box_width(),
            height: default_box_height(),
            z_index: default_box_z(),
        }
    }
}

/// Horizontal alignment of one text element within its frame/box.
/// Center is the default: projection text reads best centred, and both the
/// built-in Main/Stage Looks plus every newly created Look start centred.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum HAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// Per-text-element styling (FreeShow textbox-inspired): one `TextStyle` for
/// the Title role, one for the Body role. Scripture slides have no third text
/// element — the reference/translation line *is* the slide title
/// (`scripture.rs` feeds `add_slide` with title = reference, body = verse
/// text), so the Title style covers it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextStyle {
    /// Horizontal alignment of this element (default centre).
    #[serde(default)]
    pub align: HAlign,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub all_caps: bool,
    #[serde(default = "default_true")]
    pub shrink_to_fit: bool,
    #[serde(default)]
    pub min_size: Option<u32>,
    /// Line height multiplier (default 1.1 title / 1.4 body, matching the
    /// long-standing renderer CSS so legacy projects render identically).
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    /// Text-shadow blur radius in px (colour stays the renderer's soft black).
    #[serde(default = "default_shadow_blur")]
    pub shadow_blur: f32,
    /// Text-shadow horizontal offset in px.
    #[serde(default)]
    pub shadow_x: f32,
    /// Text-shadow vertical offset in px.
    #[serde(default = "default_shadow_y")]
    pub shadow_y: f32,
    /// Outline width in px via `-webkit-text-stroke` (0 = off).
    #[serde(default)]
    pub outline_width: f32,
    /// Outline colour (used only when `outline_width` > 0).
    #[serde(default = "default_outline_color")]
    pub outline_color: String,
    /// Optional readability bar behind the text (hex colour). Only painted
    /// when `bg_opacity` > 0, so the default is effectively "off".
    #[serde(default = "default_text_bg")]
    pub bg_color: String,
    /// Opacity of the readability bar, 0.0 (off) .. 1.0 (opaque).
    #[serde(default)]
    pub bg_opacity: f32,
}

fn default_line_height() -> f32 {
    1.25
}
fn default_shadow_blur() -> f32 {
    20.0
}
fn default_shadow_y() -> f32 {
    2.0
}
fn default_outline_color() -> String {
    "#000000".to_string()
}
fn default_text_bg() -> String {
    "#000000".to_string()
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            align: HAlign::Center,
            bold: false,
            color: None,
            italic: false,
            all_caps: false,
            shrink_to_fit: true,
            min_size: None,
            line_height: default_line_height(),
            shadow_blur: default_shadow_blur(),
            shadow_x: 0.0,
            shadow_y: default_shadow_y(),
            outline_width: 0.0,
            outline_color: default_outline_color(),
            bg_color: default_text_bg(),
            bg_opacity: 0.0,
        }
    }
}

impl TextStyle {
    /// Title-role defaults: reproduce the historic title rendering
    /// (`line-height: 1.1`, `0 2px 24px rgba(0,0,0,.45)`).
    pub fn title_default() -> Self {
        Self {
            line_height: 1.1,
            shadow_blur: 24.0,
            min_size: Some(24),
            ..Self::default()
        }
    }

    /// Body-role defaults: reproduce the historic body rendering
    /// (`line-height: 1.4`, `0 2px 20px rgba(0,0,0,.4)`).
    pub fn body_default() -> Self {
        Self {
            line_height: 1.4,
            shadow_blur: 20.0,
            ..Self::default()
        }
    }
}

/// A named style profile ("Look") that tells an output how to present the
/// *same* underlying slide differently: main audience screen, stage display,
/// or a future NDI/stream feed.
///
/// Layout: by default the text auto-flows (centred/top/bottom). Setting
/// `positioning` to `absolute` switches to a FreeShow-style template editor
/// where the title and body each live in an explicit, draggable bounding box
/// (`title_box` / `body_box`) placed in percent-of-frame coordinates and
/// translated to absolute CSS by the renderer.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    pub id: String,
    pub name: String,
    /// Base font size for the slide title (px). Serves as the fitText ceiling;
    /// text still shrinks automatically when it would overflow.
    pub title_size: u32,
    /// Base font size for the slide body (px).
    pub body_size: u32,
    /// Font family for the title (e.g. "Druk Wide", "Helvetica Neue Bold").
    #[serde(default = "default_title_font")]
    pub title_font: String,
    /// Font family for the body text.
    #[serde(default = "default_body_font")]
    pub body_font: String,
    /// Override colour for the text.
    pub text_color: String,
    /// Whether the slide's background (solid colour or media) is drawn. When
    /// off only the text is shown, e.g. transparent for stage/stream compositing.
    pub show_background: bool,
    /// Title-only Looks can hide body text; old Looks always show it.
    #[serde(default = "default_show_body")]
    pub show_body: bool,
    /// Vertical placement of the text block within the frame (auto mode only).
    pub text_position: TextPosition,
    /// Per-element text styling for the Title role (also the scripture
    /// reference line — scripture slides render the reference as the title).
    #[serde(default = "TextStyle::title_default")]
    pub title_style: TextStyle,
    /// Per-element text styling for the Body role (verse/lyric text).
    #[serde(default = "TextStyle::body_default")]
    pub body_style: TextStyle,
    /// Whether text uses auto flow or explicit absolute bounding boxes.
    #[serde(default)]
    pub positioning: Positioning,
    /// Geometry of the title box (absolute mode).
    #[serde(default)]
    pub title_box: BoxGeometry,
    /// Geometry of the body box (absolute mode).
    #[serde(default)]
    pub body_box: BoxGeometry,
    /// Optional background used by inheriting slides whose kind maps to this Look.
    #[serde(default)]
    pub background: Option<Background>,
}

fn default_title_font() -> String {
    "sans-serif".to_string()
}
fn default_body_font() -> String {
    "sans-serif".to_string()
}
fn default_show_body() -> bool { true }

impl Look {
    pub fn main_default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "Main".to_string(),
            title_size: 72,
            body_size: 40,
            title_font: default_title_font(),
            body_font: default_body_font(),
            text_color: "#ffffff".to_string(),
            show_background: true,
            show_body: true,
            text_position: TextPosition::Center,
            title_style: TextStyle::title_default(),
            body_style: TextStyle::body_default(),
            positioning: Positioning::Auto,
            title_box: BoxGeometry::default(),
            body_box: BoxGeometry::default(),
            background: None,
        }
    }

    pub fn stage_default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "Stage".to_string(),
            title_size: 60,
            body_size: 56,
            title_font: default_title_font(),
            body_font: default_body_font(),
            text_color: "#ffffff".to_string(),
            show_background: false,
            show_body: true,
            text_position: TextPosition::Center,
            title_style: TextStyle::title_default(),
            body_style: TextStyle::body_default(),
            positioning: Positioning::Auto,
            title_box: BoxGeometry::default(),
            body_box: BoxGeometry::default(),
            background: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub slides: Vec<Slide>,
    /// Shared background overrides keyed by playlist item id.
    #[serde(default)]
    pub item_backgrounds: HashMap<String, Background>,
    /// Per-item Output Look overrides. Empty for legacy projects.
    #[serde(default)]
    pub item_looks: HashMap<String, String>,
    /// Named style profiles (Looks) that outputs render against. Stored with
    /// the project so they save/load with autosave. Defaults are seeded on new
    /// (and legacy) projects.
    #[serde(default)]
    pub looks: Vec<Look>,
    pub live: Option<String>,
    #[serde(default = "default_true")]
    pub show_text: bool,
    #[serde(default = "default_true")]
    pub show_background: bool,
    /// The slide currently selected/being-armed in the editor (used to decide
    /// which media the Output preloads "on deck").
    #[serde(default)]
    pub selected: Option<String>,
    /// How the Output switches between live slides ("cut" or "fade").
    #[serde(default)]
    pub transition: Transition,
    #[serde(default = "default_aspect")]
    pub aspect_ratio: String,
    pub modified_at: String,
}

fn default_aspect() -> String { "16:9".to_string() }
fn default_true() -> bool { true }

impl Project {
    pub fn new(name: &str) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            slides: vec![Slide {
                id: Uuid::new_v4().to_string(),
                item_id: Some(Uuid::new_v4().to_string()),
                item_name: Some("Welcome to MakrStudio".to_string()),
                library_id: None,
                library_slide_id: None,
                name: Some("Welcome to MakrStudio".to_string()),
                kind: SlideKind::Generic,
                title: "Welcome to MakrStudio".to_string(),
                body: "This is the Phase 1 test slide.".to_string(),
                background: Background::default(),
                background_mode: BackgroundMode::Custom,
                auto_advance_secs: None,
            }],
            item_backgrounds: HashMap::new(),
            item_looks: HashMap::new(),
            looks: vec![Look::main_default(), Look::stage_default()],
            live: None,
            show_text: true,
            show_background: true,
            selected: None,
            transition: Transition::Cut,
            aspect_ratio: default_aspect(),
            modified_at: now_iso(),
        }
    }

    pub fn from_preset(
        name: &str,
        aspect: &str,
        transition: Transition,
        preset: &ServicePreset,
    ) -> Self {
        let mut p = Self::new(name);
        p.aspect_ratio = aspect.to_string();
        p.transition = transition;
        if preset.id == "blank" {
            p.slides = vec![];
        } else {
            p.slides = preset
                .playlist_items
                .iter()
                .map(|it| Slide {
                    id: Uuid::new_v4().to_string(),
                    item_id: Some(Uuid::new_v4().to_string()),
                    item_name: Some(it.title.clone()),
                    library_id: None,
                    library_slide_id: None,
                    name: Some(it.title.clone()),
                    kind: match it.item_type.as_str() {
                        "song" => SlideKind::Song,
                        "scripture" => SlideKind::Scripture,
                        _ => SlideKind::Generic,
                    },
                    title: it.title.clone(),
                    body: it.content.clone().unwrap_or_default(),
                    background: Background::default(),
                    background_mode: BackgroundMode::Custom,
                    auto_advance_secs: None,
                })
                .collect();
        }
        if let Some(first) = p.slides.first() {
            p.selected = Some(first.id.clone());
        }
        p.modified_at = now_iso();
        p
    }

    /// Guarantee at least the default Main/Stage looks exist. Called on legacy
    /// projects loaded from disk that predate the Looks feature.
    pub fn ensure_default_looks(&mut self) {
        if self.looks.is_empty() {
            self.looks.push(Look::main_default());
            self.looks.push(Look::stage_default());
        }
    }

    pub fn find_look(&self, id: &str) -> Option<&Look> {
        self.looks.iter().find(|l| l.id == id)
    }

    pub fn test() -> Self {
        Self::new("First Service")
    }

    pub fn find(&self, id: &str) -> Option<&Slide> {
        self.slides.iter().find(|s| s.id == id)
    }

    /// The slide queued after the given id in the playlist (used for the
    /// Stage Display "next" preview). Returns None when nothing follows.
    pub fn next_slide(&self, id: &str) -> Option<&Slide> {
        let index = self.slides.iter().position(|s| s.id == id)?;
        self.slides.get(index + 1)
    }

    /// The slide whose media the Output should preload. The editor's selected
    /// slide wins when it is not already live; otherwise it is the next slide
    /// in the playlist (the operator's most likely next cue).
    pub fn on_deck(&self) -> Option<&Slide> {
        match &self.selected {
            Some(id) if self.live.as_deref() == Some(id.as_str()) => self.next_slide(id),
            Some(id) => self.find(id),
            None => self.live.as_deref().and_then(|id| self.next_slide(id)),
        }
    }

    pub fn effective_background(&self, slide: &Slide, defaults: &DefaultLooks) -> Background {
        if slide.background_mode == BackgroundMode::Custom {
            return slide.background.clone();
        }
        if let Some(background) = slide.item_id.as_ref().and_then(|id| self.item_backgrounds.get(id)) {
            return background.clone();
        }
        let kind_look_id = match slide.kind {
            SlideKind::Scripture => defaults.scripture.as_deref(),
            SlideKind::Song => defaults.song.as_deref(),
            SlideKind::Generic => defaults.generic.as_deref(),
        };
        let look = slide.item_id.as_ref().and_then(|id| self.item_looks.get(id))
            .and_then(|id| self.find_look(id))
            .or_else(|| kind_look_id
            .and_then(|id| self.find_look(id))
            )
            .or_else(|| self.looks.iter().find(|look| look.name == "Main"))
            .or_else(|| self.looks.first());
        look.and_then(|look| look.background.clone()).unwrap_or_default()
    }

    /// Resolve the Output Look from explicit item override, assigned kind Look,
    /// then the Output mapping. Old projects with no kind assignment therefore
    /// retain their prior Output mapping unchanged.
    pub fn effective_output_look_id(
        &self,
        slide: &Slide,
        defaults: &DefaultLooks,
        output_look_id: Option<&str>,
    ) -> Option<String> {
        let item = slide.item_id.as_ref().and_then(|id| self.item_looks.get(id)).map(String::as_str);
        let kind = match slide.kind {
            SlideKind::Song => defaults.song.as_deref(),
            SlideKind::Scripture => defaults.scripture.as_deref(),
            SlideKind::Generic => defaults.generic.as_deref(),
        };
        let item = item.filter(|id| self.find_look(id).is_some());
        let kind = kind.filter(|id| self.find_look(id).is_some());
        let output = output_look_id.filter(|id| self.find_look(id).is_some());
        item.or(kind).or(output)
            .or_else(|| self.looks.iter().find(|look| look.name == "Main").map(|look| look.id.as_str()))
            .or_else(|| self.looks.first().map(|look| look.id.as_str()))
            .map(str::to_string)
    }

    pub fn remove_look_and_reassign(&mut self, look_id: &str, replacement: Option<&str>) -> Result<Option<String>, String> {
        if !self.looks.iter().any(|look| look.id == look_id) {
            return Err(format!("look {look_id} not found"));
        }
        if let Some(id) = replacement {
            if id == look_id || !self.looks.iter().any(|look| look.id == id) {
                return Err("Choose a different existing Look as the replacement.".into());
            }
        }
        self.looks.retain(|look| look.id != look_id);
        self.ensure_default_looks();
        let fallback = replacement.map(str::to_string)
            .or_else(|| self.looks.iter().find(|look| look.name == "Main").map(|look| look.id.clone()))
            .or_else(|| self.looks.first().map(|look| look.id.clone()));
        for assigned in self.item_looks.values_mut() {
            if assigned == look_id { if let Some(id) = &fallback { *assigned = id.clone(); } }
        }
        self.modified_at = now_iso();
        Ok(fallback)
    }

    pub fn effective_backgrounds(&self, defaults: &DefaultLooks) -> HashMap<String, Background> {
        self.slides.iter().map(|slide| {
            (slide.id.clone(), self.effective_background(slide, defaults))
        }).collect()
    }
}

// ---------------------------------------------------------------------------
// Client-facing messages
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub kind: String,
    pub message: String,
    pub at: Option<String>,
}

/// Render acknowledgment from a dumb-renderer window (Output/Stage): proof it
/// is alive and applied state. Stamped by the backend on receipt (never trusts
/// the sender's clock). `live_id` is informational only — freshness is judged
/// on `at`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderAck {
    pub at: String,
    pub live_id: Option<String>,
}

/// Latest ack per renderer window, fanned out as a tiny event (never a full
/// snapshot — acks arrive every few seconds and must not churn the UI).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AckUpdate {
    pub output: Option<RenderAck>,
    pub stage: Option<RenderAck>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputView {
    /// true once the on-demand output window exists and is showing.
    pub visible: bool,
    pub monitor_index: Option<usize>,
    pub monitor_name: Option<String>,
    pub fullscreen: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageView {
    pub visible: bool,
    pub monitor_index: Option<usize>,
    pub monitor_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioStatus {
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStateView {
    pub status: AudioStatus,
    pub current_path: Option<String>,
    pub volume: f32,
    pub device_id: Option<String>,
    #[serde(default)]
    pub duration_secs: Option<u64>,
    #[serde(default)]
    pub position_secs: Option<u64>,
}

/// Runtime status of the NDI broadcast feed (not persisted; derived live).
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastView {
    /// Whether NDI is enabled in settings and currently broadcasting.
    pub enabled: bool,
    /// The NDI source name receivers see on the network.
    pub source_name: String,
    /// Whether a real frame has ever been accepted by `send_frame`'s validation
    /// (distinct from `enabled` which only reflects "source created"). While
    /// `current` in `spawn_send_thread` is still `None` (no capture yet),
    /// this stays `false` — the source is discoverable but transmits no real video.
    pub has_real_frames: bool,
    /// ISO timestamp of the last accepted real frame, if any. Used for staleness
    /// like `RenderAck` (`ACK_STALE_MS` pattern) — if `None` or older than a few
    /// seconds, the feed is stale/not-live even though `enabled` is true.
    pub last_frame_at: Option<String>,
    /// Whether the feed is stale: enabled but no valid frame recently. True when
    /// `has_real_frames` is false or `last_frame_at` is older than the staleness
    /// window (currently ~5s, mirroring `ACK_STALE_MS`).
    pub is_stale: bool,
    #[serde(default)]
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientState {
    pub project: Project,
    /// Derived from the flat slide order; this is not separately persisted.
    pub items: Vec<PlaylistItem>,
    /// Rust-resolved background for each slide; derived at snapshot time and never persisted.
    #[serde(default)]
    pub effective_backgrounds: HashMap<String, Background>,
    /// Rust-resolved Output Look id per playlist item; renderers only look up the id.
    #[serde(default)]
    pub effective_item_look_ids: HashMap<String, String>,
    pub notice: Option<Notice>,
    pub output: OutputView,
    pub stage: StageView,
    /// Runtime NDI broadcast status.
    pub broadcast: BroadcastView,
    /// true on the very first launch (no saved project or settings yet).
    pub first_run: bool,
    /// Per-machine default transition used for new projects.
    pub default_transition: Transition,
    /// Resolved live slide (None when output is black).
    pub current: Option<Slide>,
    /// Optional song credit display resolved by Rust for Output only.
    #[serde(default)]
    pub live_credit_line: Option<String>,
    /// Resolved next slide in the playlist (None when nothing queued).
    pub next: Option<Slide>,
    /// Resolved on-deck slide: the selected-but-not-live slide when there is
    /// one, otherwise the slide after the live one. Its media is preloaded by
    /// the Output so a cut to it never decodes on demand.
    pub on_deck: Option<Slide>,
    /// The project's named Looks, plus the ids each output is currently mapped
    /// to. Outputs resolve their slice of this list by id.
    pub looks: Vec<Look>,
    pub output_look_id: Option<String>,
    pub stage_look_id: Option<String>,
    /// Look id assigned to the NDI feed (None -> first look).
    pub ndi_look_id: Option<String>,
    pub default_looks: DefaultLooks,
    /// Absolute path of the optional exit/outro animation video (None = instant quit).
    pub exit_animation: Option<String>,
    /// Whether the native MIDI input listener is enabled.
    pub midi_enabled: bool,
    /// Stable id of the selected MIDI input device (None when unset).
    pub midi_device_id: Option<String>,
    /// Whether the OSC UDP listener is enabled.
    pub osc_enabled: bool,
    /// UDP port the OSC listener binds to.
    pub osc_port: u16,
    /// Trigger-to-action mappings (MIDI + OSC), persisted in settings.
    pub triggers: Vec<crate::triggers::TriggerMapping>,
    /// Whether the local-network Stage Display server is enabled.
    pub stage_network_enabled: bool,
    /// Port the Stage Display web/WebSocket server binds to.
    pub stage_network_port: u16,
    /// Targeted stage-only message (nursery alerts, countdowns, operator notes).
    /// Separate from `Project.live` — changing it never affects Output.
    pub stage_message: Option<String>,
    /// Independent overlay layer for Output — lower-third / logo on top of background+main.
    /// `None` = no overlay, `Some` with `visible=false` = hidden but content preserved.
    pub overlay: Option<Overlay>,
    /// Saved overlay library and runtime visibility, rendered in stable id order.
    #[serde(default)]
    pub overlays: Vec<Overlay>,
    /// Single-track backing audio state (rodio on cpal) — ONE track at a time, not tied to slides.
    pub audio: AudioStateView,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistItem {
    pub id: String,
    pub name: String,
    pub kind: SlideKind,
    pub slide_ids: Vec<String>,
}

pub fn ensure_item_ids(slides: &mut [Slide]) {
    let mut previous_library: Option<String> = None;
    let mut previous_id: Option<String> = None;
    for slide in slides {
        if slide.item_id.is_some() {
            previous_library = slide.library_id.clone();
            previous_id = slide.item_id.clone();
            continue;
        }
        if let Some(library_id) = slide.library_id.as_ref() {
            if previous_library.as_ref() == Some(library_id) {
                slide.item_id = previous_id.clone();
            } else {
                slide.item_id = Some(Uuid::new_v4().to_string());
            }
            previous_library = Some(library_id.clone());
        } else {
            slide.item_id = Some(Uuid::new_v4().to_string());
            previous_library = None;
        }
        previous_id = slide.item_id.clone();
    }
}

pub fn derive_items(project: &Project, library: &Library) -> Vec<PlaylistItem> {
    let mut items: Vec<PlaylistItem> = Vec::new();
    for slide in &project.slides {
        let id = slide.item_id.clone().unwrap_or_else(|| slide.id.clone());
        if let Some(item) = items.last_mut().filter(|item| item.id == id) {
            item.slide_ids.push(slide.id.clone());
            continue;
        }
        let name = slide.item_name.clone().or_else(|| slide.library_id.as_ref()
            .and_then(|song_id| library.songs.iter().find(|song| &song.id == song_id))
            .map(|song| song.title.clone())).unwrap_or_else(|| slide.display_name());
        items.push(PlaylistItem { id, name, kind: slide.kind, slide_ids: vec![slide.id.clone()] });
    }
    items
}

pub fn reorder_item_slides(slides: Vec<Slide>, item_id: &str, new_index: usize) -> Result<Vec<Slide>, String> {
    let mut groups: Vec<(String, Vec<Slide>)> = Vec::new();
    for slide in slides {
        let id = slide.item_id.clone().unwrap_or_else(|| slide.id.clone());
        if let Some((_, members)) = groups.last_mut().filter(|(last, _)| last == &id) {
            members.push(slide);
        } else { groups.push((id, vec![slide])); }
    }
    let old = groups.iter().position(|(id, _)| id == item_id)
        .ok_or_else(|| format!("playlist item {item_id} not found"))?;
    let item = groups.remove(old);
    groups.insert(new_index.min(groups.len()), item);
    Ok(groups.into_iter().flat_map(|(_, members)| members).collect())
}

/// Copy the slides in one playlist item into the item containing `target_slide_id`.
/// The source item stays intact; copies inherit the target item's name and group
/// settings while retaining their own slide content and slide-level settings.
pub fn copy_slides_into_item(
    project: &mut Project,
    source_slide_ids: &[String],
    target_slide_id: &str,
    insert_at: usize,
) -> Result<Vec<String>, String> {
    if source_slide_ids.is_empty() {
        return Err("source playlist item has no slides".to_string());
    }
    let mut seen = std::collections::HashSet::new();
    for id in source_slide_ids {
        if !seen.insert(id.as_str()) {
            return Err(format!("duplicate source slide {id}"));
        }
        if !project.slides.iter().any(|slide| slide.id == *id) {
            return Err(format!("source slide {id} not found"));
        }
    }

    let target_position = project.slides.iter().position(|slide| slide.id == target_slide_id)
        .ok_or_else(|| format!("target slide {target_slide_id} not found"))?;
    let target = &project.slides[target_position];
    let target_item_id = target.item_id.clone()
        .ok_or_else(|| "target slide is not in a playlist item".to_string())?;
    let target_item_name = target.item_name.clone().unwrap_or_else(|| target.display_name());

    let mut group_start = target_position;
    while group_start > 0 && project.slides[group_start - 1].item_id.as_deref() == Some(&target_item_id) {
        group_start -= 1;
    }
    let mut group_end = target_position + 1;
    while group_end < project.slides.len() && project.slides[group_end].item_id.as_deref() == Some(&target_item_id) {
        group_end += 1;
    }
    let insertion = insert_at.clamp(group_start, group_end);

    let mut copies = Vec::with_capacity(source_slide_ids.len());
    let mut copied_ids = Vec::with_capacity(source_slide_ids.len());
    for source_id in source_slide_ids {
        let mut copy = project.slides.iter().find(|slide| slide.id == *source_id).unwrap().clone();
        copy.id = Uuid::new_v4().to_string();
        copy.item_id = Some(target_item_id.clone());
        copy.item_name = Some(target_item_name.clone());
        copied_ids.push(copy.id.clone());
        copies.push(copy);
    }
    project.slides.splice(insertion..insertion, copies);
    project.selected = copied_ids.first().cloned();
    Ok(copied_ids)
}

pub fn remove_playlist_item(project: &mut Project, item_id: &str) -> Result<bool, String> {
    if !project.slides.iter().any(|s| s.item_id.as_deref() == Some(item_id)) {
        return Err(format!("playlist item {item_id} not found"));
    }
    let removed: std::collections::HashSet<String> = project.slides.iter()
        .filter(|s| s.item_id.as_deref() == Some(item_id)).map(|s| s.id.clone()).collect();
    let removed_live = project.live.as_ref().is_some_and(|id| removed.contains(id));
    project.slides.retain(|s| !removed.contains(&s.id));
    project.item_backgrounds.remove(item_id);
    project.item_looks.remove(item_id);
    if removed_live { project.live = None; }
    if project.selected.as_ref().is_some_and(|id| removed.contains(id)) {
        project.selected = project.slides.first().map(|s| s.id.clone());
    }
    Ok(removed_live)
}

pub fn set_item_background(project: &mut Project, item_id: &str, background: Option<Background>) -> Result<(), String> {
    if !project.slides.iter().any(|slide| slide.item_id.as_deref() == Some(item_id)) {
        return Err(format!("playlist item {item_id} not found"));
    }
    if let Some(background) = background {
        project.item_backgrounds.insert(item_id.to_string(), background);
    } else {
        project.item_backgrounds.remove(item_id);
    }
    Ok(())
}

pub fn set_kind_background(project: &mut Project, defaults: &mut DefaultLooks, kind: SlideKind, background: Option<Background>) -> String {
    let current_id = match kind {
        SlideKind::Scripture => defaults.scripture.clone(),
        SlideKind::Song => defaults.song.clone(),
        SlideKind::Generic => defaults.generic.clone(),
    };
    if let Some(id) = current_id.filter(|id| project.find_look(id).is_some()) {
        if let Some(look) = project.looks.iter_mut().find(|look| look.id == id) {
            look.background = background;
            return id;
        }
    }
    let mut look = project.looks.iter()
        .find(|look| look.name == "Main")
        .or_else(|| project.looks.first())
        .cloned()
        .unwrap_or_else(Look::main_default);
    look.id = Uuid::new_v4().to_string();
    look.name = match kind { SlideKind::Song => "Songs", SlideKind::Scripture => "Scripture", SlideKind::Generic => "Text" }.to_string();
    look.background = background;
    let id = look.id.clone();
    project.looks.push(look);
    match kind {
        SlideKind::Scripture => defaults.scripture = Some(id.clone()),
        SlideKind::Song => defaults.song = Some(id.clone()),
        SlideKind::Generic => defaults.generic = Some(id.clone()),
    }
    id
}

pub fn apply_background_to_all_items(project: &mut Project, kind: Option<SlideKind>, background: Background) -> usize {
    let ids: Vec<String> = derive_items(project, &Library::default()).into_iter()
        .filter(|item| kind.map_or(true, |kind| item.kind == kind))
        .map(|item| item.id)
        .collect();
    let count = ids.len();
    for id in ids { project.item_backgrounds.insert(id, background.clone()); }
    count
}

pub fn clear_slide_background(project: &mut Project, slide_id: &str) -> Result<(), String> {
    let slide = project.slides.iter_mut().find(|slide| slide.id == slide_id)
        .ok_or_else(|| format!("slide {slide_id} not found"))?;
    slide.background = Background::default();
    slide.background_mode = BackgroundMode::Inherit;
    Ok(())
}

pub fn item_backgrounds_in_template(project: &Project) -> HashMap<String, Background> {
    let live_ids: std::collections::HashSet<String> = project.slides.iter()
        .filter_map(|slide| slide.item_id.clone())
        .collect();
    project.item_backgrounds.iter()
        .filter(|(id, _)| live_ids.contains(*id))
        .map(|(id, background)| (id.clone(), background.clone()))
        .collect()
}

pub fn remap_template_item_backgrounds(items: &[TemplateItem], saved: &HashMap<String, Background>, loaded: &[Slide]) -> HashMap<String, Background> {
    items.iter().zip(loaded).filter_map(|(item, slide)| {
        let old_id = item.item_id.as_ref()?;
        let new_id = slide.item_id.as_ref()?;
        saved.get(old_id).map(|background| (new_id.clone(), background.clone()))
    }).collect()
}

pub fn slides_from_template(items: &[TemplateItem]) -> Vec<Slide> {
    let mut groups = HashMap::<String, String>::new();
    let mut previous_legacy: Option<(String, String)> = None;
    items.iter().map(|it| {
        let item_id = if let Some(old) = &it.item_id {
            previous_legacy = None;
            groups.entry(old.clone()).or_insert_with(|| Uuid::new_v4().to_string()).clone()
        } else if let Some(lib) = &it.library_id {
            if let Some((prev_lib, id)) = &previous_legacy {
                if prev_lib == lib { id.clone() } else {
                    let id = Uuid::new_v4().to_string(); previous_legacy = Some((lib.clone(), id.clone())); id
                }
            } else { let id = Uuid::new_v4().to_string(); previous_legacy = Some((lib.clone(), id.clone())); id }
        } else { previous_legacy = None; Uuid::new_v4().to_string() };
        Slide { id: Uuid::new_v4().to_string(), item_id: Some(item_id), item_name: it.item_name.clone(),
            library_id: it.library_id.clone(), library_slide_id: it.library_slide_id.clone(),
            name: it.name.clone().or_else(|| Some(it.title.clone())), kind: it.kind,
            title: it.title.clone(), body: it.body.clone(), background: it.background.clone(),
            background_mode: it.background_mode,
            auto_advance_secs: it.auto_advance_secs }
    }).collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlidePositioning {
    #[serde(default = "default_v_align")]
    pub v_align: String,
    #[serde(default = "default_h_align")]
    pub h_align: String,
}

fn default_v_align() -> String { "center".to_string() }
fn default_h_align() -> String { "center".to_string() }

impl Default for SlidePositioning {
    fn default() -> Self { Self { v_align: default_v_align(), h_align: default_h_align() } }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySlide {
    pub id: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub positioning: Option<SlidePositioning>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub group_label: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySong {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub copyright: Option<String>,
    #[serde(default)]
    pub ccli_number: Option<String>,
    /// Optional Output credit line. Defaults off for existing and new songs.
    #[serde(default)]
    pub show_credit_line: bool,
    pub default_background: Background,
    /// Master blocks — unique named slides keyed by block name (e.g. "Verse 1", "Chorus", "Bridge")
    #[serde(default)]
    pub blocks: HashMap<String, LibrarySlide>,
    /// Default play order — array of block keys, may repeat (e.g. ["Verse 1", "Chorus", "Verse 2", "Chorus", "Bridge", "Chorus"])
    #[serde(default)]
    pub arrangement: Vec<String>,
    /// Deprecated flat list — retained for one-time migration from v1 library.json, not serialized in new files
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slides: Option<Vec<LibrarySlide>>,
}

impl LibrarySong {
    /// One-time migration from flat `slides` (v1) to `blocks`+`arrangement` (v2).
    /// Returns true if migrated. Deduplicates by block title, preserving order via `arrangement`.
    pub fn migrate_if_needed(&mut self) -> bool {
        if !self.blocks.is_empty() || self.slides.is_none() {
            return false;
        }
        let old_slides = self.slides.take().unwrap_or_default();
        if old_slides.is_empty() {
            return false;
        }
        let mut blocks: HashMap<String, LibrarySlide> = HashMap::new();
        let mut arrangement: Vec<String> = Vec::new();
        for slide in old_slides {
            let base_key = if !slide.title.trim().is_empty() {
                slide.title.clone()
            } else if let Some(ref gl) = slide.group_label {
                if !gl.trim().is_empty() {
                    gl.clone()
                } else {
                    slide.title.clone()
                }
            } else {
                slide.title.clone()
            };
            let mut key = base_key.clone();
            if key.trim().is_empty() {
                key = format!("Verse {}", arrangement.len() + 1);
            }
            if let Some(existing) = blocks.get(&key) {
                if existing.body != slide.body || existing.title != slide.title {
                    let mut counter = 2;
                    let mut new_key = format!("{} ({})", key, counter);
                    while blocks.contains_key(&new_key) {
                        counter += 1;
                        new_key = format!("{} ({})", key, counter);
                    }
                    key = new_key;
                }
            }
            if !blocks.contains_key(&key) {
                blocks.insert(key.clone(), slide);
            }
            arrangement.push(key);
        }
        self.blocks = blocks;
        self.arrangement = arrangement;
        true
    }

    /// Flatten arrangement into ordered list of block slides (resolving each key).
    /// Falls back to blocks values if arrangement empty, or deprecated slides if present.
    pub fn flattened_slides(&self) -> Vec<&LibrarySlide> {
        if !self.arrangement.is_empty() {
            let mut out = Vec::new();
            for key in &self.arrangement {
                if let Some(block) = self.blocks.get(key) {
                    out.push(block);
                }
            }
            if out.is_empty() && !self.blocks.is_empty() {
                out.extend(self.blocks.values());
            }
            out
        } else if !self.blocks.is_empty() {
            let mut vals: Vec<&LibrarySlide> = self.blocks.values().collect();
            vals.sort_by(|a, b| a.title.cmp(&b.title));
            vals
        } else if let Some(ref slides) = self.slides {
            slides.iter().collect()
        } else {
            Vec::new()
        }
    }

}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Overlay {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub text: String,
    #[serde(default)]
    pub background: Option<Background>,
    #[serde(default)]
    pub placement: OverlayPlacement,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPlacement {
    #[default]
    LowerThird,
    Logo,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedOverlay {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub background: Option<Background>,
    #[serde(default)]
    pub placement: OverlayPlacement,
}

impl SavedOverlay {
    pub fn runtime(&self, visible: bool) -> Overlay {
        Overlay {
            id: self.id.clone(),
            name: self.name.clone(),
            text: self.text.clone(),
            background: self.background.clone(),
            placement: self.placement,
            visible,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlayStore {
    pub schema_version: u32,
    pub overlays: Vec<SavedOverlay>,
}

impl Default for OverlayStore {
    fn default() -> Self {
        Self { schema_version: 1, overlays: Vec::new() }
    }
}

#[allow(dead_code)]
impl Overlay {
    pub fn new_text(text: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "Text overlay".to_string(),
            text,
            background: None,
            placement: OverlayPlacement::LowerThird,
            visible: true,
        }
    }
    pub fn new_image(bg: Background) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "Image overlay".to_string(),
            text: String::new(),
            background: Some(bg),
            placement: OverlayPlacement::LowerThird,
            visible: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServicePresetItem {
    pub title: String,
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServicePreset {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub default_aspect: String,
    pub playlist_items: Vec<ServicePresetItem>,
}

pub fn default_presets() -> Vec<ServicePreset> {
    vec![
        ServicePreset {
            id: "sunday-morning".to_string(),
            name: "Sunday Morning Service".to_string(),
            category: "Sunday Service".to_string(),
            description: "Welcome, worship, scripture, sermon & closing — the classic Sunday flow.".to_string(),
            default_aspect: "16:9".to_string(),
            playlist_items: vec![
                ServicePresetItem { title: "Welcome".to_string(), item_type: "slide".to_string(), reference_id: None, content: Some("Welcome to Worship\nWe're glad you're here!".to_string()) },
                ServicePresetItem { title: "Worship — Amazing Grace".to_string(), item_type: "song".to_string(), reference_id: Some("amazing-grace".to_string()), content: Some("Amazing grace, how sweet the sound\nThat saved a wretch like me".to_string()) },
                ServicePresetItem { title: "Worship — Great Is Thy Faithfulness".to_string(), item_type: "song".to_string(), reference_id: Some("great-is-thy-faithfulness".to_string()), content: Some("Great is Thy faithfulness, O God my Father".to_string()) },
                ServicePresetItem { title: "Scripture Reading".to_string(), item_type: "scripture".to_string(), reference_id: Some("John 3:16".to_string()), content: Some("For God so loved the world… — John 3:16".to_string()) },
                ServicePresetItem { title: "Sermon Outline — Title".to_string(), item_type: "slide".to_string(), reference_id: None, content: Some("Today's Message\nSpeaker: Pastor\nText: John 3:16".to_string()) },
                ServicePresetItem { title: "Closing Announcement".to_string(), item_type: "slide".to_string(), reference_id: None, content: Some("Thanks for joining!\nSee you next Sunday".to_string()) },
            ],
        },
        ServicePreset {
            id: "midweek".to_string(),
            name: "Midweek Prayer & Bible Study".to_string(),
            category: "Midweek".to_string(),
            description: "Opening prayer, verse-by-verse study and prayer requests.".to_string(),
            default_aspect: "16:9".to_string(),
            playlist_items: vec![
                ServicePresetItem { title: "Opening Prayer".to_string(), item_type: "slide".to_string(), reference_id: None, content: Some("Opening Prayer\nLet us pray together".to_string()) },
                ServicePresetItem { title: "Scripture — Psalm 23:1".to_string(), item_type: "scripture".to_string(), reference_id: Some("Psalm 23:1".to_string()), content: Some("The Lord is my shepherd; I shall not want. — Psalm 23:1".to_string()) },
                ServicePresetItem { title: "Scripture — Psalm 23:2".to_string(), item_type: "scripture".to_string(), reference_id: Some("Psalm 23:2".to_string()), content: Some("He makes me lie down in green pastures. — Psalm 23:2".to_string()) },
                ServicePresetItem { title: "Scripture — Psalm 23:4".to_string(), item_type: "scripture".to_string(), reference_id: Some("Psalm 23:4".to_string()), content: Some("Even though I walk through the darkest valley… — Psalm 23:4".to_string()) },
                ServicePresetItem { title: "Prayer Requests".to_string(), item_type: "slide".to_string(), reference_id: None, content: Some("Prayer Requests\nShare your burdens".to_string()) },
                ServicePresetItem { title: "Closing Blessing".to_string(), item_type: "slide".to_string(), reference_id: None, content: Some("Go in peace — see you Sunday".to_string()) },
            ],
        },
        ServicePreset {
            id: "youth".to_string(),
            name: "Youth Event — Upbeat Service".to_string(),
            category: "Youth".to_string(),
            description: "High-energy songs, games & announcements for youth night.".to_string(),
            default_aspect: "16:9".to_string(),
            playlist_items: vec![
                ServicePresetItem { title: "Welcome — Youth Night!".to_string(), item_type: "slide".to_string(), reference_id: None, content: Some("YOUTH NIGHT\nAre you ready?".to_string()) },
                ServicePresetItem { title: "Upbeat Worship".to_string(), item_type: "song".to_string(), reference_id: Some("youth-worship".to_string()), content: Some("This is the day the Lord has made\nWe will rejoice!".to_string()) },
                ServicePresetItem { title: "Game — Ice Breaker".to_string(), item_type: "slide".to_string(), reference_id: None, content: Some("Quick Game\nTwo Truths & a Lie".to_string()) },
                ServicePresetItem { title: "Announcements".to_string(), item_type: "slide".to_string(), reference_id: None, content: Some("Upcoming Events\nRetreat — Dec 12".to_string()) },
                ServicePresetItem { title: "Message — Live Boldly".to_string(), item_type: "slide".to_string(), reference_id: None, content: Some("Live boldly for Christ\n1 Timothy 4:12".to_string()) },
            ],
        },
        ServicePreset {
            id: "blank".to_string(),
            name: "Blank / Custom Service".to_string(),
            category: "Custom".to_string(),
            description: "Empty canvas — start from scratch.".to_string(),
            default_aspect: "16:9".to_string(),
            playlist_items: vec![],
        },
    ]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Library {
    pub schema_version: u32,
    pub songs: Vec<LibrarySong>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            schema_version: LIBRARY_SCHEMA_VERSION,
            songs: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Playlist templates — reusable playlist structures (e.g. "Pre-Service Loop")
// Persisted in their own templates.json with atomic writes, mirroring
// project.json / library.json. Each TemplateItem stores slide references
// (title/body/background/library refs) not duplicated media bytes.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateItem {
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub item_name: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub kind: SlideKind,
    pub title: String,
    pub body: String,
    pub background: Background,
    #[serde(default)]
    pub background_mode: BackgroundMode,
    #[serde(default)]
    pub library_id: Option<String>,
    #[serde(default)]
    pub library_slide_id: Option<String>,
    #[serde(default)]
    pub auto_advance_secs: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistTemplate {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub items: Vec<TemplateItem>,
    #[serde(default)]
    pub item_backgrounds: HashMap<String, Background>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateStore {
    pub schema_version: u32,
    pub templates: Vec<PlaylistTemplate>,
}

impl Default for TemplateStore {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            templates: Vec::new(),
        }
    }
}

fn templates_path(data_dir: &Path) -> PathBuf {
    data_dir.join("templates.json")
}

pub fn read_templates(data_dir: &Path) -> TemplateStore {
    let raw = std::fs::read_to_string(templates_path(data_dir)).ok();
    match raw.and_then(|r| serde_json::from_str::<TemplateStore>(&r).ok()) {
        Some(store) => store,
        None => TemplateStore::default(),
    }
}

pub fn write_templates(data_dir: &Path, store: &TemplateStore) -> io::Result<()> {
    atomic_write_json(data_dir, "templates.json", store)
}

fn overlays_path(data_dir: &Path) -> PathBuf {
    data_dir.join("overlays.json")
}

pub fn read_overlays(data_dir: &Path) -> OverlayStore {
    let raw = fs::read_to_string(overlays_path(data_dir)).ok();
    match raw.and_then(|r| serde_json::from_str::<OverlayStore>(&r).ok()) {
        Some(store) => store,
        None => OverlayStore::default(),
    }
}

pub fn write_overlays(data_dir: &Path, store: &OverlayStore) -> io::Result<()> {
    atomic_write_json(data_dir, "overlays.json", store)
}

// ---------------------------------------------------------------------------
// Persisted session/settings metadata
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub project_path: Option<String>,
    pub last_saved_at: Option<String>,
    pub last_open_at: Option<String>,
    /// false until a clean exit is confirmed -> used to detect crash recovery
    pub clean_shutdown: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub output_display_index: Option<usize>,
    pub output_display_name: Option<String>,
    pub output_fullscreen: bool,
    pub stage_display_index: Option<usize>,
    pub stage_display_name: Option<String>,
    pub stage_visible: bool,
    /// Default transition applied to newly created projects.
    pub default_transition: Transition,
    /// Look id assigned to the main Output window (None -> first look).
    pub output_look_id: Option<String>,
    /// Look id assigned to the Stage Display window (None -> second/default).
    pub stage_look_id: Option<String>,
    /// Whether NDI broadcast is enabled (publishes the live slide on the LAN).
    #[serde(default)]
    pub ndi_enabled: bool,
    /// Look id assigned to the NDI feed (None -> first look).
    #[serde(default)]
    pub ndi_look_id: Option<String>,
    /// Whether the MIDI input listener is enabled.
    #[serde(default)]
    pub midi_enabled: bool,
    /// Stable port id of the selected MIDI input device (midir `MidiInputPort::id()`).
    #[serde(default)]
    pub midi_device_id: Option<String>,
    /// Whether the OSC listener is enabled.
    #[serde(default)]
    pub osc_enabled: bool,
    /// UDP port the OSC listener binds to.
    #[serde(default = "default_osc_port")]
    pub osc_port: u16,
    /// Persisted trigger-to-action mappings (MIDI + OSC).
    #[serde(default)]
    pub triggers: Vec<crate::triggers::TriggerMapping>,
    /// Whether the local-network Stage Display server is enabled. When on, a
    /// phone/tablet on the same Wi-Fi can view the live Stage Display at
    /// `http://<local-ip>:<port>/stage` after entering the PIN.
    #[serde(default)]
    pub stage_network_enabled: bool,
    /// TCP port the Stage Display web server binds to.
    #[serde(default = "default_stage_port")]
    pub stage_network_port: u16,
    /// PIN required to view the Stage Display feed. Persisted plaintext is
    /// acceptable here (local LAN, low-stakes); an empty value means "any PIN
    /// accepted" (used by tests/automation only).
    #[serde(default)]
    pub stage_network_pin: String,
    /// Stable id (cpal device name) of the selected audio output device for the single backing track.
    /// `None` = system default. Stored in Settings, independent of system default.
    #[serde(default)]
    pub audio_output_device_id: Option<String>,
    /// Volume for the backing track (0.0..1.5, 1.0 = 100%). Clamped at command layer.
    #[serde(default = "default_audio_volume")]
    pub audio_volume: f32,
    /// Default Look per slide kind — Scripture/Song/Generic, each optionally a Look id.
    /// Inheriting slides follow the mapped Look live; `None` falls back to Main.
    #[serde(default)]
    pub default_looks: DefaultLooks,
    /// Optional exit/outro animation: absolute path to a user-provided video
    /// file played full-bleed on the Output/Stage windows on real Quit.
    /// `None` (default) = instant-close behavior, unchanged.
    #[serde(default)]
    pub exit_animation: Option<String>,
}

/// Default Look mapping per slide kind — Scripture/Song/Generic.
/// Output styling follows the assigned Look live; `None` uses the Output mapping.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DefaultLooks {
    pub scripture: Option<String>,
    pub song: Option<String>,
    pub generic: Option<String>,
}

impl DefaultLooks {
    pub fn for_kind(&self, kind: SlideKind) -> Option<&str> {
        match kind { SlideKind::Song => self.song.as_deref(), SlideKind::Scripture => self.scripture.as_deref(), SlideKind::Generic => self.generic.as_deref() }
    }

    pub fn assign(&mut self, project: &Project, kind: SlideKind, id: Option<String>) -> Result<(), String> {
        if let Some(id) = &id {
            if project.find_look(id).is_none() { return Err(format!("Look {id} not found")); }
        }
        *match kind { SlideKind::Song => &mut self.song, SlideKind::Scripture => &mut self.scripture, SlideKind::Generic => &mut self.generic } = id;
        Ok(())
    }

    pub fn reassign_deleted(&mut self, deleted: &str, replacement: Option<&str>) {
        for assigned in [&mut self.song, &mut self.scripture, &mut self.generic] {
            if assigned.as_deref() == Some(deleted) { *assigned = replacement.map(str::to_string); }
        }
    }
}

/// Explicit operator action only. Existing named Looks and assignments are preserved.
pub fn create_starter_looks(project: &mut Project, defaults: &mut DefaultLooks) {
    let main = project.looks.iter().find(|look| look.name == "Main")
        .or_else(|| project.looks.first()).cloned().unwrap_or_else(Look::main_default);
    for (name, kind) in [("Songs", Some(SlideKind::Song)), ("Scripture", Some(SlideKind::Scripture)), ("Text", Some(SlideKind::Generic)), ("Title", None)] {
        let existing = project.looks.iter().find(|look| look.name.eq_ignore_ascii_case(name)).map(|look| look.id.clone());
        let id = existing.unwrap_or_else(|| {
            let mut look = main.clone();
            look.id = Uuid::new_v4().to_string();
            look.name = name.into();
            look.positioning = Positioning::Auto;
            look.text_position = TextPosition::Center;
            look.show_body = name != "Title";
            look.title_style.align = HAlign::Center;
            look.body_style.align = HAlign::Center;
            look.title_style.bold = true;
            match name {
                "Songs" => { look.title_size = 32; look.body_size = 72; look.body_style.bold = true; look.body_style.shadow_blur = 32.0; }
                "Scripture" => { look.title_size = 56; look.body_size = 44; look.body_style.bold = false; look.title_style.bg_opacity = 0.65; look.body_style.bg_opacity = 0.65; }
                "Text" => { look.title_size = 96; look.body_size = 44; }
                _ => { look.title_size = 144; }
            }
            let id = look.id.clone();
            project.looks.push(look);
            id
        });
        if let Some(kind) = kind {
            if defaults.for_kind(kind).and_then(|id| project.find_look(id)).is_none() {
                // id came from an existing or newly created Look.
                defaults.assign(project, kind, Some(id)).expect("starter Look exists");
            }
        }
    }
}

fn default_osc_port() -> u16 {
    crate::osc::DEFAULT_OSC_PORT
}

fn default_stage_port() -> u16 {
    crate::network::DEFAULT_STAGE_PORT
}

fn default_audio_volume() -> f32 {
    1.0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_display_index: None,
            output_display_name: None,
            output_fullscreen: false,
            stage_display_index: None,
            stage_display_name: None,
            stage_visible: false,
            default_transition: Transition::Cut,
            output_look_id: None,
            stage_look_id: None,
            ndi_enabled: false,
            ndi_look_id: None,
            midi_enabled: false,
            midi_device_id: None,
            osc_enabled: false,
            osc_port: 9000,
            triggers: Vec::new(),
            stage_network_enabled: false,
            stage_network_port: crate::network::DEFAULT_STAGE_PORT,
            stage_network_pin: String::new(),
            audio_output_device_id: None,
            audio_volume: default_audio_volume(),
            default_looks: Default::default(),
            exit_animation: None,
        }
    }
}

/// True only on the very first launch: no project and no settings have ever
/// been written to disk. Used to show the one-time welcome message.
pub fn is_first_run(data_dir: &Path) -> bool {
    !data_dir.join("project.json").exists() && !data_dir.join("settings.json").exists()
}

// ---------------------------------------------------------------------------
// Disk layout (all under the app data dir):
//   project.json                  - current autosaved project (atomic writes)
//   versions/<millis>.json        - versioned snapshots (capped)
//   session.json                  - recovery bookkeeping
//   settings.json                 - per-machine settings
// ---------------------------------------------------------------------------

fn current_project_path(data_dir: &Path) -> PathBuf {
    data_dir.join("project.json")
}

fn versions_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("versions")
}

pub fn read_session(data_dir: &Path) -> Option<Session> {
    let raw = fs::read_to_string(data_dir.join("session.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn write_session(data_dir: &Path, session: &Session) -> io::Result<()> {
    fs::create_dir_all(data_dir)?;
    let json = serde_json::to_string_pretty(session)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    fs::write(data_dir.join("session.json"), json)
}

pub fn read_settings(data_dir: &Path) -> Settings {
    let raw = fs::read_to_string(data_dir.join("settings.json")).ok();
    raw.and_then(|r| serde_json::from_str(&r).ok())
        .unwrap_or_default()
}

pub fn write_settings(data_dir: &Path, settings: &Settings) -> io::Result<()> {
    fs::create_dir_all(data_dir)?;
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    fs::write(data_dir.join("settings.json"), json)
}

fn atomic_write_json(data_dir: &Path, file_name: &str, value: &impl Serialize) -> io::Result<()> {
    fs::create_dir_all(data_dir)?;
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    let tmp = data_dir.join(format!("{file_name}.tmp"));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, data_dir.join(file_name))
}

/// Load the library from disk, seeding sample songs on first run.
/// Handles one-time migration from flat `slides` (v1) to master-block `blocks`+`arrangement` (v2).
/// Returns the library and whether migration occurred (for logging via AppState).
#[allow(dead_code)]
pub fn read_library(data_dir: &Path) -> Library {
    let (lib, _migrated) = read_library_with_migration_info(data_dir);
    lib
}

/// Inner helper that also returns migration count for logging.
pub fn read_library_with_migration_info(data_dir: &Path) -> (Library, usize) {
    let raw = fs::read_to_string(data_dir.join("library.json")).ok();
    match raw.and_then(|r| serde_json::from_str::<Library>(&r).ok()) {
        Some(mut library) => {
            let mut migrated = 0;
            for song in &mut library.songs {
                if song.migrate_if_needed() {
                    migrated += 1;
                }
            }
            let needs_bump = library.schema_version < LIBRARY_SCHEMA_VERSION;
            if migrated > 0 || needs_bump {
                let old_v = library.schema_version;
                library.schema_version = LIBRARY_SCHEMA_VERSION;
                eprintln!(
                    "library: migrated {} song(s) from flat slides (v{}) to blocks+arrangement (v{})",
                    migrated, old_v, LIBRARY_SCHEMA_VERSION
                );
                // Persist migrated library immediately so next launch is clean
                let _ = write_library(data_dir, &library);
            }
            (library, migrated)
        }
        None => {
            let library = seed_library();
            let _ = write_library(data_dir, &library);
            (library, 0)
        }
    }
}

pub fn write_library(data_dir: &Path, library: &Library) -> io::Result<()> {
    atomic_write_json(data_dir, "library.json", library)
}

/// A couple of sample songs so the library has content on first launch — now using master-block architecture.
fn seed_library() -> Library {
    let mut ag_blocks = HashMap::new();
    let ag_v1 = LibrarySlide {
        id: Uuid::new_v4().to_string(),
        title: "Verse 1".to_string(),
        body: "Amazing grace, how sweet the sound\nThat saved a wretch like me\nI once was lost, but now am found\nWas blind, but now I see.".to_string(),
        positioning: None,
        group_id: Some("verse-1".to_string()),
        group_label: Some("Verse 1".to_string()),
    };
    let ag_ch = LibrarySlide {
        id: Uuid::new_v4().to_string(),
        title: "Chorus".to_string(),
        body: "Was grace that taught my heart to fear\nAnd grace my fears relieved\nHow precious did that grace appear\nThe hour I first believed.".to_string(),
        positioning: None,
        group_id: Some("chorus".to_string()),
        group_label: Some("Chorus".to_string()),
    };
    ag_blocks.insert(ag_v1.title.clone(), ag_v1);
    ag_blocks.insert(ag_ch.title.clone(), ag_ch);

    let mut gf_blocks = HashMap::new();
    let gf_v1 = LibrarySlide {
        id: Uuid::new_v4().to_string(),
        title: "Verse 1".to_string(),
        body: "Great is Thy faithfulness, O God my Father\nThere is no shadow of turning with Thee\nThou changest not, Thy compassions, they fail not\nAs Thou hast been, Thou forever wilt be.".to_string(),
        positioning: None,
        group_id: Some("verse-1".to_string()),
        group_label: Some("Verse 1".to_string()),
    };
    let gf_ch = LibrarySlide {
        id: Uuid::new_v4().to_string(),
        title: "Chorus".to_string(),
        body: "Great is Thy faithfulness!\nGreat is Thy faithfulness!\nMorning by morning new mercies I see\nAll I have needed Thy hand hath provided\nGreat is Thy faithfulness, Lord, unto me.".to_string(),
        positioning: None,
        group_id: Some("chorus".to_string()),
        group_label: Some("Chorus".to_string()),
    };
    gf_blocks.insert(gf_v1.title.clone(), gf_v1);
    gf_blocks.insert(gf_ch.title.clone(), gf_ch);

    Library {
        schema_version: LIBRARY_SCHEMA_VERSION,
        songs: vec![
            LibrarySong {
                id: Uuid::new_v4().to_string(),
                title: "Amazing Grace".to_string(),
                author: None,
                copyright: None,
                ccli_number: None,
                show_credit_line: false,
                default_background: Background::Solid {
                    color: "#1f3a2f".to_string(),
                },
                blocks: ag_blocks,
                arrangement: vec!["Verse 1".to_string(), "Chorus".to_string()],
                slides: None,
            },
            LibrarySong {
                id: Uuid::new_v4().to_string(),
                title: "Great Is Thy Faithfulness".to_string(),
                author: None,
                copyright: None,
                ccli_number: None,
                show_credit_line: false,
                default_background: Background::Solid {
                    color: "#0f2b4a".to_string(),
                },
                blocks: gf_blocks,
                arrangement: vec!["Verse 1".to_string(), "Chorus".to_string()],
                slides: None,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_assignments_follow_resolution_order_and_unassign_to_output() {
        let mut project = Project::new("Kinds");
        let main = project.looks[0].id.clone();
        let mut defaults = DefaultLooks::default();
        create_starter_looks(&mut project, &mut defaults);
        let title = project.looks.iter().find(|look| look.name == "Title").unwrap().id.clone();
        let output = project.looks.iter().find(|look| look.name == "Stage").unwrap().id.clone();
        for kind in [SlideKind::Song, SlideKind::Scripture, SlideKind::Generic] {
            let mut slide = test_slide("slide", Some("item"), None);
            slide.kind = kind;
            slide.background_mode = BackgroundMode::Custom;
            let assigned = defaults.for_kind(kind).unwrap().to_string();
            assert_eq!(project.effective_output_look_id(&slide, &defaults, Some(&output)), Some(assigned.clone()));
            // A custom background changes only the background, never typography.
            assert_eq!(project.effective_background(&slide, &defaults), slide.background);
            project.item_looks.insert("item".into(), title.clone());
            assert_eq!(project.effective_output_look_id(&slide, &defaults, Some(&output)), Some(title.clone()));
            project.item_looks.clear();
            defaults.assign(&project, kind, Some(title.clone())).unwrap();
            assert_eq!(defaults.for_kind(kind), Some(title.as_str()));
            assert_eq!(project.effective_output_look_id(&slide, &defaults, Some(&output)), Some(title.clone()));
            defaults.assign(&project, kind, None).unwrap();
            assert_eq!(project.effective_output_look_id(&slide, &defaults, Some(&output)), Some(output.clone()));
            assert_eq!(project.effective_output_look_id(&slide, &defaults, None), Some(main.clone()));
            assert!(defaults.assign(&project, kind, Some("missing".into())).is_err());
            assert_eq!(defaults.for_kind(kind), None);
        }
    }

    #[test]
    fn starter_looks_are_explicit_idempotent_and_keep_existing_names_and_mappings() {
        let mut project = Project::new("Starters");
        assert_eq!(project.looks.len(), 2); // New projects never create starters.
        let main = project.looks[0].clone();
        let mut defaults = DefaultLooks::default();
        create_starter_looks(&mut project, &mut defaults);
        assert_eq!(project.looks.len(), 6);
        assert_eq!(project.looks[0], main);
        let songs = project.find_look(defaults.song.as_deref().unwrap()).unwrap();
        assert!(songs.body_style.bold && songs.body_size > songs.title_size);
        let scripture = project.find_look(defaults.scripture.as_deref().unwrap()).unwrap();
        assert!(scripture.title_style.bold && scripture.title_style.bg_opacity > 0.0);
        let title = project.looks.iter().find(|look| look.name == "Title").unwrap();
        assert!(!title.show_body);
        let roundtrip: Project = serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
        assert_eq!(roundtrip.looks, project.looks);
        assert!(![defaults.song.as_deref(), defaults.scripture.as_deref(), defaults.generic.as_deref()].contains(&Some(title.id.as_str())));
        let before = project.looks.clone();
        let assignments = defaults.clone();
        create_starter_looks(&mut project, &mut defaults);
        assert_eq!(project.looks, before);
        assert_eq!(defaults, assignments);
        // Existing custom names/styles are reused and never overwritten.
        let existing = project.looks.iter_mut().find(|look| look.name == "Songs").unwrap();
        existing.body_size = 123;
        existing.name = "songs".into();
        defaults.song = None;
        defaults.generic = Some(main.id.clone());
        let before = project.looks.clone();
        create_starter_looks(&mut project, &mut defaults);
        assert_eq!(project.looks, before);
        assert_eq!(defaults.song, assignments.song);
        assert_eq!(defaults.generic, Some(main.id));
    }

    #[test]
    fn deleting_kind_look_reassigns_to_valid_replacement_for_every_kind() {
        let mut project = Project::new("Delete mapped Look");
        let mut defaults = DefaultLooks::default();
        create_starter_looks(&mut project, &mut defaults);
        let main = project.looks[0].id.clone();
        let deleted = defaults.song.clone().unwrap();
        for kind in [SlideKind::Song, SlideKind::Scripture, SlideKind::Generic] {
            defaults.assign(&project, kind, Some(deleted.clone())).unwrap();
        }
        let fallback = project.remove_look_and_reassign(&deleted, Some(&main)).unwrap();
        defaults.reassign_deleted(&deleted, fallback.as_deref());
        for kind in [SlideKind::Song, SlideKind::Scripture, SlideKind::Generic] {
            assert_eq!(defaults.for_kind(kind), Some(main.as_str()));
        }
    }

    #[test]
    fn legacy_projects_and_settings_keep_looks_and_have_no_kind_assignments() {
        let project = Project::new("Legacy");
        let mut raw = serde_json::to_value(&project).unwrap();
        for look in raw["looks"].as_array_mut().unwrap() { look.as_object_mut().unwrap().remove("showBody"); }
        let legacy: Project = serde_json::from_value(raw).unwrap();
        assert_eq!(legacy.looks, project.looks);
        let mut raw_settings = serde_json::to_value(Settings::default()).unwrap();
        raw_settings.as_object_mut().unwrap().remove("defaultLooks");
        let settings: Settings = serde_json::from_value(raw_settings).unwrap();
        assert_eq!(settings.default_looks, DefaultLooks::default());
        for kind in [SlideKind::Song, SlideKind::Scripture, SlideKind::Generic] {
            let mut slide = test_slide("legacy", Some("item"), None);
            slide.kind = kind;
            assert_eq!(legacy.effective_background(&slide, &settings.default_looks), slide.background);
            assert_eq!(legacy.effective_output_look_id(&slide, &settings.default_looks, Some(&legacy.looks[1].id)), Some(legacy.looks[1].id.clone()));
        }
    }

    #[test]
    fn legacy_look_without_alignment_loads_with_unchanged_centered_styles() {
        let project = Project::new("Legacy alignment");
        let mut raw = serde_json::to_value(&project).unwrap();
        for look in raw["looks"].as_array_mut().unwrap() {
            for role in ["titleStyle", "bodyStyle"] {
                look[role].as_object_mut().unwrap().remove("align");
            }
        }
        let data_dir = std::env::temp_dir().join(format!("makrstudio-legacy-align-{}", Uuid::new_v4()));
        fs::create_dir_all(&data_dir).unwrap();
        let source = serde_json::to_string(&raw).unwrap();
        fs::write(current_project_path(&data_dir), &source).unwrap();
        let restored = recover_or_seed(&data_dir).0;
        assert_eq!(restored.looks, project.looks);
        assert_eq!(fs::read_to_string(current_project_path(&data_dir)).unwrap(), source);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn all_project_creation_and_load_paths_restore_main_and_stage_looks() {
        let assert_defaults = |project: &Project| {
            for name in ["Main", "Stage"] {
                assert!(project.looks.iter().any(|look| look.name == name), "missing {name}");
            }
            assert_ne!(project.looks[0].id, project.looks[1].id);
        };
        // These are the constructors used by new_project and new_project_from_preset.
        assert_defaults(&Project::new("New service"));
        for preset in default_presets() {
            assert_defaults(&Project::from_preset("New preset", "16:9", Transition::Cut, &preset));
        }
        let root = std::env::temp_dir().join(format!("makrstudio-look-loads-{}", Uuid::new_v4()));
        assert_defaults(&recover_or_seed(&root.join("new-install")).0);
        for empty_array in [false, true] {
            let mut legacy = serde_json::to_value(Project::new("Legacy service")).unwrap();
            if empty_array { legacy["looks"] = serde_json::json!([]); }
            else { legacy.as_object_mut().unwrap().remove("looks"); }
            let raw = serde_json::to_string(&legacy).unwrap();
            for source in ["session", "autosave", "snapshot"] {
                let data_dir = root.join(format!("{source}-{empty_array}"));
                fs::create_dir_all(versions_dir(&data_dir)).unwrap();
                let path = match source {
                    "session" => data_dir.join("service.json"),
                    "snapshot" => versions_dir(&data_dir).join("2026-10-06.json"),
                    _ => current_project_path(&data_dir),
                };
                fs::write(&path, &raw).unwrap();
                if source == "session" {
                    write_session(&data_dir, &Session {
                        project_path: Some(path.to_string_lossy().into_owned()),
                        last_saved_at: None, last_open_at: None, clean_shutdown: true,
                    }).unwrap();
                }
                let loaded = recover_or_seed(&data_dir).0;
                assert_defaults(&loaded);
                assert_eq!(loaded.name, "Legacy service");
                assert_eq!(loaded.slides[0].body, legacy["slides"][0]["body"].as_str().unwrap());
                // Migration repairs memory without rewriting the source project.
                assert_eq!(fs::read_to_string(path).unwrap(), raw);
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn overlay_store_round_trips_through_atomic_file() {
        let data_dir = std::env::temp_dir().join(format!("makrstudio-overlays-{}", Uuid::new_v4()));
        let store = OverlayStore {
            schema_version: 1,
            overlays: vec![SavedOverlay {
                id: "welcome".into(),
                name: "Welcome".into(),
                text: "Welcome to worship".into(),
                background: None,
                placement: OverlayPlacement::LowerThird,
            }],
        };

        write_overlays(&data_dir, &store).unwrap();
        assert_eq!(read_overlays(&data_dir), store);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn old_runtime_overlay_defaults_new_fields() {
        let overlay: Overlay = serde_json::from_str(
            r#"{"id":"old","text":"Hello","visible":true}"#,
        )
        .unwrap();
        assert_eq!(overlay.name, "");
        assert_eq!(overlay.placement, OverlayPlacement::LowerThird);
        assert_eq!(overlay.background, None);
    }

    #[test]
    fn legacy_library_song_defaults_credit_details_off() {
        let song: LibrarySong = serde_json::from_str(
            r##"{"id":"old-song","title":"Old Song","defaultBackground":{"type":"solid","color":"#000000"},"blocks":{},"arrangement":[]}"##,
        )
        .unwrap();
        assert_eq!(song.author, None);
        assert_eq!(song.copyright, None);
        assert_eq!(song.ccli_number, None);
        assert!(!song.show_credit_line);
    }

    fn test_slide(id: &str, item_id: Option<&str>, library_id: Option<&str>) -> Slide {
        Slide {
            id: id.to_string(),
            item_id: item_id.map(str::to_string),
            item_name: None,
            library_id: library_id.map(str::to_string),
            library_slide_id: None,
            name: Some(id.to_string()),
            kind: if library_id.is_some() { SlideKind::Song } else { SlideKind::Generic },
            title: id.to_string(),
            body: String::new(),
            background: Background::Solid { color: "#000000".into() },
            background_mode: BackgroundMode::Custom,
            auto_advance_secs: None,
        }
    }

    #[test]
    fn legacy_project_groups_consecutive_song_slides_in_memory() {
        let mut project = Project::test();
        project.slides = vec![
            test_slide("a1", None, Some("song-a")),
            test_slide("a2", None, Some("song-a")),
            test_slide("b1", None, Some("song-b")),
            test_slide("custom", None, None),
        ];
        let mut json = serde_json::to_value(&project).unwrap();
        for slide in json["slides"].as_array_mut().unwrap() {
            slide.as_object_mut().unwrap().remove("itemId");
            slide.as_object_mut().unwrap().remove("itemName");
        }
        let mut loaded: Project = serde_json::from_value(json).unwrap();
        ensure_item_ids(&mut loaded.slides);
        let ids: Vec<_> = loaded.slides.iter().map(|slide| slide.item_id.clone().unwrap()).collect();
        assert_eq!(ids[0], ids[1]);
        assert_ne!(ids[1], ids[2]);
        assert_ne!(ids[2], ids[3]);
        let library = Library::default();
        let items = derive_items(&loaded, &library);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].slide_ids, vec!["a1", "a2"]);
    }

    #[test]
    fn item_identity_keeps_repeated_songs_separate_and_reorder_contiguous() {
        let slides = vec![
            test_slide("a1", Some("first-song"), Some("song")),
            test_slide("a2", Some("first-song"), Some("song")),
            test_slide("b1", Some("middle"), None),
            test_slide("a3", Some("second-song"), Some("song")),
            test_slide("a4", Some("second-song"), Some("song")),
        ];
        let reordered = reorder_item_slides(slides, "second-song", 0).unwrap();
        let ids: Vec<_> = reordered.iter().map(|slide| slide.id.as_str()).collect();
        assert_eq!(ids, vec!["a3", "a4", "a1", "a2", "b1"]);
        assert_eq!(reordered[0].item_id, reordered[1].item_id);
    }

    #[test]
    fn copied_playlist_item_slides_join_target_group_and_keep_source() {
        let mut project = Project::test();
        let mut source_one = test_slide("source-one", Some("source"), Some("song"));
        source_one.title = "Verse 1".into();
        source_one.body = "Copied lyrics".into();
        source_one.name = Some("Verse One".into());
        source_one.library_slide_id = Some("library-verse".into());
        source_one.auto_advance_secs = Some(12);
        let source_two = test_slide("source-two", Some("source"), Some("song"));
        let target_one = test_slide("target-one", Some("target"), None);
        let target_two = test_slide("target-two", Some("target"), None);
        let after = test_slide("after", Some("after"), None);
        project.slides = vec![source_one, source_two, target_one, target_two, after];
        project.slides[2].item_name = Some("Target group".into());

        let copied = copy_slides_into_item(
            &mut project,
            &["source-one".into(), "source-two".into()],
            "target-one",
            3,
        ).unwrap();

        assert_eq!(project.slides.iter().map(|slide| slide.id.as_str()).collect::<Vec<_>>(), vec![
            "source-one", "source-two", "target-one", copied[0].as_str(), copied[1].as_str(), "target-two", "after",
        ]);
        assert_eq!(project.slides.iter().filter(|slide| slide.item_id.as_deref() == Some("source")).count(), 2);
        let first_copy = project.slides.iter().find(|slide| slide.id == copied[0]).unwrap();
        assert_eq!(first_copy.item_id.as_deref(), Some("target"));
        assert_eq!(first_copy.item_name.as_deref(), Some("Target group"));
        assert_eq!(first_copy.title, "Verse 1");
        assert_eq!(first_copy.body, "Copied lyrics");
        assert_eq!(first_copy.library_slide_id.as_deref(), Some("library-verse"));
        assert_eq!(first_copy.auto_advance_secs, Some(12));
        assert_ne!(first_copy.id, "source-one");
        assert_eq!(project.selected.as_deref(), Some(copied[0].as_str()));
    }

    #[test]
    fn copying_playlist_slides_rejects_missing_sources_before_mutating() {
        let mut project = Project::test();
        project.slides = vec![test_slide("target", Some("target"), None)];
        let before = project.slides.clone();
        let result = copy_slides_into_item(&mut project, &["missing".into()], "target", 1);
        assert!(result.is_err());
        assert_eq!(project.slides.len(), before.len());
        assert_eq!(project.slides[0].id, before[0].id);
    }

    #[test]
    fn deleting_live_item_clears_live_slide() {
        let mut project = Project::test();
        project.slides = vec![test_slide("one", Some("item"), None), test_slide("two", Some("item"), None)];
        project.live = Some("two".into());
        assert!(remove_playlist_item(&mut project, "item").unwrap());
        assert!(project.live.is_none());
        assert!(project.slides.is_empty());
    }

    #[test]
    fn template_load_assigns_fresh_ids_and_preserves_grouping() {
        let mut project = Project::test();
        project.slides = vec![test_slide("one", Some("song-item"), Some("song")), test_slide("two", Some("song-item"), Some("song")), test_slide("custom", Some("custom-item"), None)];
        let template_items: Vec<TemplateItem> = project.slides.iter().map(|slide| TemplateItem {
            item_id: slide.item_id.clone(), item_name: Some("Saved item".into()),
            name: slide.name.clone(), kind: slide.kind, title: slide.title.clone(), body: slide.body.clone(),
            background: slide.background.clone(), background_mode: slide.background_mode, library_id: slide.library_id.clone(),
            library_slide_id: slide.library_slide_id.clone(), auto_advance_secs: slide.auto_advance_secs,
        }).collect();
        let loaded = slides_from_template(&template_items);
        assert_eq!(loaded[0].item_id, loaded[1].item_id);
        assert_ne!(loaded[1].item_id, loaded[2].item_id);
        assert_ne!(loaded[0].item_id, project.slides[0].item_id);
    }

    #[test]
    fn old_template_item_without_item_fields_still_loads() {
        let old = r##"{"title":"Legacy","body":"Text","background":{"type":"solid","color":"#000000"}}"##;
        let item: TemplateItem = serde_json::from_str(old).unwrap();
        assert!(item.item_id.is_none());
        assert!(item.item_name.is_none());
        assert_eq!(item.background_mode, BackgroundMode::Custom);
        let legacy_item: serde_json::Value = serde_json::from_str(old).unwrap();
        let legacy_template = serde_json::json!({
            "id": "legacy", "name": "Legacy", "createdAt": "now", "items": [legacy_item]
        });
        let template: PlaylistTemplate = serde_json::from_value(legacy_template).unwrap();
        assert!(template.item_backgrounds.is_empty());
    }

    #[test]
    fn output_look_resolution_uses_item_then_kind_then_output_and_legacy_mapping() {
        let mut project = Project::test();
        let main = project.looks.iter().find(|look| look.name == "Main").unwrap().id.clone();
        let ids = ["item-look", "kind-look", "output-look"].map(str::to_string);
        for id in &ids {
            let mut look = Look::main_default();
            look.id = id.clone();
            look.name = id.clone();
            project.looks.push(look);
        }
        let mut slide = test_slide("song", Some("item-a"), Some("song-a"));
        slide.kind = SlideKind::Song;
        let mut defaults = DefaultLooks::default();
        defaults.song = Some(ids[1].clone());
        project.item_looks.insert("item-a".into(), ids[0].clone());
        assert_eq!(project.effective_output_look_id(&slide, &defaults, Some(&ids[2])), Some(ids[0].clone()));
        project.item_looks.clear();
        assert_eq!(project.effective_output_look_id(&slide, &defaults, Some(&ids[2])), Some(ids[1].clone()));
        defaults.song = None;
        assert_eq!(project.effective_output_look_id(&slide, &defaults, Some(&ids[2])), Some(ids[2].clone()));
        assert_eq!(project.effective_output_look_id(&slide, &defaults, None), Some(main));
    }

    #[test]
    fn legacy_look_text_style_defaults_keep_existing_renderer_settings() {
        let mut look = Look::main_default();
        let mut json = serde_json::to_value(&look).unwrap();
        for role in ["titleStyle", "bodyStyle"] {
            let style = json[role].as_object_mut().unwrap();
            for key in ["bold", "italic", "allCaps", "shrinkToFit", "minSize"] { style.remove(key); }
        }
        look = serde_json::from_value(json).unwrap();
        assert!(!look.title_style.bold && !look.title_style.italic && !look.title_style.all_caps);
        assert!(look.title_style.shrink_to_fit);
        assert_eq!(look.title_style.min_size, None);
        assert_eq!(look.body_style.min_size, None);
    }

    #[test]
    fn deleting_a_mapped_look_reassigns_item_override_safely() {
        let mut project = Project::test();
        let main = project.looks.iter().find(|look| look.name == "Main").unwrap().id.clone();
        let mut alternate = Look::main_default();
        alternate.id = "alternate".into();
        project.looks.push(alternate);
        project.item_looks.insert("item".into(), "alternate".into());
        assert_eq!(project.remove_look_and_reassign("alternate", Some(&main)).unwrap(), Some(main.clone()));
        assert_eq!(project.item_looks.get("item"), Some(&main));
        assert!(project.remove_look_and_reassign(&main, Some(&main)).is_err());
    }

    #[test]
    fn look_json_export_import_roundtrip_keeps_all_fields() {
        let mut look = Look::main_default();
        look.title_style.bold = true;
        look.title_style.min_size = Some(31);
        look.background = Some(Background::Image { path: "/cache/hash.png".into(), hash: "hash".into(), thumb: "/cache/hash.jpg".into() });
        let encoded = serde_json::to_string(&look).unwrap();
        let decoded: Look = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, look);
    }

    #[test]
    fn effective_background_uses_custom_then_item_then_kind_look_precedence() {
        let mut project = Project::test();
        project.slides = vec![test_slide("custom", Some("item"), None), test_slide("inherited", Some("item"), None)];
        let mut defaults = DefaultLooks::default();
        let mut song_look = Look::main_default();
        song_look.id = "song-look".into();
        song_look.background = Some(Background::Solid { color: "#112233".into() });
        project.looks.push(song_look);
        defaults.generic = Some("song-look".into());
        project.item_backgrounds.insert("item".into(), Background::Solid { color: "#445566".into() });
        project.slides[1].background_mode = BackgroundMode::Inherit;
        assert_eq!(project.effective_background(&project.slides[0], &defaults), Background::Solid { color: "#000000".into() });
        assert_eq!(project.effective_background(&project.slides[1], &defaults), Background::Solid { color: "#445566".into() });
        project.item_backgrounds.clear();
        assert_eq!(project.effective_background(&project.slides[1], &defaults), Background::Solid { color: "#112233".into() });
    }

    #[test]
    fn legacy_project_backgrounds_remain_custom_and_render_unchanged() {
        let mut project = Project::test();
        project.slides[0].background = Background::Solid { color: "#765432".into() };
        let expected = project.slides[0].background.clone();
        let mut json = serde_json::to_value(&project).unwrap();
        json.as_object_mut().unwrap().remove("itemBackgrounds");
        json["slides"][0].as_object_mut().unwrap().remove("backgroundMode");
        let loaded: Project = serde_json::from_value(json).unwrap();
        assert_eq!(loaded.slides[0].background_mode, BackgroundMode::Custom);
        assert_eq!(loaded.effective_background(&loaded.slides[0], &DefaultLooks::default()), expected);
    }

    #[test]
    fn template_roundtrip_preserves_item_backgrounds_with_fresh_item_ids() {
        let mut project = Project::test();
        project.slides = vec![test_slide("one", Some("old-item"), None), test_slide("two", Some("old-item"), None)];
        let saved_background = Background::Solid { color: "#aabbcc".into() };
        project.item_backgrounds.insert("old-item".into(), saved_background.clone());
        let template_items: Vec<TemplateItem> = project.slides.iter().map(|slide| TemplateItem {
            item_id: slide.item_id.clone(), item_name: Some("Saved".into()), name: slide.name.clone(), kind: slide.kind,
            title: slide.title.clone(), body: slide.body.clone(), background: slide.background.clone(),
            background_mode: slide.background_mode, library_id: slide.library_id.clone(),
            library_slide_id: slide.library_slide_id.clone(), auto_advance_secs: slide.auto_advance_secs,
        }).collect();
        let template = PlaylistTemplate { id: "t".into(), name: "T".into(), created_at: now_iso(), items: template_items.clone(), item_backgrounds: item_backgrounds_in_template(&project) };
        let roundtrip: PlaylistTemplate = serde_json::from_str(&serde_json::to_string(&template).unwrap()).unwrap();
        let loaded = slides_from_template(&roundtrip.items);
        let backgrounds = remap_template_item_backgrounds(&roundtrip.items, &roundtrip.item_backgrounds, &loaded);
        let new_id = loaded[0].item_id.as_ref().unwrap();
        assert_ne!(new_id, "old-item");
        assert_eq!(loaded[0].item_id, loaded[1].item_id);
        assert_eq!(backgrounds.get(new_id), Some(&saved_background));
    }

    #[test]
    fn deleting_item_cleans_its_background_and_clear_slide_inherits() {
        let mut project = Project::test();
        project.slides = vec![test_slide("one", Some("item"), None), test_slide("two", Some("other"), None)];
        project.item_backgrounds.insert("item".into(), Background::Solid { color: "#123456".into() });
        remove_playlist_item(&mut project, "item").unwrap();
        assert!(!project.item_backgrounds.contains_key("item"));
        project.slides[0].background_mode = BackgroundMode::Custom;
        clear_slide_background(&mut project, "two").unwrap();
        assert_eq!(project.slides[0].background_mode, BackgroundMode::Inherit);
    }

    #[test]
    fn setting_kind_background_creates_a_kind_look_when_missing() {
        let mut project = Project::test();
        let mut defaults = DefaultLooks::default();
        let color = Some(Background::Solid { color: "#334455".into() });
        let id = set_kind_background(&mut project, &mut defaults, SlideKind::Song, color.clone());
        assert_eq!(defaults.song.as_deref(), Some(id.as_str()));
        assert_eq!(project.find_look(&id).unwrap().background, color);
        assert_eq!(project.find_look(&id).unwrap().name, "Songs");
    }

    #[test]
    fn bulk_background_targets_only_requested_kind_without_changing_live_slide() {
        let mut project = Project::test();
        project.slides = vec![test_slide("song", Some("song-item"), Some("s")), test_slide("text", Some("text-item"), None)];
        project.live = Some("song".into());
        let background = Background::Solid { color: "#223344".into() };
        project.slides[0].background_mode = BackgroundMode::Inherit;
        assert_eq!(apply_background_to_all_items(&mut project, Some(SlideKind::Song), background.clone()), 1);
        assert_eq!(project.item_backgrounds.get("song-item"), Some(&background));
        assert!(!project.item_backgrounds.contains_key("text-item"));
        let effective_before = project.effective_background(&project.slides[0], &DefaultLooks::default());
        assert_eq!(project.live.as_deref(), Some("song"));
        assert_eq!(apply_background_to_all_items(&mut project, Some(SlideKind::Song), background.clone()), 1);
        assert_eq!(project.effective_background(&project.slides[0], &DefaultLooks::default()), effective_before);
        assert_eq!(project.live.as_deref(), Some("song"));
    }

    #[test]
    fn preset_creation_preserves_slide_kinds_and_layout_choices() {
        let preset = default_presets()
            .into_iter()
            .find(|preset| preset.id == "sunday-morning")
            .unwrap();
        let project = Project::from_preset("Sunday", "4:3", Transition::Fade, &preset);

        assert_eq!(project.aspect_ratio, "4:3");
        assert_eq!(project.transition, Transition::Fade);
        assert_eq!(project.slides[0].kind, SlideKind::Generic);
        assert_eq!(project.slides[1].kind, SlideKind::Song);
        assert_eq!(project.slides[3].kind, SlideKind::Scripture);
    }

    #[test]
    fn all_transition_styles_roundtrip_with_stable_names() {
        for (transition, expected) in [
            (Transition::Cut, "cut"),
            (Transition::Fade, "fade"),
            (Transition::Wipe, "wipe"),
            (Transition::Push, "push"),
        ] {
            let encoded = serde_json::to_string(&transition).unwrap();
            assert_eq!(encoded, format!("\"{expected}\""));
            assert_eq!(serde_json::from_str::<Transition>(&encoded).unwrap(), transition);
        }
    }

    #[test]
    fn library_migration_preserves_amazing_grace() {
        let old_json = r##"{
            "schemaVersion": 1,
            "songs": [
                {
                    "id": "test-id-1",
                    "title": "Amazing Grace",
                    "defaultBackground": {"type": "solid", "color": "#1f3a2f"},
                    "slides": [
                        {"id": "s1", "title": "Verse 1", "body": "Amazing grace, how sweet the sound\nThat saved a wretch like me", "groupId": "verse-1", "groupLabel": "Verse 1"},
                        {"id": "s2", "title": "Chorus", "body": "Was grace that taught my heart to fear", "groupId": "chorus", "groupLabel": "Chorus"}
                    ]
                },
                {
                    "id": "test-id-2",
                    "title": "Great Is Thy Faithfulness",
                    "defaultBackground": {"type": "solid", "color": "#0f2b4a"},
                    "slides": [
                        {"id": "s3", "title": "Verse 1", "body": "Great is Thy faithfulness", "groupId": "verse-1", "groupLabel": "Verse 1"},
                        {"id": "s4", "title": "Chorus", "body": "Great is Thy faithfulness! Great is Thy faithfulness!", "groupId": "chorus", "groupLabel": "Chorus"}
                    ]
                }
            ]
        }"##;
        let mut lib: Library = serde_json::from_str(old_json).unwrap();
        assert_eq!(lib.songs[0].slides.as_ref().unwrap().len(), 2);
        assert!(lib.songs[0].blocks.is_empty());
        let migrated0 = lib.songs[0].migrate_if_needed();
        let migrated1 = lib.songs[1].migrate_if_needed();
        assert!(migrated0);
        assert!(migrated1);
        assert_eq!(lib.songs[0].blocks.len(), 2);
        assert_eq!(lib.songs[0].arrangement, vec!["Verse 1".to_string(), "Chorus".to_string()]);
        assert!(lib.songs[0].slides.is_none());
        let flat0 = lib.songs[0].flattened_slides();
        assert_eq!(flat0.len(), 2);
        assert_eq!(flat0[0].title, "Verse 1");
        assert_eq!(flat0[1].title, "Chorus");
        let flat1 = lib.songs[1].flattened_slides();
        assert_eq!(flat1.len(), 2);
        assert_eq!(lib.songs[1].arrangement, vec!["Verse 1".to_string(), "Chorus".to_string()]);
    }

    #[test]
    fn seed_library_has_blocks_and_arrangement() {
        let lib = seed_library();
        assert_eq!(lib.schema_version, LIBRARY_SCHEMA_VERSION);
        for song in &lib.songs {
            assert!(!song.blocks.is_empty(), "song {} should have blocks", song.title);
            assert!(!song.arrangement.is_empty(), "song {} should have arrangement", song.title);
            assert!(song.slides.is_none(), "new seed should not have deprecated slides");
            for key in &song.arrangement {
                assert!(song.blocks.contains_key(key), "missing block {} in {}", key, song.title);
            }
        }
        let ag = lib.songs.iter().find(|s| s.title == "Amazing Grace").unwrap();
        assert_eq!(ag.arrangement, vec!["Verse 1".to_string(), "Chorus".to_string()]);
        assert_eq!(ag.blocks.len(), 2);
        let flat = ag.flattened_slides();
        assert_eq!(flat.len(), 2);
    }

    #[test]
    fn arrangement_duplicate_preserves_repeats() {
        let mut song = seed_library().songs.into_iter().find(|s| s.title == "Amazing Grace").unwrap();
        // Add an extra Chorus via arrangement duplicate
        let mut new_arr = song.arrangement.clone();
        new_arr.push("Chorus".to_string());
        song.arrangement = new_arr.clone();
        let flat = song.flattened_slides();
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].title, "Verse 1");
        assert_eq!(flat[1].title, "Chorus");
        assert_eq!(flat[2].title, "Chorus");
    }
}

/// Atomically persist the project (temp file + rename) and keep a versioned
/// snapshot of the previous state. Also refreshes the session file.
pub fn persist(project: &Project, data_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(data_dir)?;

    let current = current_project_path(data_dir);
    if current.exists() {
        let versions = versions_dir(data_dir);
        fs::create_dir_all(&versions)?;
        let stamp = chrono::Utc::now().timestamp_millis();
        let _ = fs::copy(&current, versions.join(format!("{stamp:020}.json")));
        cull_snapshots(&versions, MAX_SNAPSHOTS);
    }

    let json = serde_json::to_string_pretty(project)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    let tmp = data_dir.join("project.json.tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, &current)?;

    let session = Session {
        project_path: Some(current.to_string_lossy().to_string()),
        last_saved_at: Some(now_iso()),
        ..read_session(data_dir).unwrap_or_default()
    };
    write_session(data_dir, &session)
}

fn newest_snapshot(dir: &Path) -> Option<PathBuf> {
    let mut names: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    names.sort();
    names.last().cloned()
}

fn cull_snapshots(versions: &Path, max: usize) {
    let mut names: Vec<PathBuf> = fs::read_dir(versions)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    names.sort();
    let excess = names.len().saturating_sub(max);
    for path in names.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

fn load_from(path: &Path) -> Option<Project> {
    let raw = fs::read_to_string(path).ok()?;
    let mut project: Project = serde_json::from_str(&raw).ok()?;
    if project.schema_version != SCHEMA_VERSION {
        eprintln!(
            "project {} uses schema v{}, expected v{SCHEMA_VERSION}",
            path.display(),
            project.schema_version
        );
        return None;
    }
    project.ensure_default_looks();
    Some(project)
}

/// Load the saved project at startup. Prefers the session's project file, then
/// the current autosave, then the newest snapshot. Never silently drops data:
/// if everything is missing, seeds the Phase 1 test project.
pub fn recover_or_seed(data_dir: &Path) -> (Project, Option<Notice>) {
    let session = read_session(data_dir);
    let recovering = session.as_ref().is_some_and(|s| !s.clean_shutdown);
    let saved_at = session.as_ref().and_then(|s| s.last_saved_at.clone());

    let mut project = None;
    let mut loaded_from_snapshot = false;
    if let Some(path) = session.as_ref().and_then(|s| s.project_path.clone()) {
        project = load_from(&PathBuf::from(path));
    }
    let current = current_project_path(data_dir);
    if project.is_none() && current.exists() {
        project = load_from(&current);
    }
    if project.is_none() {
        if let Some(snapshot) = newest_snapshot(&versions_dir(data_dir)) {
            project = load_from(&snapshot);
            loaded_from_snapshot = true;
        }
    }
    let mut project = project.unwrap_or_else(|| {
        eprintln!("no usable project file found, seeding new project");
        Project::test()
    });
    project.ensure_default_looks();
    ensure_item_ids(&mut project.slides);

    let recovered = recovering || loaded_from_snapshot;
    let notice = if recovered {
        Some(Notice {
            kind: "recovered".to_string(),
            message: format!(
                "Recovered project \"{}\" from the last autosave.",
                project.name
            ),
at: saved_at.clone(),
        })
    } else {
        None
    };

    // Start of a session: mark as unclean until the app exits cleanly.
    let _ = write_session(
        data_dir,
        &Session {
            project_path: Some(current.to_string_lossy().to_string()),
            last_saved_at: saved_at,
            last_open_at: Some(now_iso()),
            clean_shutdown: false,
        },
    );

    (project, notice)
}

// ---------------------------------------------------------------------------
// Autosave worker
// ---------------------------------------------------------------------------

/// Background thread that is woken on every mutation, debounces for
/// `AUTOSAVE_DEBOUNCE_MS`, then persists a quiet version of the project.
pub fn spawn_autosave(
    project: Arc<RwLock<Project>>,
    library: Arc<RwLock<Library>>,
    data_dir: PathBuf,
    app: tauri::AppHandle,
) -> mpsc::Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::spawn(move || loop {
        if rx.recv().is_err() {
            return;
        }
        // Keep draining until the project has been quiet long enough.
        while rx.recv_timeout(Duration::from_millis(AUTOSAVE_DEBOUNCE_MS)).is_ok() {}

        // Clone under lock, then release before doing any file I/O. Holding a
        // RwLock across `persist`/`write_library` would block every `mutate`
        // (which needs a write lock) for the entire disk write, freezing the
        // editor on slow disks or large projects.
        let (project_snapshot, library_snapshot) = {
            let p = project.read().unwrap().clone();
            let l = library.read().unwrap().clone();
            (p, l)
        };
        let result =
            persist(&project_snapshot, &data_dir).and_then(|_| write_library(&data_dir, &library_snapshot));
        match result {
            Ok(()) => {
                app.state::<AppState>().logger.log(Level::Info, "autosave: saved");
                let _ = app.emit("autosave", serde_json::json!({ "status": "saved", "at": now_iso() }));
            }
            Err(error) => {
                eprintln!("autosave failed: {error}");
                app.state::<AppState>()
                    .logger
                    .log(Level::Error, &format!("autosave: failed: {error}"));
                let _ = app.emit(
                    "autosave",
                    serde_json::json!({ "status": "error", "message": error.to_string() }),
                );
            }
        }
    });
    tx
}

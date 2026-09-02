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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slide {
    pub id: String,
    /// When this playlist slide was added from the library, the source song id.
    #[serde(default)]
    pub library_id: Option<String>,
    /// The source verse/section id within that song.
    #[serde(default)]
    pub library_slide_id: Option<String>,
    pub title: String,
    pub body: String,
    pub background: Background,
    /// Optional per-slide auto-advance timer: when Some(n) and this slide is
    /// live, the backend automatically advances to the next playlist item after
    /// n seconds. None / 0 means no auto-advance. Stored per slide so templates
    /// and persistence cover it.
    #[serde(default)]
    pub auto_advance_secs: Option<u64>,
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
    /// Vertical placement of the text block within the frame (auto mode only).
    pub text_position: TextPosition,
    /// Whether text uses auto flow or explicit absolute bounding boxes.
    #[serde(default)]
    pub positioning: Positioning,
    /// Geometry of the title box (absolute mode).
    #[serde(default)]
    pub title_box: BoxGeometry,
    /// Geometry of the body box (absolute mode).
    #[serde(default)]
    pub body_box: BoxGeometry,
}

fn default_title_font() -> String {
    "sans-serif".to_string()
}
fn default_body_font() -> String {
    "sans-serif".to_string()
}

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
            text_position: TextPosition::Center,
            positioning: Positioning::Auto,
            title_box: BoxGeometry::default(),
            body_box: BoxGeometry::default(),
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
            text_position: TextPosition::Center,
            positioning: Positioning::Auto,
            title_box: BoxGeometry::default(),
            body_box: BoxGeometry::default(),
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
                library_id: None,
                library_slide_id: None,
                title: "Welcome to MakePresent".to_string(),
                body: "This is the Phase 1 test slide.".to_string(),
                background: Background::default(),
                auto_advance_secs: None,
            }],
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
                    library_id: None,
                    library_slide_id: None,
                    title: it.title.clone(),
                    body: it.content.clone().unwrap_or_default(),
                    background: Background::default(),
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

/// Runtime status of the NDI broadcast feed (not persisted; derived live).
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastView {
    /// Whether NDI is enabled in settings and currently broadcasting.
    pub enabled: bool,
    /// The NDI source name receivers see on the network.
    pub source_name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientState {
    pub project: Project,
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
    pub title: String,
    pub body: String,
    pub background: Background,
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
}

fn default_osc_port() -> u16 {
    crate::osc::DEFAULT_OSC_PORT
}

fn default_stage_port() -> u16 {
    crate::network::DEFAULT_STAGE_PORT
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
    let project: Project = serde_json::from_str(&raw).ok()?;
    if project.schema_version != SCHEMA_VERSION {
        eprintln!(
            "project {} uses schema v{}, expected v{SCHEMA_VERSION}",
            path.display(),
            project.schema_version
        );
        return None;
    }
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
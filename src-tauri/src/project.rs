use crate::logging::Level;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub slides: Vec<Slide>,
    pub live: Option<String>,
    /// How the Output switches between live slides ("cut" or "fade").
    #[serde(default)]
    pub transition: Transition,
    pub modified_at: String,
}

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
            }],
            live: None,
            transition: Transition::Cut,
            modified_at: now_iso(),
        }
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientState {
    pub project: Project,
    pub notice: Option<Notice>,
    pub output: OutputView,
    pub stage: StageView,
    /// true on the very first launch (no saved project or settings yet).
    pub first_run: bool,
    /// Per-machine default transition used for new projects.
    pub default_transition: Transition,
    /// Resolved live slide (None when output is black).
    pub current: Option<Slide>,
    /// Resolved next slide in the playlist (None when nothing queued).
    pub next: Option<Slide>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySlide {
    pub id: String,
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySong {
    pub id: String,
    pub title: String,
    pub default_background: Background,
    pub slides: Vec<LibrarySlide>,
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
            schema_version: SCHEMA_VERSION,
            songs: Vec::new(),
        }
    }
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_display_index: None,
            output_display_name: None,
            output_fullscreen: true,
            stage_display_index: None,
            stage_display_name: None,
            stage_visible: false,
            default_transition: Transition::Cut,
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
pub fn read_library(data_dir: &Path) -> Library {
    let raw = fs::read_to_string(data_dir.join("library.json")).ok();
    match raw.and_then(|r| serde_json::from_str::<Library>(&r).ok()) {
        Some(library) => library,
        None => {
            let library = seed_library();
            let _ = write_library(data_dir, &library);
            library
        }
    }
}

pub fn write_library(data_dir: &Path, library: &Library) -> io::Result<()> {
    atomic_write_json(data_dir, "library.json", library)
}

/// A couple of sample songs so the library has content on first launch.
fn seed_library() -> Library {
    Library {
        schema_version: SCHEMA_VERSION,
        songs: vec![
            LibrarySong {
                id: Uuid::new_v4().to_string(),
                title: "Amazing Grace".to_string(),
                default_background: Background::Solid {
                    color: "#1f3a2f".to_string(),
                },
                slides: vec![
                    LibrarySlide {
                        id: Uuid::new_v4().to_string(),
                        title: "Verse 1".to_string(),
                        body: "Amazing grace, how sweet the sound\nThat saved a wretch like me\nI once was lost, but now am found\nWas blind, but now I see.".to_string(),
                    },
                    LibrarySlide {
                        id: Uuid::new_v4().to_string(),
                        title: "Chorus".to_string(),
                        body: "Was grace that taught my heart to fear\nAnd grace my fears relieved\nHow precious did that grace appear\nThe hour I first believed.".to_string(),
                    },
                ],
            },
            LibrarySong {
                id: Uuid::new_v4().to_string(),
                title: "Great Is Thy Faithfulness".to_string(),
                default_background: Background::Solid {
                    color: "#0f2b4a".to_string(),
                },
                slides: vec![
                    LibrarySlide {
                        id: Uuid::new_v4().to_string(),
                        title: "Verse 1".to_string(),
                        body: "Great is Thy faithfulness, O God my Father\nThere is no shadow of turning with Thee\nThou changest not, Thy compassions, they fail not\nAs Thou hast been, Thou forever wilt be.".to_string(),
                    },
                    LibrarySlide {
                        id: Uuid::new_v4().to_string(),
                        title: "Chorus".to_string(),
                        body: "Great is Thy faithfulness!\nGreat is Thy faithfulness!\nMorning by morning new mercies I see\nAll I have needed Thy hand hath provided\nGreat is Thy faithfulness, Lord, unto me.".to_string(),
                    },
                ],
            },
        ],
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
    let project = project.unwrap_or_else(|| {
        eprintln!("no usable project file found, seeding new project");
        Project::test()
    });

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

        let project = project.read().unwrap();
        let library = library.read().unwrap();
        let result = persist(&project, &data_dir).and_then(|_| write_library(&data_dir, &library));
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
use crate::logging::Logger;
use crate::project::{Library, Notice, Project, Settings};
use crate::scripture::ScriptureIndex;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};

/// The single source of truth for the whole application, managed by Tauri.
pub struct AppState {
    pub project: Arc<RwLock<Project>>,
    pub library: Arc<RwLock<Library>>,
    pub settings: RwLock<Settings>,
    pub notice: RwLock<Option<Notice>>,
    pub data_dir: RwLock<PathBuf>,
    /// Rolling event log (flushed immediately) for crash diagnostics.
    pub logger: Logger,
    /// Wake channel for the autosave worker thread.
    pub save_tx: Mutex<Option<Sender<()>>>,
    /// Scripture search index, loaded once at startup from vendored KJV data.
    pub scripture: RwLock<Option<ScriptureIndex>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            project: Arc::new(RwLock::new(Project::new("New Project"))),
            library: Arc::new(RwLock::new(Library::default())),
            settings: RwLock::new(Settings::default()),
            notice: RwLock::new(None),
            data_dir: RwLock::new(PathBuf::new()),
            logger: Logger::default(),
            save_tx: Mutex::new(None),
            scripture: RwLock::new(None),
        }
    }
}

impl AppState {
    /// Schedule an autosave. Safe no-op if the worker is not running yet.
    pub fn request_save(&self) {
        if let Some(tx) = self.save_tx.lock().unwrap().as_ref() {
            let _ = tx.send(());
        }
    }

    pub fn current_settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn apply_settings(&self, settings: Settings) {
        *self.settings.write().unwrap() = settings;
    }

    pub fn set_notice(&self, notice: Option<Notice>) {
        *self.notice.write().unwrap() = notice;
    }

    pub fn app_data_dir(&self) -> PathBuf {
        self.data_dir.read().unwrap().clone()
    }
}
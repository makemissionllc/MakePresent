use crate::broadcast::Broadcaster;
use crate::logging::Logger;
use crate::midi::MidiListener;
use crate::network::NetworkServer;
use crate::osc::OscListener;
use crate::audio::AudioPlayer;
use crate::project::{now_iso, Library, Notice, Overlay, Project, RenderAck, AckUpdate, Settings};
use crate::scripture::ScriptureIndex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
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
    /// NDI broadcaster (runtime-loaded SDK; inactive unless NDI is enabled).
    pub broadcaster: Broadcaster,
    /// Native MIDI input listener (inactive unless a device is selected).
    pub midi: MidiListener,
    /// UDP OSC listener (inactive unless OSC is enabled).
    pub osc: OscListener,
    /// Local-network Stage Display server (HTTP+WebSocket; inactive unless
    /// Stage Network is enabled).
    pub network: NetworkServer,
    /// Generation counter for per-slide auto-advance timers. Each time the live
    /// slide changes (or is cleared) the generation is bumped so any previously
    /// spawned timer thread can detect it has been cancelled.
    pub auto_advance_gen: AtomicU64,
    /// Targeted stage-only message (nursery alerts, countdowns, operator notes).
    /// Separate from `Project.live` — never affects Output. `None` = no banner.
    pub stage_message: RwLock<Option<String>>,
    /// Generation for stage-message auto-expire timers (like auto_advance_gen).
    pub stage_message_gen: AtomicU64,
    /// Independent overlay layer for Output — lower-third / logo, shown on top of
    /// background + main slide. Separate from `Project.live`; toggling never
    /// affects main slide. `None` = no overlay, `Some` with `visible=false` = hidden but content preserved.
    pub overlay: RwLock<Option<Overlay>>,
    /// Single-track backing audio player (rodio on cpal) — dedicated thread, not tied to slides.
    /// ONE track at a time, routable to a specific output device, independent of system default.
    pub audio: AudioPlayer,
    /// Last render-ack per dumb-renderer window (Output/Stage heartbeat).
    /// Updated from one-way frontend events only — never triggers autosave or
    /// a full snapshot; Editor reads it via the tiny `ack-update` event.
    pub output_ack: RwLock<Option<RenderAck>>,
    pub stage_ack: RwLock<Option<RenderAck>>,
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
            broadcaster: Broadcaster::default(),
            midi: MidiListener::default(),
            osc: OscListener::default(),
            network: NetworkServer::default(),
            auto_advance_gen: AtomicU64::new(0),
            stage_message: RwLock::new(None),
            stage_message_gen: AtomicU64::new(0),
            overlay: RwLock::new(None),
            audio: AudioPlayer::default(),
            output_ack: RwLock::new(None),
            stage_ack: RwLock::new(None),
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

    /// Bump the auto-advance generation, cancelling any previously scheduled
    /// timer without blocking. Returns the new generation id.
    pub fn bump_auto_advance(&self) -> u64 {
        self.auto_advance_gen.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn current_auto_advance_gen(&self) -> u64 {
        self.auto_advance_gen.load(Ordering::SeqCst)
    }

    pub fn bump_stage_message(&self) -> u64 {
        self.stage_message_gen.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn current_stage_message_gen(&self) -> u64 {
        self.stage_message_gen.load(Ordering::SeqCst)
    }

    /// Record a one-way render-ack from a dumb-renderer window. Stamps receipt
    /// time on the backend clock and returns the latest per-window picture for
    /// fan-out. Lock-hold is two short writes — never blocks, never saves.
    pub fn record_ack(&self, window: &str, live_id: Option<String>) -> AckUpdate {
        let ack = RenderAck {
            at: now_iso(),
            live_id,
        };
        match window {
            "stage" => *self.stage_ack.write().unwrap() = Some(ack),
            _ => *self.output_ack.write().unwrap() = Some(ack),
        }
        AckUpdate {
            output: self.output_ack.read().unwrap().clone(),
            stage: self.stage_ack.read().unwrap().clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ack_tracks_windows_independently() {
        let state = AppState::default();
        // Unknown window names fall through to output (fail-open, never drops).
        let u = state.record_ack("output", Some("slide-1".to_string()));
        assert!(u.output.is_some());
        assert_eq!(u.output.as_ref().unwrap().live_id.as_deref(), Some("slide-1"));
        assert!(u.stage.is_none());
        let u = state.record_ack("stage", None);
        assert!(u.output.is_some());
        assert!(u.stage.is_some());
        assert_eq!(u.stage.as_ref().unwrap().live_id, None);
    }
}
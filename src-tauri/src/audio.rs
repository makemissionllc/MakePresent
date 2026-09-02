use crate::project::{AudioStateView, AudioStatus};
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use serde::Serialize;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

// ---------------------------------------------------------------------------
// Device info — cpal enumeration, independent of system default
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn list_output_devices() -> Vec<AudioDeviceInfo> {
    let host = cpal::default_host();
    let default_name = host
        .default_output_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();
    let mut out = Vec::new();
    if let Ok(devices) = host.output_devices() {
        for dev in devices {
            if let Ok(name) = dev.name() {
                out.push(AudioDeviceInfo {
                    id: name.clone(),
                    name: name.clone(),
                    is_default: name == default_name,
                });
            }
        }
    }
    if out.is_empty() {
        if let Some(dev) = host
            .default_output_device()
            .and_then(|d| d.name().ok().map(|n| AudioDeviceInfo {
                id: n.clone(),
                name: n,
                is_default: true,
            }))
        {
            out.push(dev);
        }
    }
    out
}

fn find_output_device_by_id(id: &str) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if let Ok(devices) = host.output_devices() {
        for dev in devices {
            if let Ok(name) = dev.name() {
                if name == id {
                    return Some(dev);
                }
            }
        }
    }
    host.default_output_device()
}

// ---------------------------------------------------------------------------
// Audio player — dedicated background thread, ONE track at a time
// ---------------------------------------------------------------------------

enum Command {
    Load(PathBuf),
    Play,
    Pause,
    Stop,
    SetVolume(f32),
    Seek(u64),
    SetDevice(Option<String>),
}

pub struct AudioPlayer {
    state: Arc<Mutex<AudioStateView>>,
    tx: Mutex<Option<mpsc::Sender<Command>>>,
    handle: Mutex<Option<JoinHandle<()>>>,
    is_active: AtomicBool,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(AudioStateView {
                status: AudioStatus::Stopped,
                current_path: None,
                volume: 1.0,
                device_id: None,
                duration_secs: None,
                position_secs: None,
            })),
            tx: Mutex::new(None),
            handle: Mutex::new(None),
            is_active: AtomicBool::new(false),
        }
    }
}

impl AudioPlayer {
    fn ensure_thread(&self) {
        let mut guard = self.tx.lock().unwrap();
        if guard.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel::<Command>();
        let state_clone = Arc::clone(&self.state);
        let handle = std::thread::spawn(move || {
            // These live ONLY on this thread — never cross Send/Sync boundary via AppState
            let mut stream: Option<OutputStream> = None;
            let mut handle: Option<OutputStreamHandle> = None;
            let mut sink: Option<Sink> = None;
            let mut current_path: Option<PathBuf> = None;

            // Helper to ensure we have a sink for current device
            let ensure_sink = |device_id: Option<&String>,
                               stream: &mut Option<OutputStream>,
                               handle: &mut Option<OutputStreamHandle>,
                               sink: &mut Option<Sink>,
                               volume: f32|
             -> Result<(), String> {
                if sink.is_some() {
                    return Ok(());
                }
                let desired = device_id.cloned();
                let dev = desired
                    .as_deref()
                    .and_then(find_output_device_by_id)
                    .or_else(|| cpal::default_host().default_output_device())
                    .ok_or_else(|| "no output device available".to_string())?;
                let (s, h) = OutputStream::try_from_device(&dev)
                    .map_err(|e| format!("failed to open audio device '{}': {e}", dev.name().unwrap_or_else(|_| "?".to_string())))?;
                let new_sink = Sink::try_new(&h).map_err(|e| format!("failed to create audio sink: {e}"))?;
                new_sink.set_volume(volume);
                *stream = Some(s);
                *handle = Some(h);
                *sink = Some(new_sink);
                Ok(())
            };

            while let Ok(cmd) = rx.recv() {
                let res: Result<(), String> = (|| {
                    match cmd {
                        Command::Load(path) => {
                            // Ensure sink
                            {
                                let st = state_clone.lock().unwrap();
                                let vol = st.volume;
                                let dev = st.device_id.clone();
                                drop(st);
                                ensure_sink(dev.as_ref(), &mut stream, &mut handle, &mut sink, vol)?;
                            }
                            let sink_ref = sink.as_ref().ok_or_else(|| "no audio sink".to_string())?;
                            sink_ref.stop();
                            let file = File::open(&path)
                                .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
                            let decoder = Decoder::new(BufReader::new(file))
                                .map_err(|e| format!("cannot decode {}: {e}", path.display()))?;
                            sink_ref.append(decoder);
                            sink_ref.pause();
                            {
                                let mut st = state_clone.lock().unwrap();
                                st.current_path = Some(path.to_string_lossy().to_string());
                                st.status = AudioStatus::Paused;
                                current_path = Some(path);
                            }
                            Ok(())
                        }
                        Command::Play => {
                            let sink_ref = sink.as_ref().ok_or_else(|| "no track loaded".to_string())?;
                            sink_ref.play();
                            {
                                let mut st = state_clone.lock().unwrap();
                                if st.status != AudioStatus::Playing {
                                    st.status = AudioStatus::Playing;
                                }
                            }
                            Ok(())
                        }
                        Command::Pause => {
                            let sink_ref = sink.as_ref().ok_or_else(|| "no track loaded".to_string())?;
                            sink_ref.pause();
                            {
                                let mut st = state_clone.lock().unwrap();
                                st.status = AudioStatus::Paused;
                            }
                            Ok(())
                        }
                        Command::Stop => {
                            if let Some(s) = sink.as_ref() {
                                s.stop();
                            }
                            {
                                let mut st = state_clone.lock().unwrap();
                                st.status = AudioStatus::Stopped;
                                // Keep current_path so we can reload? But for now, clear status to stopped
                            }
                            Ok(())
                        }
                        Command::SetVolume(v) => {
                            let vol = v.clamp(0.0, 1.5);
                            {
                                let mut st = state_clone.lock().unwrap();
                                st.volume = vol;
                            }
                            if let Some(s) = sink.as_ref() {
                                s.set_volume(vol);
                            }
                            Ok(())
                        }
                        Command::Seek(_secs) => {
                            // rodio 0.17 Sink has no try_seek; keep as no-op (future: re-decode with skip)
                            // We keep the command channel for forward compatibility with seek UI
                            Ok(())
                        }
                        Command::SetDevice(id) => {
                            {
                                let mut st = state_clone.lock().unwrap();
                                st.device_id = id.clone();
                            }
                            // If no sink yet, just store device_id and wait for next load/play
                            // If we have a sink, we need to recreate it for new device
                            if sink.is_some() {
                                let was_playing = sink.as_ref().map(|s| !s.is_paused()).unwrap_or(false);
                                let vol = state_clone.lock().unwrap().volume;
                                // Drop old
                                sink = None;
                                stream = None;
                                handle = None;
                                // Recreate for new device
                                let dev = id
                                    .as_deref()
                                    .and_then(find_output_device_by_id)
                                    .or_else(|| cpal::default_host().default_output_device())
                                    .ok_or_else(|| "no output device available".to_string())?;
                                let (s, h) = OutputStream::try_from_device(&dev)
                                    .map_err(|e| format!("failed to open audio device '{}': {e}", dev.name().unwrap_or_else(|_| "?".to_string())))?;
                                let new_sink = Sink::try_new(&h)
                                    .map_err(|e| format!("failed to create audio sink: {e}"))?;
                                new_sink.set_volume(vol);
                                if was_playing {
                                    new_sink.play();
                                } else {
                                    new_sink.pause();
                                }
                                stream = Some(s);
                                handle = Some(h);
                                sink = Some(new_sink);
                                // If we had a current_path, reload? For now, keep paused and require load
                                if let Some(path) = current_path.clone() {
                                    // Reload the file so the new sink has data
                                    if let Ok(file) = File::open(&path) {
                                        if let Ok(decoder) = Decoder::new(BufReader::new(file)) {
                                            if let Some(s) = sink.as_ref() {
                                                s.stop();
                                                s.append(decoder);
                                                if was_playing {
                                                    s.play();
                                                } else {
                                                    s.pause();
                                                }
                                                let mut st = state_clone.lock().unwrap();
                                                if was_playing {
                                                    st.status = AudioStatus::Playing;
                                                } else {
                                                    st.status = AudioStatus::Paused;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Ok(())
                        }
                    }
                })();
                if let Err(e) = res {
                    eprintln!("audio: command failed: {e}");
                }
                // Update status based on sink state
                {
                    let mut st = state_clone.lock().unwrap();
                    if let Some(s) = sink.as_ref() {
                        if s.empty() {
                            st.status = AudioStatus::Stopped;
                        } else if s.is_paused() {
                            st.status = AudioStatus::Paused;
                        } else {
                            st.status = AudioStatus::Playing;
                        }
                    }
                }
            }
        });
        *self.handle.lock().unwrap() = Some(handle);
        *guard = Some(tx);
        self.is_active.store(true, Ordering::SeqCst);
    }

    fn send(&self, cmd: Command) -> Result<(), String> {
        self.ensure_thread();
        let tx = self.tx.lock().unwrap();
        if let Some(sender) = tx.as_ref() {
            sender
                .send(cmd)
                .map_err(|e| format!("audio thread disconnected: {e}"))?;
            Ok(())
        } else {
            Err("audio player not initialized".to_string())
        }
    }

    pub fn load(&self, path: &Path) -> Result<(), String> {
        // Update state immediately
        {
            let mut st = self.state.lock().unwrap();
            st.current_path = Some(path.to_string_lossy().to_string());
            st.status = AudioStatus::Paused;
        }
        self.send(Command::Load(path.to_path_buf()))
    }

    pub fn play(&self) -> Result<(), String> {
        {
            let mut st = self.state.lock().unwrap();
            st.status = AudioStatus::Playing;
        }
        self.send(Command::Play)
    }

    pub fn pause(&self) -> Result<(), String> {
        {
            let mut st = self.state.lock().unwrap();
            st.status = AudioStatus::Paused;
        }
        self.send(Command::Pause)
    }

    pub fn stop(&self) -> Result<(), String> {
        {
            let mut st = self.state.lock().unwrap();
            st.status = AudioStatus::Stopped;
        }
        self.send(Command::Stop)
    }

    pub fn set_volume(&self, vol: f32) -> Result<(), String> {
        let v = vol.clamp(0.0, 1.5);
        {
            let mut st = self.state.lock().unwrap();
            st.volume = v;
        }
        self.send(Command::SetVolume(v))
    }

    pub fn seek(&self, secs: u64) -> Result<(), String> {
        self.send(Command::Seek(secs))
    }

    pub fn set_device(&self, device_id: Option<String>) -> Result<(), String> {
        {
            let mut st = self.state.lock().unwrap();
            st.device_id = device_id.clone();
        }
        self.send(Command::SetDevice(device_id))
    }

    pub fn get_status(&self) -> AudioStateView {
        self.state.lock().unwrap().clone()
    }

    pub fn shutdown(&self) {
        *self.tx.lock().unwrap() = None;
        if let Some(handle) = self.handle.lock().unwrap().take() {
            let _ = handle.join();
        }
        self.is_active.store(false, Ordering::SeqCst);
    }

    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::SeqCst)
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

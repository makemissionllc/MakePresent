use crate::project::{AudioStateView, AudioStatus};
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use serde::Serialize;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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

fn playback_position(base_secs: u64, started_at: Option<Instant>) -> u64 {
    base_secs.saturating_add(started_at.map_or(0, |instant| instant.elapsed().as_secs()))
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
    None
}

// ---------------------------------------------------------------------------
// Audio player — dedicated background thread, ONE track at a time
// ---------------------------------------------------------------------------

enum Command {
    Load(PathBuf, mpsc::SyncSender<Result<(), String>>),
    Play,
    Pause,
    Stop,
    SetVolume(f32),
    Seek(u64, mpsc::SyncSender<Result<(), String>>),
    SetDevice(Option<String>, mpsc::SyncSender<Result<(), String>>),
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
            let mut position_base_secs = 0u64;
            let mut playing_since: Option<Instant> = None;

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
                let dev = match desired.as_deref() {
                    Some(id) => find_output_device_by_id(id)
                        .ok_or_else(|| format!("audio output device \"{id}\" is no longer available"))?,
                    None => cpal::default_host()
                        .default_output_device()
                        .ok_or_else(|| "no output device available".to_string())?,
                };
                let (s, h) = OutputStream::try_from_device(&dev)
                    .map_err(|e| format!("failed to open audio device '{}': {e}", dev.name().unwrap_or_else(|_| "?".to_string())))?;
                let new_sink = Sink::try_new(&h).map_err(|e| format!("failed to create audio sink: {e}"))?;
                new_sink.set_volume(volume);
                *stream = Some(s);
                *handle = Some(h);
                *sink = Some(new_sink);
                Ok(())
            };

            loop {
                let cmd = match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(cmd) => cmd,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let position = playback_position(position_base_secs, playing_since);
                        let mut state = state_clone.lock().unwrap();
                        if let Some(current_sink) = sink.as_ref() {
                            if current_sink.empty() && state.status == AudioStatus::Playing {
                                state.status = AudioStatus::Stopped;
                                position_base_secs = state.duration_secs.unwrap_or(position);
                                playing_since = None;
                                state.position_secs = Some(position_base_secs);
                            } else {
                                state.position_secs = Some(
                                    state.duration_secs.map_or(position, |duration| position.min(duration)),
                                );
                            }
                        }
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                let res: Result<(), String> = (|| {
                    match cmd {
                        Command::Load(path, reply) => {
                            let result = (|| {
                                // Ensure sink, then fully open/decode the file
                                // before replacing the previous track.
                                {
                                    let st = state_clone.lock().unwrap();
                                    let vol = st.volume;
                                    let dev = st.device_id.clone();
                                    drop(st);
                                    ensure_sink(dev.as_ref(), &mut stream, &mut handle, &mut sink, vol)?;
                                }
                                let file = File::open(&path)
                                    .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
                                let decoder = Decoder::new(BufReader::new(file))
                                    .map_err(|e| format!("cannot decode {}: {e}", path.display()))?;
                                let duration_secs = decoder.total_duration().map(|d| d.as_secs());
                                let sink_ref = sink.as_ref().ok_or_else(|| "no audio sink".to_string())?;
                                sink_ref.stop();
                                sink_ref.append(decoder);
                                sink_ref.pause();
                                {
                                    let mut st = state_clone.lock().unwrap();
                                    st.current_path = Some(path.to_string_lossy().to_string());
                                    st.status = AudioStatus::Paused;
                                    st.duration_secs = duration_secs;
                                    st.position_secs = Some(0);
                                    current_path = Some(path);
                                    position_base_secs = 0;
                                    playing_since = None;
                                }
                                Ok(())
                            })();
                            let _ = reply.send(result.clone());
                            result
                        }
                        Command::Play => {
                            let sink_ref = sink.as_ref().ok_or_else(|| "no track loaded".to_string())?;
                            if sink_ref.empty() {
                                let path = current_path
                                    .as_ref()
                                    .ok_or_else(|| "no track loaded".to_string())?;
                                let file = File::open(path)
                                    .map_err(|e| format!("cannot reopen {}: {e}", path.display()))?;
                                let decoder = Decoder::new(BufReader::new(file))
                                    .map_err(|e| format!("cannot decode {}: {e}", path.display()))?;
                                sink_ref.append(decoder);
                                position_base_secs = 0;
                            }
                            sink_ref.play();
                            {
                                let mut st = state_clone.lock().unwrap();
                                st.status = AudioStatus::Playing;
                                st.position_secs = Some(position_base_secs);
                            }
                            playing_since = Some(Instant::now());
                            Ok(())
                        }
                        Command::Pause => {
                            let sink_ref = sink.as_ref().ok_or_else(|| "no track loaded".to_string())?;
                            position_base_secs = playback_position(position_base_secs, playing_since);
                            playing_since = None;
                            sink_ref.pause();
                            {
                                let mut st = state_clone.lock().unwrap();
                                st.status = AudioStatus::Paused;
                                st.position_secs = Some(position_base_secs);
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
                                st.position_secs = Some(0);
                            }
                            position_base_secs = 0;
                            playing_since = None;
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
                        Command::Seek(secs, reply) => {
                            let result = (|| {
                                let path = current_path
                                    .as_ref()
                                    .ok_or_else(|| "no track loaded".to_string())?;
                                let current_sink = sink.as_ref().ok_or_else(|| "no audio sink".to_string())?;
                                let duration = state_clone.lock().unwrap().duration_secs;
                                let target = duration.map_or(secs, |d| secs.min(d));
                                let file = File::open(path)
                                    .map_err(|e| format!("cannot reopen {}: {e}", path.display()))?;
                                let decoder = Decoder::new(BufReader::new(file))
                                    .map_err(|e| format!("cannot decode {}: {e}", path.display()))?;
                                let was_playing = state_clone.lock().unwrap().status == AudioStatus::Playing;
                                let skipped = decoder.skip_duration(Duration::from_secs(target));
                                current_sink.stop();
                                current_sink.append(skipped);
                                position_base_secs = target;
                                if target >= duration.unwrap_or(u64::MAX) {
                                    playing_since = None;
                                } else if was_playing {
                                    current_sink.play();
                                    playing_since = Some(Instant::now());
                                } else {
                                    current_sink.pause();
                                    playing_since = None;
                                }
                                let mut state = state_clone.lock().unwrap();
                                state.position_secs = Some(target);
                                state.status = if target >= duration.unwrap_or(u64::MAX) {
                                    AudioStatus::Stopped
                                } else if was_playing {
                                    AudioStatus::Playing
                                } else {
                                    AudioStatus::Paused
                                };
                                Ok(())
                            })();
                            let _ = reply.send(result.clone());
                            result
                        }
                        Command::SetDevice(id, reply) => {
                            let result = (|| {
                                // If playback has not opened a stream yet, store
                                // the validated selection for the next Load.
                                if sink.is_none() {
                                    state_clone.lock().unwrap().device_id = id;
                                    return Ok(());
                                }

                                // Build the replacement before dropping the
                                // active stream. A disconnected or unavailable
                                // device must not stop the current track.
                                let device = match id.as_deref() {
                                    Some(id) => find_output_device_by_id(id).ok_or_else(|| {
                                        format!("audio output device \"{id}\" is no longer available")
                                    })?,
                                    None => cpal::default_host()
                                        .default_output_device()
                                        .ok_or_else(|| "no output device available".to_string())?,
                                };
                                let name = device.name().unwrap_or_else(|_| "?".to_string());
                                let (new_stream, new_handle) = OutputStream::try_from_device(&device)
                                    .map_err(|e| format!("failed to open audio device '{name}': {e}"))?;
                                let volume = state_clone.lock().unwrap().volume;
                                let new_sink = Sink::try_new(&new_handle)
                                    .map_err(|e| format!("failed to create audio sink: {e}"))?;
                                new_sink.set_volume(volume);
                                let was_playing = state_clone.lock().unwrap().status == AudioStatus::Playing;
                                let position = playback_position(position_base_secs, playing_since);
                                if let Some(path) = current_path.as_ref() {
                                    let file = File::open(path)
                                        .map_err(|e| format!("cannot reload {}: {e}", path.display()))?;
                                    let decoder = Decoder::new(BufReader::new(file))
                                        .map_err(|e| format!("cannot decode {}: {e}", path.display()))?;
                                    new_sink.append(decoder.skip_duration(Duration::from_secs(position)));
                                }
                                if was_playing {
                                    new_sink.play();
                                } else {
                                    new_sink.pause();
                                }

                                let old_sink = sink.replace(new_sink);
                                let old_handle = handle.replace(new_handle);
                                let old_stream = stream.replace(new_stream);
                                drop(old_sink);
                                drop(old_handle);
                                drop(old_stream);
                                position_base_secs = position;
                                playing_since = was_playing.then(Instant::now);
                                let mut state = state_clone.lock().unwrap();
                                state.device_id = id;
                                if current_path.is_some() {
                                    state.status = if was_playing {
                                        AudioStatus::Playing
                                    } else {
                                        AudioStatus::Paused
                                    };
                                    state.position_secs = Some(position);
                                }
                                Ok(())
                            })();
                            let _ = reply.send(result.clone());
                            result
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

    fn send_wait(
        &self,
        command: impl FnOnce(mpsc::SyncSender<Result<(), String>>) -> Command,
    ) -> Result<(), String> {
        self.ensure_thread();
        let (reply, response) = mpsc::sync_channel(1);
        {
            let tx = self.tx.lock().unwrap();
            tx.as_ref()
                .ok_or_else(|| "audio player not initialized".to_string())?
                .send(command(reply))
                .map_err(|e| format!("audio thread disconnected: {e}"))?;
        }
        response
            .recv()
            .map_err(|e| format!("audio command did not finish: {e}"))?
    }

    pub fn load(&self, path: &Path) -> Result<(), String> {
        self.send_wait(|reply| Command::Load(path.to_path_buf(), reply))
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
        self.send_wait(|reply| Command::Seek(secs, reply))
    }

    pub fn set_device(&self, device_id: Option<String>) -> Result<(), String> {
        self.send_wait(|reply| Command::SetDevice(device_id, reply))
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

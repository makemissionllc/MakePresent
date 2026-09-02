//! Native MIDI input via [`midir`].
//!
//! This replaces the clunky WebMIDI browser wrapper story: MakePresent talks
//! to real MIDI hardware (foot controllers, keyboards) or software (Ableton
//! Live, virtual MIDI cables) natively on the OS MIDI subsystem.
//!
//! # Device enumeration differences by OS
//!
//! * **Windows** — midir uses the WinMM (`midiIn*`) API. Ports are WinMM
//!   device ids; names look like "MPU-401" / "LoopBe Internal MIDI".
//! * **Linux/Ubuntu** — midir uses ALSA raw MIDI (sequencer). Ports are ALSA
//!   sub-devices; names look like "Midi Through:... 14:0". Requires the ALSA
//!   dev libraries at build time (`libasound2-dev`).
//! * **macOS** — midir uses CoreMIDI.
//!
//! `midir` already normalises these behind one API; we just enumerate
//! `MidiInput::ports()`. The stable per-port [`midir::MidiInputPort::id`] is
//! what we persist in settings, so a saved device can be reconnected by id even
//! if its index changes between reboots.

use crate::logging::Level;
use crate::state::AppState;
use crate::triggers::{route_incoming, Trigger};
use midir::{Ignore, MidiInput, MidiInputConnection};
use serde::Serialize;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

static LAST_STATUS: AtomicU8 = AtomicU8::new(0);

/// A MIDI input device as shown in the settings panel.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiDeviceInfo {
    /// Stable opaque device id (for persistence & reconnect).
    pub id: String,
    /// Human-friendly port name.
    pub name: String,
}

/// Parsed, human-readable MIDI message emitted for the live "what am I
/// sending?" monitor in settings.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiMessageView {
    pub channel: u8,
    pub kind: String,
    pub data: String,
    pub describe: String,
    /// First data byte (note / controller / program) — lets the settings UI
    /// rebuild a `Trigger` from a captured message.
    pub data1: Option<u8>,
    /// Second data byte (velocity / CC value), when present.
    pub data2: Option<u8>,
}

/// Owns the active MIDI input connection. Held in [`AppState`]; at most one
/// device is listened to at a time.
pub struct MidiListener {
    inner: Mutex<Option<MidiInputConnection<AppHandle>>>,
}

impl Default for MidiListener {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl MidiListener {
    pub fn is_active(&self) -> bool {
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    /// Connect to the device matching `device_id`. Replaces any current
    /// connection. Returns an error (logged by the caller) without crashing.
    pub fn start(&self, app: AppHandle, device_id: &str) -> Result<(), String> {
        let state = app.state::<AppState>();
        // Create the MidiInput on the connecting thread. On Linux this needs
        // the ALSA client setup, on Windows the WinMM device init.
        let mut midi_in = MidiInput::new("MakrStudio")
            .map_err(|e| format!("could not initialise MIDI subsystem: {e}"))?;
        // We want to see real messages, but there is no need for our trigger
        // matching to react to raw clock/active-sense noise. Keeping them out
        // of the callback also protects against a flood from a misbehaving
        // device. (We still receive everything else — more below.)
        midi_in.ignore(Ignore::ActiveSense);

        let port = midi_in
            .find_port_by_id(device_id)
            .ok_or_else(|| {
                format!(
                    "MIDI device {device_id} not found — is it connected? Available devices may have changed."
                )
            })?;
        let port_name = midi_in
            .port_name(&port)
            .unwrap_or_else(|_| "unknown device".to_string());

        // Drop any previous connection first so midir can release the port.
        self.stop();

        let connection = midi_in
            .connect(
                &port,
                "makepresent-advance",
                on_midi_message,
                app.clone(),
            )
            .map_err(|e| format!("could not open MIDI device \"{port_name}\": {e}"))?;

        *self.inner.lock().unwrap() = Some(connection);
        state.logger.log(
            Level::Info,
            &format!("midi: listening on \"{port_name}\""),
        );
        Ok(())
    }

    /// Close the current connection, if any.
    pub fn stop(&self) {
        if let Some(conn) = self.inner.lock().unwrap().take() {
            drop(conn); // Dropping the connection releases the MIDI port.
        }
    }
}

/// Called by midir for every fully-buffered MIDI message on the selected
/// device. Runs on midir's own I/O thread. Parses defensively: a malformed or
/// unknown byte sequence is logged and ignored — it can never panic or advance
/// a slide on its own.
fn on_midi_message(timestamp: u64, message: &[u8], app: &mut AppHandle) {
    let parsed = match parse_midi_message(message) {
        Some(p) => p,
        None => {
            app.state::<AppState>().logger.log(
                Level::Debug,
                &format!(
                    "midi: unrecognised message ({} bytes): {:02X?} — ignored",
                    message.len(),
                    message
                ),
            );
            return;
        }
    };

    // Always surface the raw message to the settings "live monitor" so an
    // operator can press a button and immediately see which note/CC it sends.
    let _ = app.emit(
        "midi-message",
        MidiMessageView {
            channel: parsed.channel,
            kind: parsed.kind.label().to_string(),
            data: parsed.raw_description(),
            describe: parsed.describe(),
            data1: Some(parsed.data1),
            data2: parsed.data2,
        },
    );

    // Route through the trigger system (incoming + timestamp for future use).
    let _ = timestamp;
    route_incoming(app, &parsed.into_trigger());
}

// ---------------------------------------------------------------------------
// Device enumeration
// ---------------------------------------------------------------------------

/// Enumerate every available MIDI input device. The set differs by OS, but
/// midir hides that behind one `ports()` call.
pub fn list_devices() -> Result<Vec<MidiDeviceInfo>, String> {
    let midi_in = MidiInput::new("MakrStudio")
        .map_err(|e| format!("could not initialise MIDI subsystem: {e}"))?;
    let ports = midi_in.ports();
    let mut out = Vec::with_capacity(ports.len());
    for port in ports {
        let name = midi_in
            .port_name(&port)
            .unwrap_or_else(|_| "unknown device".to_string());
        let id = port.id();
        out.push(MidiDeviceInfo { id, name });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Message parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum MidiKind {
    NoteOn,
    NoteOff,
    ControlChange,
    ProgramChange,
}

impl MidiKind {
    fn label(self) -> &'static str {
        match self {
            MidiKind::NoteOn => "note_on",
            MidiKind::NoteOff => "note_off",
            MidiKind::ControlChange => "cc",
            MidiKind::ProgramChange => "program",
        }
    }
}

/// A single, clean, channel-aware message with its musical meaning.
#[derive(Clone)]
struct ParsedMidi {
    channel: u8,
    kind: MidiKind,
    data1: u8,
    data2: Option<u8>,
}

impl ParsedMidi {
    fn describe(&self) -> String {
        match self.kind {
            MidiKind::NoteOn => format!("Note On {} (note {})", note_name(self.data1), self.data1),
            MidiKind::NoteOff => format!("Note Off {} (note {})", note_name(self.data1), self.data1),
            MidiKind::ControlChange => {
                format!("CC {} value {}", self.data1, self.data2.unwrap_or(0))
            }
            MidiKind::ProgramChange => format!("Program {}", self.data1),
        }
    }

    fn raw_description(&self) -> String {
        let first = match self.kind {
            MidiKind::NoteOn => 0x90,
            MidiKind::NoteOff => 0x80,
            MidiKind::ControlChange => 0xB0,
            MidiKind::ProgramChange => 0xC0,
        } | (self.channel - 1);
        match self.data2 {
            Some(d2) => format!("{first:#04X} {:02X} {:02X}", self.data1, d2),
            None => format!("{first:#04X} {:02X}", self.data1),
        }
    }

    fn into_trigger(self) -> Trigger {
        match self.kind {
            MidiKind::NoteOn => Trigger::MidiNote {
                channel: self.channel,
                note: self.data1,
            },
            MidiKind::NoteOff => Trigger::MidiNote {
                channel: self.channel,
                note: self.data1,
            },
            MidiKind::ControlChange => Trigger::MidiControl {
                channel: self.channel,
                controller: self.data1,
                value: self.data2,
            },
            MidiKind::ProgramChange => Trigger::MidiProgram {
                channel: self.channel,
                program: self.data1,
            },
        }
    }
}

/// Parse a raw MIDI byte slice into a clean message. Handles running status
/// (reusing the previous status byte when a message is missing one) and
/// guards every index so a short/garbage buffer is logged, not panic'd on.
fn parse_midi_message(message: &[u8]) -> Option<ParsedMidi> {
    let running_status = LAST_STATUS.load(Ordering::Relaxed);

    if message.is_empty() {
        return None;
    }

    let mut status = message[0];

    let body: &[u8] = if !is_status(status) {
        if !is_status(running_status) || running_status < 0x80 {
            return None;
        }
        status = running_status;
        message
    } else {
        LAST_STATUS.store(status, Ordering::Relaxed);
        &message[1..]
    };

    parse_body(status, body)
}

fn parse_body(status: u8, body: &[u8]) -> Option<ParsedMidi> {
    let channel = (status & 0x0F) + 1; // 1..=16
    match status & 0xF0 {
        0x80 | 0x90 => {
            // Note Off / Note On: [data1, data2]
            if body.len() < 2 {
                return None;
            }
            let kind = if status & 0xF0 == 0x90 && body[1] > 0 {
                MidiKind::NoteOn
            } else {
                MidiKind::NoteOff
            };
            Some(ParsedMidi {
                channel,
                kind,
                data1: body[0],
                data2: Some(body[1]),
            })
        }
        0xB0 => {
            // Control Change: [controller, value]
            if body.len() < 2 {
                return None;
            }
            // Ignore the channel common "All Notes Off" style CCs? No — treat
            // all CCs uniformly; matching is by the mapping anyway.
            Some(ParsedMidi {
                channel,
                kind: MidiKind::ControlChange,
                data1: body[0],
                data2: Some(body[1]),
            })
        }
        0xC0 => {
            // Program Change: [program]
            if body.len() < 1 {
                return None;
            }
            Some(ParsedMidi {
                channel,
                kind: MidiKind::ProgramChange,
                data1: body[0],
                data2: None,
            })
        }
        _ => None,
    }
}

fn is_status(b: u8) -> bool {
    b >= 0x80
}

/// Convert a MIDI note number to a human name (C0..G10) for display.
fn note_name(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let pc = (note % 12) as usize;
    let octave = (note as i32 / 12) - 1;
    format!("{}{}", NAMES[pc], octave)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_note_on() {
        let p = parse_midi_message(&[0x90, 60, 100]).unwrap();
        assert_eq!(p.channel, 1);
        assert!(matches!(p.kind, MidiKind::NoteOn));
        assert_eq!(p.data1, 60);
    }

    #[test]
    fn parses_channel_10_cc() {
        let p = parse_midi_message(&[0xB9, 7, 64]).unwrap(); // ch 10 (0x09+1)
        assert_eq!(p.channel, 10);
        assert!(matches!(p.kind, MidiKind::ControlChange));
        assert_eq!(p.data1, 7);
    }

    #[test]
    fn parses_program_change() {
        let p = parse_midi_message(&[0xC3, 5]).unwrap(); // ch 4
        assert_eq!(p.channel, 4);
        assert!(matches!(p.kind, MidiKind::ProgramChange));
    }

    #[test]
    fn note_off_zero_velocity_is_treated_as_off() {
        let p = parse_midi_message(&[0x90, 60, 0]).unwrap();
        assert!(matches!(p.kind, MidiKind::NoteOff));
    }

    #[test]
    fn empty_or_short_messages_are_safe() {
        assert!(parse_midi_message(&[]).is_none());
        assert!(parse_midi_message(&[0x90]).is_none());
        assert!(parse_midi_message(&[0x90, 60]).is_none());
    }

    #[test]
    fn garbage_status_is_rejected() {
        assert!(parse_midi_message(&[0xF0, 0x01]).is_none()); // sysex start unsupported
    }

    #[test]
    fn human_note_names() {
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(72), "C5");
        assert_eq!(note_name(61), "C#4");
    }
}

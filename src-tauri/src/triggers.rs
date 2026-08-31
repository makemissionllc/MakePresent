//! External trigger system: turning MIDI and OSC messages into presentation
//! actions (next / previous / jump to a slide / clear output).
//!
//! This is the "Future" feature that lets real hardware and software (Ableton
//! Live, a MIDI foot controller, a lighting console, QLab, ...) cue slides
//! directly, instead of reaching for clunky WebMIDI browser wrappers.
//!
//! # Design
//!
//! * A **trigger** is a concrete wire message — a MIDI Note/CC/Program Change
//!   or an OSC address — defined here as a serializable enum so mappings can
//!   be stored in the per-machine settings file.
//! * A **mapping** pairs a trigger with an **action**. Actions deliberately
//!   map 1:1 onto the existing command path (`set_live_slide` / `clear_output`)
//!   so triggering never duplicates slide-advance logic.
//! * `run_action` is the single choke point that both the MIDI and OSC
//!   listeners call. It is intentionally defensive: every branch is guarded,
//!   a misconfigured action (e.g. jump index out of range) is logged and
//!   ignored, and it can never panic or corrupt live output.

use crate::commands;
use crate::logging::Level;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Trigger — a concrete incoming message
// ---------------------------------------------------------------------------

/// A single incoming message we know how to describe. The wire listeners
/// (`midi.rs`, `osc.rs`) produce these; mappings compare against them.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// MIDI Note On / Note Off. `channel` is 1..=16 (human-friendly), `note`
    /// is the 0..=127 MIDI note number (60 = middle C).
    MidiNote { channel: u8, note: u8 },
    /// MIDI Control Change. `value` is `None` to match any value (e.g. a pedal
    /// press) or `Some(v)` for a specific controller value.
    MidiControl {
        channel: u8,
        controller: u8,
        value: Option<u8>,
    },
    /// MIDI Program Change.
    MidiProgram { channel: u8, program: u8 },
    /// An OSC address pattern, e.g. `/makepresent/next` or `/makepresent/goto/n`.
    OscAddress { address: String },
}

impl Trigger {
    /// Human-readable description used for logging and the settings list.
    pub fn describe(&self) -> String {
        match self {
            Trigger::MidiNote { channel, note } => {
                format!("MIDI Note {} (C{}) on ch {}", note, self.note_name(), channel)
            }
            Trigger::MidiControl {
                channel,
                controller,
                value,
            } => match value {
                Some(v) => format!("MIDI CC {} = {} on ch {}", controller, v, channel),
                None => format!("MIDI CC {} (any value) on ch {}", controller, channel),
            },
            Trigger::MidiProgram { channel, program } => {
                format!("MIDI Program {} on ch {}", program, channel)
            }
            Trigger::OscAddress { address } => format!("OSC {}", address),
        }
    }

    fn note_name(&self) -> String {
        let Trigger::MidiNote { note, .. } = self else {
            return String::new();
        };
        const NAMES: [&str; 12] = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        let pc = (*note % 12) as usize;
        let octave = (*note as i32 / 12) - 1;
        format!("{}{}", NAMES[pc], octave)
    }
}

// ---------------------------------------------------------------------------
// Action — what a mapping does
// ---------------------------------------------------------------------------

/// What should happen when a trigger fires. These mirror the manual controls
/// so a foot pedal and a mouse click end up on the exact same code path.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerAction {
    /// Advance to the slide after the current live slide.
    NextSlide,
    /// Go back to the slide before the current live slide.
    PrevSlide,
    /// Jump straight to a slide by playlist index (0-based).
    JumpTo { index: u32 },
    /// Blank the output (no live slide).
    ClearOutput,
}

impl TriggerAction {
    pub fn label(&self) -> String {
        match self {
            TriggerAction::NextSlide => "Next slide".to_string(),
            TriggerAction::PrevSlide => "Previous slide".to_string(),
            TriggerAction::JumpTo { index } => format!("Jump to slide {}", index + 1),
            TriggerAction::ClearOutput => "Clear output / blank".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Mapping — trigger + action, stored in settings
// ---------------------------------------------------------------------------

/// A persisted trigger-to-action mapping.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerMapping {
    pub id: String,
    pub trigger: Trigger,
    pub action: TriggerAction,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub label: Option<String>,
}

impl TriggerMapping {
    pub fn new(trigger: Trigger, action: TriggerAction) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            trigger,
            action,
            enabled: true,
            label: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Routing / action execution
// ---------------------------------------------------------------------------

/// The single choke point both listeners (MIDI + OSC) call on every incoming
/// message. It looks the trigger up in the *current* settings (so edits take
/// effect immediately) and either fires the mapped action or logs the message
/// as unrecognised. Never panics — every step is best-effort.
pub fn route_incoming(app: &AppHandle, trigger: &Trigger) {
    let state = app.state::<AppState>();
    state.logger.log(
        Level::Debug,
        &format!("trigger: saw {}", trigger.describe()),
    );

    let settings = state.current_settings();

    // Exact match first (a mapping whose trigger equals the incoming message).
    let exact = settings
        .triggers
        .iter()
        .find(|m| m.enabled && &m.trigger == trigger)
        .map(|m| (m.action.clone(), m.label.clone()));

    // Allow parameterised OSC "goto" addresses: an incoming `/makepresent/goto/N`
    // matches a mapping registered for the bare prefix `/makepresent/goto` and
    // resolves to "jump to slide N".
    let pattern = (match trigger {
        Trigger::OscAddress { address } => osc_goto_match(&settings.triggers, address),
        _ => None,
    })
    .map(|(m, index)| (m.action_with_goto(index), m.label.clone()));

    let outcome = exact.or(pattern);

    match outcome {
        Some((action, label)) => {
            let who = label.unwrap_or_else(|| trigger.describe());
            state.logger.log(
                Level::Info,
                &format!("trigger: {} → \"{}\"", who, action.label()),
            );
            run_action(app, &action);
        }
        None => {
            // Unmapped / unexpected message: log and move on — never erroring.
            state.logger.log(
                Level::Debug,
                &format!("trigger: {} (unmapped, ignored)", trigger.describe()),
            );
        }
    }
}

/// If `address` is of the form `/prefix/N` (e.g. `/makepresent/goto/5`) and a
/// mapping exists for the bare `/prefix`, return that mapping together with the
/// zero-based slide index (N-1). Called only for OSC triggers.
fn osc_goto_match<'a>(
    mappings: &'a [TriggerMapping],
    address: &str,
) -> Option<(&'a TriggerMapping, u32)> {
    let (prefix, last_seg) = address.rsplit_once('/')?;
    if prefix.is_empty() || last_seg.is_empty() {
        return None;
    }
    let n: u32 = last_seg.parse().ok()?;
    if n == 0 {
        return None;
    }
    mappings
        .iter()
        .find(|m| {
            m.enabled
                && matches!(&m.trigger, Trigger::OscAddress { address: a } if a == prefix)
        })
        .map(|m| (m, n - 1))
}

impl TriggerMapping {
    /// When a goto-pattern matched, the `index` overrides the mapping's own
    /// action target (the mapping is usually `JumpTo { index }` or the user's
    /// chosen action; a bare `/makepresent/goto` prefix is jump-oriented).
    fn action_with_goto(&self, index: u32) -> TriggerAction {
        match &self.action {
            TriggerAction::JumpTo { .. } => TriggerAction::JumpTo { index },
            other => other.clone(),
        }
    }
}

/// Execute a trigger action through the *same* command path the UI uses.
/// `commands::execute_action` delegates to `set_live_slide` / `clear_output`,
/// which own the actual slide-advance state changes, window management and
/// state broadcast — we never rewrite that logic here.
///
/// Runs `commands::execute_action` off the main thread; slide-advance itself
/// keeps its main-thread guarantees internally (see `set_live_slide`).
pub fn run_action(app: &AppHandle, action: &TriggerAction) {
    let app = app.clone();
    let action = action.clone();
    std::thread::spawn(move || {
        if let Err(e) = commands::execute_action(&app, &action) {
            app.state::<AppState>()
                .logger
                .log(Level::Error, &format!("trigger: could not run action: {e}"));
        }
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_slide_describes_as_next() {
        assert_eq!(TriggerAction::NextSlide.label(), "Next slide");
    }

    #[test]
    fn jump_label_is_one_based() {
        assert_eq!(TriggerAction::JumpTo { index: 4 }.label(), "Jump to slide 5");
    }

    #[test]
    fn note_trigger_roundtrips_json() {
        let t = Trigger::MidiNote {
            channel: 1,
            note: 60,
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: Trigger = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn note_describe_uses_musical_name() {
        let t = Trigger::MidiNote {
            channel: 1,
            note: 60,
        };
        assert!(t.describe().contains("C4"));
    }
}

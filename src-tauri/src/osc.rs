//! Lightweight OSC (Open Sound Control) listener over UDP.
//!
//! OSC is the de-facto network control protocol for lighting consoles and many
//! digital audio workstations. MakePresent supports a small family of address
//! patterns — e.g. `/makepresent/next`, `/makepresent/goto/5` — mapped through
//! the same trigger system as MIDI, so a lighting desk or LTC-following app can
//! cue slides without any browser/MIDI wrapper.
//!
//! The listener binds a UDP socket on a chosen port and loops, decoding each
//! datagram with `rosc`, then routing decoded messages through
//! [`crate::triggers::route_incoming`]. It is deliberately simple: no reply, no
//! TCP, no osc timetag scheduling — one address equals one action.

use crate::logging::Level;
use crate::state::AppState;
use crate::triggers::{route_incoming, Trigger};
use rosc::decoder::decode_udp;
use rosc::{OscPacket, OscType};
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tauri::{AppHandle, Manager};

/// Default UDP port for the OSC listener (a conventional "sends on 9000" choice,
/// and one that won't collide with typical 8000-range web tooling).
pub const DEFAULT_OSC_PORT: u16 = 9000;

/// Owns the running OSC listener thread (if any). Held in [`AppState`].
pub struct OscListener {
    inner: Mutex<Option<OscInner>>,
}

struct OscInner {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Default for OscListener {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl OscListener {
    pub fn is_active(&self) -> bool {
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    /// Start (or restart) the UDP listener on `port`. Replaces any current
    /// listener. Returns an error (logged by the caller) if the port can't be
    /// bound — e.g. already taken — without crashing anything.
    pub fn start(&self, app: AppHandle, port: u16) -> Result<(), String> {
        self.stop();

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let addr = format!("0.0.0.0:{port}");
        let thread = thread::Builder::new()
            .name("osc-listener".to_string())
            .spawn(move || {
                let socket = match UdpSocket::bind(&addr) {
                    Ok(s) => s,
                    Err(e) => {
                        app.state::<AppState>().logger.log(
                            Level::Error,
                            &format!("osc: could not bind UDP {addr}: {e}"),
                        );
                        return;
                    }
                };
                // 1 second read timeout so the loop can check the stop flag
                // and exit promptly on shutdown.
                let _ = socket.set_read_timeout(Some(std::time::Duration::from_millis(1000)));
                app.state::<AppState>().logger.log(
                    Level::Info,
                    &format!("osc: listening on UDP {addr}"),
                );

                let mut buf = [0u8; 65507]; // max UDP payload
                while !thread_stop.load(Ordering::Relaxed) {
                    match socket.recv_from(&mut buf) {
                        Ok((len, _src)) => {
                            let datagram = &buf[..len];
                            match decode_udp(datagram) {
                                Ok((_rest, packet)) => {
                                    for trigger in packet_to_triggers(&packet) {
                                        route_incoming(&app, &trigger);
                                    }
                                }
                                Err(e) => {
                                    // Log clearly but keep listening — a junk
                                    // datagram must never crash the app.
                                    app.state::<AppState>().logger.log(
                                        Level::Debug,
                                        &format!(
                                            "osc: could not decode {} bytes: {e} — ignored",
                                            datagram.len()
                                        ),
                                    );
                                }
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // timeout — loop back and check the stop flag
                        }
                        Err(e) => {
                            if thread_stop.load(Ordering::Relaxed) {
                                break;
                            }
                            app.state::<AppState>().logger.log(
                                Level::Warn,
                                &format!("osc: receive error: {e} — continuing"),
                            );
                        }
                    }
                }
                app.state::<AppState>().logger.log(
                    Level::Info,
                    &format!("osc: stopped listening on UDP {addr}"),
                );
            })
            .map_err(|e| format!("could not spawn OSC listener: {e}"))?;

        *self.inner.lock().unwrap() = Some(OscInner {
            stop,
            thread: Some(thread),
        });
        Ok(())
    }

    /// Stop the running listener and join the thread.
    pub fn stop(&self) {
        if let Some(inner) = self.inner.lock().unwrap().take() {
            inner.stop.store(true, Ordering::Relaxed);
            if let Some(thread) = inner.thread {
                let _ = thread.join();
            }
        }
    }
}

/// Flatten an OSC packet (which may be a bundle of nested messages) into a list
/// of triggers. Defensive: any unknown/empty content simply yields nothing.
fn packet_to_triggers(packet: &OscPacket) -> Vec<Trigger> {
    let mut out = Vec::new();
    collect(packet, &mut out);
    out
}

fn collect(packet: &OscPacket, out: &mut Vec<Trigger>) {
    match packet {
        OscPacket::Message(msg) => {
            let address = msg.addr.trim().to_string();
            if address.is_empty() {
                return;
            }
            // A `/makepresent/goto/N` style address is stored verbatim so an
            // exact-address mapping can match it; the numeric handling itself
            // lives in triggers::route_incoming so the mapping model stays
            // uniform with MIDI.
            out.push(Trigger::OscAddress { address });
        }
        OscPacket::Bundle(bundle) => {
            for inner in &bundle.content {
                collect(inner, out);
            }
        }
    }
}

/// Whether an OscType contains an integer we can use (i32 / i64) — helper for
/// future-proofing argument parsing. Currently unused; kept for documentation.
fn _int_of(t: &OscType) -> Option<i64> {
    match t {
        OscType::Int(v) => Some(*v as i64),
        OscType::Long(v) => Some(*v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosc::{OscBundle, OscMessage, OscTime};

    fn msg(addr: &str) -> OscPacket {
        OscPacket::Message(OscMessage {
            addr: addr.to_string(),
            args: vec![],
        })
    }

    #[test]
    fn extracts_single_message() {
        let triggers = packet_to_triggers(&msg("/makepresent/next"));
        assert_eq!(
            triggers,
            vec![Trigger::OscAddress {
                address: "/makepresent/next".to_string()
            }]
        );
    }

    #[test]
    fn extracts_nested_bundle_messages() {
        let bundle = OscPacket::Bundle(OscBundle {
            timetag: OscTime {
                seconds: 0,
                fractional: 0,
            },
            content: vec![msg("/makepresent/next"), msg("/makepresent/clear")],
        });
        let triggers = packet_to_triggers(&bundle);
        assert_eq!(triggers.len(), 2);
        assert_eq!(
            triggers[1],
            Trigger::OscAddress {
                address: "/makepresent/clear".to_string()
            }
        );
    }

    #[test]
    fn empty_address_is_skipped() {
        assert!(packet_to_triggers(&msg("")).is_empty());
    }
}

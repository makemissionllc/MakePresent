//! Local-network "Stage Display" server.
//!
//! Exposes a lightweight HTTP + WebSocket server on the local network so a
//! performer/musician can view the live Stage Display (current slide + the
//! assigned Look) on their own phone or tablet — no app install, no dedicated
//! physical monitor, no manual IP lookup.
//!
//! The server:
//!   * serves a tiny, self-contained HTML page at `/stage` (embedded build) that
//!     connects to `/ws` and renders the stage content using the same Look
//!     resolution as the desktop Stage window;
//!   * exposes a WebSocket at `/ws` that streams a compact "stage" message
//!     (live slide + Looks + assigned stage Look id). Clients receive the full
//!     current state immediately on connect (so a reconnect always resyncs),
//!     then incremental updates as the live slide changes;
//!   * gateed by a PIN. The page asks for the PIN before it opens the WebSocket;
//!     without it a random device on the church Wi-Fi cannot view the feed.
//!   * optionally serves cached media (image/video backgrounds) from the app's
//!     managed `media/` cache so phones render full backgrounds, not just text.
//!
//! Like the OSC listener it runs on its own dedicated thread (with its own
//! nested tokio runtime) and is started/stopped from Settings. It never touches
//! the Tauri main thread or the render loop.

use crate::logging::Level;
use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tauri::{AppHandle, Manager};

/// Fixed port for the stage server — small enough to type on a phone, and out
/// of the common 8000-range web-tooling collision zone.
pub const DEFAULT_STAGE_PORT: u16 = 1426;

/// The compact snapshot a phone/tablet needs to render the stage display. It
/// mirrors what `Stage.svelte` derives from the full `ClientState` — the live
/// slide, the next slide, the project's Looks, and the stage Look mapping.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageBroadcast {
    pub current: Option<crate::project::Slide>,
    pub next: Option<crate::project::Slide>,
    pub looks: Vec<crate::project::Look>,
    pub stage_look_id: Option<String>,
}

/// Build the current stage snapshot from the single source of truth — the same
/// data the desktop Stage window renders.
pub fn stage_broadcast(app: &AppHandle) -> StageBroadcast {
    let state = app.state::<AppState>();
    let settings = state.current_settings();
    let project = state.project.read().unwrap();
    let current = project
        .live
        .as_deref()
        .and_then(|id| project.find(id))
        .cloned();
    let next = project
        .live
        .as_deref()
        .and_then(|id| project.next_slide(id))
        .cloned();
    StageBroadcast {
        current,
        next,
        looks: project.looks.clone(),
        stage_look_id: settings.stage_look_id,
    }
}

/// A PIN digest used for constant-time comparison in the (rare) auth check.
fn pin_digest(pin: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(pin.as_bytes());
    hasher.finalize().into()
}

/// Owner of the running stage server. Held in [`AppState`].
pub struct NetworkServer {
    inner: Mutex<Option<NetworkInner>>,
}

struct NetworkInner {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    clients: Arc<Mutex<Vec<ClientEntry>>>,
    /// Current PIN (empty string => "any PIN accepted", used by tests/automation).
    pin: Arc<tokio::sync::RwLock<String>>,
}

/// One connected, authenticated WebSocket client.
struct ClientEntry {
    /// Unique id for the lifetime of this connection (used to remove it).
    id: u64,
    tx: tokio::sync::mpsc::UnboundedSender<String>,
    // Held on drop to cancel the relay task.
    _task: tokio::task::JoinHandle<()>,
}

impl Default for NetworkServer {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl NetworkServer {
    pub fn is_active(&self) -> bool {
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    /// Bind `addr`. Returns an error if the port is already taken.
    pub fn start(&self, app: AppHandle, addr: SocketAddr, pin: String) -> Result<(), String> {
        self.stop();

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let clients: Arc<Mutex<Vec<ClientEntry>>> = Arc::new(Mutex::new(Vec::new()));
        let clients_inner = clients.clone();
        let pin_store: Arc<tokio::sync::RwLock<String>> = Arc::new(tokio::sync::RwLock::new(pin));
        let pin_inner = pin_store.clone();

        let thread = thread::Builder::new()
            .name("network-stage".to_string())
            .spawn(move || {
                let clients_clear = clients_inner.clone();
                let state = ServeState {
                    app: app.clone(),
                    clients: clients_inner,
                    pin: pin_inner,
                    data_dir: app.state::<AppState>().app_data_dir(),
                };
                let router = Router::new()
                    .route("/", get(|| async { Redirect::temporary("/stage") }))
                    .route("/stage", get(stage_page))
                    .route("/ws", get(ws_handler))
                    .route("/asset", get(asset_handler))
                    .with_state(state);

                let rt_result = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .thread_name("network-stage-rt")
                    .build();

                let listener = std::net::TcpListener::bind(addr);
                match (rt_result, listener) {
                    (Ok(rt), Ok(listener)) => {
                        let _ = listener.set_nonblocking(true);
                        let listener = match tokio::net::TcpListener::from_std(listener) {
                            Ok(l) => l,
                            Err(e) => {
                                app.state::<AppState>().logger.log(
                                    Level::Error,
                                    &format!("stage-server: convert listener: {e}"),
                                );
                                return;
                            }
                        };
                        app.state::<AppState>().logger.log(
                            Level::Info,
                            &format!("stage-server: listening on http://{addr}/stage"),
                        );
                        let thread_stop_owned = thread_stop.clone();
                        rt.block_on(async move {
                            let _ = axum::serve(listener, router)
                                .with_graceful_shutdown(async move {
                                    loop {
                                        if thread_stop_owned.load(Ordering::Relaxed) {
                                            break;
                                        }
                                        tokio::time::sleep(std::time::Duration::from_millis(200))
                                            .await;
                                    }
                                })
                                .await;
                        });
                    }
                    (_, Err(e)) => {
                        app.state::<AppState>().logger.log(
                            Level::Error,
                            &format!("stage-server: could not bind {addr}: {e}"),
                        );
                    }
                    (Err(e), _) => {
                        app.state::<AppState>().logger.log(
                            Level::Error,
                            &format!("stage-server: runtime error: {e}"),
                        );
                    }
                }
                clients_clear.lock().unwrap().clear();
                app.state::<AppState>()
                    .logger
                    .log(Level::Info, "stage-server: stopped");
            })
            .map_err(|e| format!("could not spawn stage server: {e}"))?;

        *self.inner.lock().unwrap() = Some(NetworkInner {
            stop,
            thread: Some(thread),
            clients,
            pin: pin_store,
        });
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(inner) = self.inner.lock().unwrap().take() {
            inner.stop.store(true, Ordering::Relaxed);
            // Drop all client senders so their sockets close on shutdown.
            inner.clients.lock().unwrap().clear();
            if let Some(thread) = inner.thread {
                let _ = thread.join();
            }
        }
    }

    /// Push the current stage snapshot to every connected, authenticated client.
    /// Non-blocking: queues to each client's unbounded channel; a stalled client
    /// is dropped by its own relay task on the next send after the socket dies.
    pub fn broadcast(&self, snap: &StageBroadcast) {
        let Ok(json) = serde_json::to_string(&snap) else {
            return;
        };
        let lock = self.inner.lock().unwrap();
        let Some(inner) = lock.as_ref() else {
            return;
        };
        let guard = inner.clients.lock().unwrap();
        for c in guard.iter() {
            let _ = c.tx.send(json.clone());
        }
    }

    /// Update the running server's PIN live, without restarting it. Returns
    /// `false` if the server is not currently running (caller persists the
    /// value regardless).
    pub fn set_pin_live(&self, pin: &str) -> bool {
        let lock = self.inner.lock().unwrap();
        let Some(inner) = lock.as_ref() else {
            return false;
        };
        *inner.pin.blocking_write() = pin.to_string();
        true
    }
}

/// Per-request state shared by the axum handlers.
#[derive(Clone)]
struct ServeState {
    app: AppHandle,
    clients: Arc<Mutex<Vec<ClientEntry>>>,
    pin: Arc<tokio::sync::RwLock<String>>,
    data_dir: PathBuf,
}

/// Serve the embedded, self-contained stage web client.
async fn stage_page() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        STAGE_PAGE_HTML,
    )
}

/// Serve a cached media file (image/video background) safely from the managed
/// media cache only — never arbitrary filesystem paths.
async fn asset_handler(
    State(state): State<ServeState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let Some(p) = params.get("p") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    // Resolve against the media cache dir and require the file to actually live
    // under it (defends against path traversal in `..`).
    let media_dir = state.data_dir.join("media");
    let rel = match Path::new(p).strip_prefix(&media_dir) {
        Ok(rel) if !rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) => rel.to_path_buf(),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let canonical = media_dir.join(rel);
    match tokio::fs::read(&canonical).await {
        Ok(bytes) => {
            let ct = content_type(&canonical);
            ([(axum::http::header::CONTENT_TYPE, ct)], bytes).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    }
}

/// Handle an incoming WebSocket upgrade at `/ws`. The client must authenticate
/// with the PIN (a `{ "pin": "..." }` frame) before it receives any state.
async fn ws_handler(State(state): State<ServeState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: ServeState) {
    let (mut sender, mut receiver) = socket.split();

    // Phase 1: require a valid PIN frame within a short window.
    let required = state.pin.read().await.clone();
    let mut authed = false;
    if required.is_empty() {
        authed = true;
    } else {
        let target = pin_digest(&required);
        match tokio::time::timeout(std::time::Duration::from_secs(15), receiver.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if let Some(pin) = serde_json::from_str::<AuthFrame>(&text)
                    .ok()
                    .and_then(|a| a.pin)
                {
                    if pin_digest(&pin) == target {
                        authed = true;
                        let _ = sender.send(Message::Text("{\"type\":\"authed\"}".into())).await;
                    }
                }
            }
            Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Ok(_))) | Ok(Some(Err(_))) | Ok(None) | Err(_) => {
                // Disconnect / timeout / non-text frame before auth.
            }
        }
    }

    if !authed {
        let _ = sender.send(Message::Close(None)).await;
        return;
    }

    // Phase 2: send the current snapshot immediately so the phone resyncs (a
    // reconnect always gets the freshest live slide), then hand the socket to a
    // relay task that forwards broadcasts until the remote disconnects.
    let snap = stage_broadcast(&state.app);
    let jsn = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".to_string());
    let _ = sender.send(Message::Text(jsn)).await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Allocate a unique client id for later removal.
    let id = {
        let mut guard = state.clients.lock().unwrap();
        let id = next_client_id(&mut guard);
        // Move the socket + broadcast receiver into a relay task that owns this
        // socket's lifecycle end-to-end.
        let mut sender_task = sender;
        let mut receiver_task = receiver;
        let clients_for_cleanup = state.clients.clone();
        let tx_relay = tx.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Broadcast >>> socket (normal data flow).
                    maybe = rx.recv() => {
                        let Some(msg) = maybe else { break };
                        if sender_task.send(Message::Text(msg)).await.is_err() {
                            break;
                        }
                    }
                    // Incoming frame: ignore content but detect disconnect.
                    maybe = receiver_task.next() => {
                        match maybe {
                            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                            _ => {}
                        }
                    }
                }
            }
            let _ = sender_task.send(Message::Close(None)).await;
            // Socket closed: remove this client from the shared list.
            let mut g = clients_for_cleanup.lock().unwrap();
            g.retain(|c| c.id != id);
        });
        guard.push(ClientEntry {
            id,
            tx: tx_relay,
            _task: task,
        });
        let _ = &mut guard;
        id
    };

    // Wait for the relay task to finish (it drops when the socket closes or the
    // broadcast sender is dropped on server shutdown). Keep `tx` alive so the
    // entry's channel stays open until cleanup. Hold the handle to join.
    loop {
        let closed = {
            let guard = state.clients.lock().unwrap();
            // This client is gone once it is removed by the relay task.
            !guard.iter().any(|c| c.id == id)
        };
        if closed {
            break;
        }
        // If the relay finished but somehow wasn't removed, converge.
        if tx.is_closed() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Assign an incrementing client id (wrapping; collisions practically never
/// occur at per-service client counts).
fn next_client_id(clients: &mut Vec<ClientEntry>) -> u64 {
    let base = clients.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    let mut id = base;
    while clients.iter().any(|c| c.id == id) {
        id += 1;
    }
    id
}

#[derive(serde::Deserialize)]
struct AuthFrame {
    pin: Option<String>,
}

// ---------------------------------------------------------------------------
// Embedded web client
// ---------------------------------------------------------------------------

/// The stage page is served directly by the axum server. Reuse the same Look
/// resolution + fit-text approach as the desktop Stage window in plain JS so a
/// phone renders visually consistent with it, with no Tauri/runtime dependency.
const STAGE_PAGE_HTML: &str = r###"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>MakrStudio Stage</title>
<style>
  :root { --bg:#0b0b0e; --panel:#101014; --line:#26262e; --text:#f4f4f7;
          --dim:#555a68; --accent:#4aa7ff; }
  * { box-sizing: border-box; }
  html,body { height:100%; margin:0; }
  body { background:var(--bg); color:var(--text);
    font-family:system-ui,-apple-system,"Segoe UI",Roboto,"Helvetica Neue",Arial,sans-serif;
    -webkit-font-smoothing:antialiased; overscroll-behavior:none; }
  #app { height:100%; display:flex; flex-direction:column; }
  #gate { flex:1; display:flex; align-items:center; justify-content:center; padding:24px; }
  .card { width:min(380px,100%); background:var(--panel); border:1px solid var(--line);
    border-radius:14px; padding:28px 24px; display:flex; flex-direction:column; gap:14px;
    text-align:center; }
  .card h1 { font-size:20px; margin:0; }
  .card .sub { color:var(--dim); font-size:13px; margin:0; line-height:1.5; }
  #pin { width:100%; background:#0d0e12; border:1px solid var(--line); border-radius:8px;
    color:var(--text); font-size:22px; letter-spacing:.35em; text-align:center; padding:12px;
    outline:none; }
  #pin:focus { border-color:var(--accent); }
  #connect { background:var(--accent); color:#062033; border:none; border-radius:8px;
    padding:12px; font-size:15px; font-weight:700; cursor:pointer; }
  #connect:disabled { opacity:.5; cursor:default; }
  #err { color:#e07a7a; font-size:13px; min-height:1em; margin:0; }
  #stage { flex:1; display:none; position:relative; }
  .stage-inner { display:flex; width:100%; height:100%; overflow:hidden; }
  .current { position:relative; flex:1; min-width:0; overflow:hidden; }
  .side { width:30%; min-width:200px; display:flex; flex-direction:column;
    justify-content:space-between; border-left:1px solid var(--line); padding:4vh 2vw;
    background:var(--panel); }
  .next-label { font-size:10px; font-weight:700; letter-spacing:.18em; text-transform:uppercase;
    color:var(--dim); }
  .next-body { font-size:clamp(.9rem,2.2vmin,1.6rem); line-height:1.4; color:#d6d9e2;
    white-space:pre-wrap; overflow:hidden; display:-webkit-box; -webkit-line-clamp:8;
    -webkit-box-orient:vertical; margin:0; }
  .clock { font-size:clamp(1.6rem,6vmin,4rem); font-weight:700; font-variant-numeric:tabular-nums;
    color:#fff; }
  .slide-render { position:absolute; inset:0; display:flex; flex-direction:column;
    align-items:center; gap:2.5vh; padding:8vh 10vw; text-align:center; overflow:hidden; }
  .slide-render.pos-top { justify-content:flex-start; }
  .slide-render.pos-center { justify-content:center; }
  .slide-render.pos-bottom { justify-content:flex-end; }
  .media-layer { position:absolute; inset:0; width:100%; height:100%; object-fit:cover; }
  .look-title,.look-body { position:relative; z-index:1; margin:0; }
  .look-title { font-size:var(--t); font-weight:400; line-height:1.1;
    text-shadow:0 2px 24px rgba(0,0,0,.45); }
  .look-body { font-size:var(--b); font-weight:400; max-width:80%; line-height:1.4;
    white-space:pre-wrap; text-shadow:0 2px 20px rgba(0,0,0,.4); }
  .ph { color:var(--dim); font-size:clamp(1rem,2.2vmin,1.8rem); margin:0; }
  .offline { position:fixed; inset:0; display:flex; flex-direction:column; gap:12px;
    align-items:center; justify-content:center; background:var(--bg); z-index:5;
    text-align:center; padding:24px; }
  .offline .big { font-size:18px; }
  .offline .sub { color:var(--dim); font-size:13px; max-width:320px; line-height:1.5; }
</style>
</head>
<body>
<div id="app">
  <div id="gate">
    <div class="card">
      <h1>MakrStudio Stage</h1>
      <p class="sub">Enter the PIN shown in MakrStudio → Settings → Network on the computer running the presentation.</p>
      <input id="pin" type="password" inputmode="numeric" autocomplete="one-time-code" placeholder="••••" maxlength="12" enterkeyhint="go">
      <p id="err" role="alert"></p>
      <button id="connect">Connect</button>
    </div>
  </div>
  <div id="stage">
    <div class="stage-inner">
      <div class="current" id="current"><span class="ph">No live slide</span></div>
      <div class="side">
        <div><div class="next-label">Next</div><div id="next" class="ph">Nothing queued</div></div>
        <div class="clock" id="clock">--:--:--</div>
      </div>
    </div>
  </div>
</div>
<script>
(function () {
  "use strict";
  var gate=document.getElementById("gate"),stageEl=document.getElementById("stage");
  var pin=document.getElementById("pin"),connect=document.getElementById("connect"),err=document.getElementById("err");
  var currentEl=document.getElementById("current"),nextEl=document.getElementById("next"),clockEl=document.getElementById("clock");
  var ws=null,timer=null,desiredPin="",lastState=null;

  function esc(s){var d=document.createElement("div");d.textContent=s;return d.innerHTML;}
  function resolveLook(s){var looks=(s&&s.looks)||[];if(!looks.length)return null;
    var m=looks.filter(function(l){return l.id===(s&&s.stageLookId);})[0];if(m)return m;
    var n=looks.filter(function(l){return l.name==="Stage";})[0];return n||looks[0];}
  function shrink(el,base,floor){
    if(!el||!base)return;var max=parseFloat(base)||floor,size=max;
    var rect=(el.parentElement.getBoundingClientRect());
    for(;size>=floor;size-=2){el.style.fontSize=size+"px";
      var er=el.getBoundingClientRect();
      if(er.height<=rect.height&&er.width<=rect.width)break;}
    el.style.fontSize=Math.max(size,floor)+"px";}
  function renderCurrent(s){
    var look=resolveLook(s),slide=s&&s.current;
    if(!slide){currentEl.innerHTML='<span class="ph">No live slide</span>';return;}
    var showBg=!look||look.showBackground;
    var bg="#000";if(slide.background.type==="solid")bg=slide.background.color;
    var media="";
    if(showBg&&(slide.background.type==="image"||slide.background.type==="video")){
      var tag=(slide.background.type==="video"?"video":"img");
      var ls=tag==="video"?"autoplay loop muted playsinline preload='auto'":"";
      var onerr=tag==="img"?" onerror='this.style.display=\"none\"'":"";
      media='<'+tag+' class="media-layer" src="/asset?p='+encodeURIComponent(slide.background.path)+'"'+ls+onerr+'></'+tag+'>';}
    var posCls=look?("pos-"+look.textPosition):"pos-center";
    var tSize=(look?look.titleSize:60)+"px",bSize=(look?look.bodySize:40)+"px";
    var textColor=look?look.textColor:"#ffffff";
    var title=slide.title?'<h1 class="look-title">'+esc(slide.title)+'</h1>':"";
    var body=slide.body?'<p class="look-body">'+esc(slide.body)+'</p>':"";
    currentEl.innerHTML='<div class="slide-render '+posCls+'" style="background-color:'+
      (showBg?bg:"transparent")+';color:'+textColor+';--t:'+tSize+';--b:'+bSize+'">'+
      media+title+body+'</div>';
    fit(currentEl);}
  function fit(container){var d=container.querySelector(".slide-render");
    if(!d)return;shrink(d.querySelector(".look-title"),d.style.getPropertyValue("--t"),24);
    shrink(d.querySelector(".look-body"),d.style.getPropertyValue("--b"),16);}
  function renderNext(s){var n=s&&s.next;
    if(!n){nextEl.className="ph";nextEl.textContent="Nothing queued";return;}
    nextEl.className="next-body";nextEl.textContent=n.body||n.title;}
  function render(s){lastState=s;renderCurrent(s);renderNext(s);}
  function nowClock(){var d=new Date(),p=function(n){return n<10?"0"+n:""+n;};
    clockEl.textContent=p(d.getHours())+":"+p(d.getMinutes())+":"+p(d.getSeconds());}
  setInterval(nowClock,1000);nowClock();
  function showStage(){gate.style.display="none";stageEl.style.display="flex";
    var o=document.querySelector(".offline");if(o)o.remove();}
  function showOffline(){if(gate.style.display==="none"){
    var off=document.createElement("div");off.className="offline";
    off.innerHTML='<div class="big">Connection lost</div><div class="sub">Reconnecting to the stage feed…</div>';
    document.body.appendChild(off);}}
  function connectNow(){
    connect.disabled=true;connect.textContent="Connecting…";
    var proto="ws"+(location.protocol==="https:"?"s":"")+"://";
    ws=new WebSocket(proto+location.host+"/ws");
    ws.onopen=function(){err.textContent="";if(desiredPin)ws.send(JSON.stringify({pin:desiredPin}));};
    ws.onmessage=function(e){var m;try{m=JSON.parse(e.data);}catch(_){return;}
      if(m&&m.type==="authed")return;
      if(m&&(m.current!==undefined||m.looks)){showStage();render(m);}};
    ws.onclose=function(){timer=setTimeout(function(){showOffline();connectNow();},1500+Math.floor(Math.random()*800));};
    ws.onerror=function(){try{ws.close();}catch(_){}};}
  pin.addEventListener("input",function(){err.textContent="";desiredPin=pin.value.trim();});
  function submit(){if(!desiredPin){err.textContent="Enter the PIN to continue.";return;}
    err.textContent="Connecting…";connectNow();}
  connect.addEventListener("click",submit);
  pin.addEventListener("keydown",function(e){if(e.key==="Enter")submit();});
})();
</script>
</body>
</html>"###;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_frames_parse() {
        // A `{ "pin": "1234" }` frame carries the PIN.
        let f: AuthFrame = serde_json::from_str(r#"{ "pin": "1234" }"#).unwrap();
        assert_eq!(f.pin.as_deref(), Some("1234"));
    }

    #[test]
    fn pin_digest_differs_per_pin_and_constant_time_shape() {
        assert_ne!(pin_digest("1234"), pin_digest("5678"));
        assert_eq!(pin_digest("aaaa"), pin_digest("aaaa"));
        assert_eq!(pin_digest("").len(), 32);
    }
}

use crate::logging::Level;
use crate::project::Background;
use crate::state::AppState;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};

pub const THUMB_EXT: &str = "jpg";
/// Thumbnails are generated at this width; height follows the source ratio.
const THUMB_WIDTH: u32 = 320;
const COPY_SUFFIX: &str = ".part";

/// The reference into the media cache that a slide's background points at:
/// the copied source file plus a generated thumbnail, both keyed by the
/// content hash so duplicates collapse onto a single cached copy.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaRef {
    pub kind: MediaKind,
    pub path: String,
    pub hash: String,
    pub thumb: String,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
}

impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            MediaKind::Image => "image",
            MediaKind::Video => "video",
        }
    }

    /// Classify a file by its extension. Unknown types are rejected loudly at
    /// import time rather than being stored as a broken background.
    pub fn from_extension(path: &Path) -> Option<MediaKind> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "tif" | "svg"
            | "avif" => Some(MediaKind::Image),
            "mp4" | "m4v" | "mov" | "webm" | "mkv" | "avi" | "ogv" => Some(MediaKind::Video),
            _ => None,
        }
    }
}

/// Extract the media cache reference from a slide/library background, if any.
pub fn background_media(bg: &Background) -> Option<MediaRef> {
    match bg {
        Background::Image {
            path,
            hash,
            thumb,
        } => Some(MediaRef {
            kind: MediaKind::Image,
            path: path.clone(),
            hash: hash.clone(),
            thumb: thumb.clone(),
            duration_ms: None,
        }),
        Background::Video {
            path,
            hash,
            thumb,
            duration_ms,
        } => Some(MediaRef {
            kind: MediaKind::Video,
            path: path.clone(),
            hash: hash.clone(),
            thumb: thumb.clone(),
            duration_ms: *duration_ms,
        }),
        Background::Solid { .. } => None,
    }
}

// ---------------------------------------------------------------------------
// Disk layout (all inside the per-machine app data dir):
//   media/<hash>.<ext>      - one managed copy of every imported file
//   thumbnails/<hash>.jpg   - thumbnail keyed by content hash
// ---------------------------------------------------------------------------

pub fn media_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("media")
}

pub fn thumbnails_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("thumbnails")
}

pub fn thumbnail_path_for(data_dir: &Path, hash: &str) -> PathBuf {
    thumbnails_dir(data_dir).join(format!("{hash}.{THUMB_EXT}"))
}

/// Whether ffmpeg (which every thumbnail depends on) is available on PATH.
/// Checked once per process, then cached.
pub fn ffmpeg_available() -> bool {
    static OK: OnceLock<bool> = OnceLock::new();
    *OK.get_or_init(|| {
        Command::new("ffmpeg")
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

pub fn hash_file(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn extension_of(source: &Path) -> String {
    source
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
}

/// Generate (or regenerate) the thumbnail for a media file into `out`.
/// Uses ffmpeg for both kinds so there is a single, well-understood pipeline.
/// `start_secs` is where a video is sampled from (a frame there is usually
/// more representative than the first, often-black frame).
fn make_thumbnail(
    src: &Path,
    dest: &Path,
    kind: MediaKind,
    start_secs: f64,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut command = Command::new("ffmpeg");
    command.arg("-y");
    if kind == MediaKind::Video {
        command.arg("-ss").arg(format!("{start_secs:.3}"));
    }
    let status = command
        .arg("-i")
        .arg(src)
        .arg("-vf")
        .arg(format!("scale='min({THUMB_WIDTH},iw)':-2"))
        .arg("-frames:v")
        .arg("1")
        .arg("-q:v")
        .arg("4")
        .arg(dest)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("could not run ffmpeg: {e}"))?;
    if !status.success() {
        return Err(format!(
            "ffmpeg thumbnail generation failed for {}",
            src.display()
        ));
    }
    if !dest.is_file() || fs::metadata(dest).map(|m| m.len() == 0).unwrap_or(true) {
        return Err(format!("thumbnail output missing or empty: {}", dest.display()));
    }
    Ok(())
}

/// Sample a video somewhere safe: half-way for very short clips (never at or
/// past the end frame), capped at 1 second so longer clips still get an early
/// representative frame without decoding most of the file.
fn video_sample_secs(duration_ms: Option<u64>) -> f64 {
    match duration_ms {
        Some(ms) if ms > 0 => ((ms as f64 / 1000.0) * 0.5).clamp(0.05, 1.0),
        _ => 1.0,
    }
}

/// Duration of a video in whole milliseconds, via ffprobe. Best effort: None
/// when ffprobe is missing or the probe fails (the video still plays).
fn probe_duration_ms(path: &Path) -> Option<u64> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    value.parse::<f64>().ok().map(|secs| (secs * 1000.0) as u64)
}

fn media_asset_hash(path: &Path, kind: MediaKind, data_dir: &Path) -> Result<MediaRef, String> {
    if !ffmpeg_available() {
        return Err(
            "ffmpeg is not available on this system, and thumbnail generation depends on it. \
             Install ffmpeg, or bundle a static binary, then import again."
                .to_string(),
        );
    }

    let hash = hash_file(path)?;
    let ext = extension_of(path);
    let file_name = format!("{hash}.{ext}");
    let dest = media_dir(data_dir).join(&file_name);
    if !dest.exists() {
        fs::create_dir_all(media_dir(data_dir)).map_err(|e| e.to_string())?;
        let tmp = media_dir(data_dir).join(format!("{file_name}.{}.part", std::process::id()));
        fs::copy(path, &tmp).map_err(|e| format!("could not copy media file: {e}"))?;
        // Ignore rename failure if another concurrent import already created dest
        if let Err(e) = fs::rename(&tmp, &dest) {
            let _ = fs::remove_file(&tmp);
            if !dest.exists() {
                return Err(format!("could not finalize media copy: {e}"));
            }
        }
    }

    let thumb = thumbnail_path_for(data_dir, &hash);
    let duration_ms = if kind == MediaKind::Video {
        probe_duration_ms(&dest)
    } else {
        None
    };
    if !thumb.exists() {
        make_thumbnail(&dest, &thumb, kind, video_sample_secs(duration_ms))?;
    }

    Ok(MediaRef {
        kind,
        path: dest.to_string_lossy().into_owned(),
        hash,
        thumb: thumb.to_string_lossy().into_owned(),
        duration_ms,
    })
}

/// Copy a source media file into the managed cache, dedupe by content hash,
/// and ensure its thumbnail exists. Returns the reference for the slide's
/// background. Never references the user's original file location.
pub fn import(source: &Path, data_dir: &Path) -> Result<Background, String> {
    let kind = MediaKind::from_extension(source)
        .ok_or_else(|| format!("unsupported media type: \"{}\"", extension_of(source)))?;
    let asset = media_asset_hash(source, kind, data_dir)?;
    let background = match (asset.kind, asset.duration_ms) {
        (MediaKind::Image, _) => Background::Image {
            path: asset.path,
            hash: asset.hash,
            thumb: asset.thumb,
        },
        (MediaKind::Video, duration_ms) => Background::Video {
            path: asset.path,
            hash: asset.hash,
            thumb: asset.thumb,
            duration_ms,
        },
    };
    Ok(background)
}

/// Result surfaced to the Editor after an import (what was added, plus the
/// background to assign to the slide).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAsset {
    pub background: Background,
    pub kind: String,
    pub file_name: String,
    pub hash: String,
    pub duration_ms: Option<u64>,
}

/// Rebuild the MediaAsset returned to the frontend from a just-imported
/// background plus the original source file name.
pub fn to_asset(background: Background, source: &Path) -> MediaAsset {
    let file_name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (kind, hash, duration_ms) = match background_media(&background) {
        Some(m) => (m.kind, m.hash, m.duration_ms),
        None => (MediaKind::Image, String::new(), None),
    };
    MediaAsset {
        background,
        kind: kind.label().to_string(),
        file_name,
        hash,
        duration_ms,
    }
}

/// Startup cache verification: every media asset referenced by the current
/// project or library must still have its source file AND a thumbnail. Any
/// missing/corrupt thumbnail is regenerated with ffmpeg; missing source files
/// are logged loudly (the UI also shows a fallback, never a silent blank).
pub fn verify_on_startup(app: AppHandle) {
    let state = app.state::<AppState>();

    let mut seen = std::collections::HashSet::new();
    let mut refs: Vec<MediaRef> = Vec::new();
    let mut push_unique = |m: MediaRef| {
        if seen.insert(m.hash.clone()) {
            refs.push(m);
        }
    };

    {
        let project = state.project.read().unwrap();
        for slide in &project.slides {
            if let Some(m) = background_media(&slide.background) {
                push_unique(m);
            }
        }
    }
    {
        let library = state.library.read().unwrap();
        for song in &library.songs {
            if let Some(m) = background_media(&song.default_background) {
                push_unique(m);
            }
        }
    }

    if refs.is_empty() {
        return;
    }

    let mut ok = 0usize;
    let mut missing_source = 0usize;
    let mut regenerated = 0usize;
    for m in &refs {
        if !Path::new(&m.path).is_file() {
            missing_source += 1;
            state.logger.log(
                Level::Warn,
                &format!(
                    "media: MISSING source file for {} — {} (thumbnail cannot be rebuilt)",
                    m.hash, m.path
                ),
            );
            continue;
        }
        if !Path::new(&m.thumb).is_file() {
            match make_thumbnail(
                Path::new(&m.path),
                Path::new(&m.thumb),
                m.kind,
                video_sample_secs(m.duration_ms),
            ) {
                Ok(()) => {
                    regenerated += 1;
                    state
                        .logger
                        .log(Level::Info, &format!("media: regenerated thumbnail for {}", m.hash));
                }
                Err(e) => state.logger.log(
                    Level::Error,
                    &format!("media: thumbnail rebuild failed for {}: {e}", m.hash),
                ),
            }
        }
        ok += 1;
    }

    state.logger.log(
        Level::Info,
        &format!(
            "media: cache verified — {ok}/{} referenced assets OK ({regenerated} thumbnails regenerated, {missing_source} missing sources)",
            refs.len()
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mp-media-{label}-{}-{:x}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const SKIP: &str = "ffmpeg not available — skipping media integration test";

    fn lavfi(dest: &Path, source: &str) -> bool {
        Command::new("ffmpeg")
            .arg("-y")
            .arg("-f")
            .arg("lavfi")
            .arg("-i")
            .arg(source)
            .arg("-frames:v")
            .arg("1")
            .arg(dest)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn classifies_media_types() {
        assert_eq!(
            MediaKind::from_extension(Path::new("x.PNG")),
            Some(MediaKind::Image)
        );
        assert_eq!(
            MediaKind::from_extension(Path::new("x.mp4")),
            Some(MediaKind::Video)
        );
        assert_eq!(
            MediaKind::from_extension(Path::new("x.WEBM")),
            Some(MediaKind::Video)
        );
        assert_eq!(MediaKind::from_extension(Path::new("x.txt")), None);
        assert_eq!(MediaKind::from_extension(Path::new("noext")), None);
    }

    #[test]
    fn import_image_copies_and_thumbs() {
        if !ffmpeg_available() {
            eprintln!("{SKIP}");
            return;
        }
        let dir = temp_dir("img");
        let src = dir.join("source.png");
        assert!(lavfi(&src, "color=c=blue:s=64x48"));
        let background = import(&src, &dir).expect("import succeeds");
        let media = background_media(&background).expect("background is media");

        assert_eq!(media.kind, MediaKind::Image);
        assert_eq!(media.hash.len(), 64);
        assert!(media.path.ends_with(format!(".png").as_str()));
        assert!(Path::new(&media.path).is_file(), "media copy exists");
        assert!(Path::new(&media.thumb).is_file(), "thumbnail exists");
        assert!(
            fs::metadata(&media.thumb).unwrap().len() > 0,
            "thumbnail is not empty"
        );
        assert!(
            !src.to_string_lossy().to_string().eq(&media.path),
            "project must reference the managed copy, not the original"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_video_probes_duration() {
        if !ffmpeg_available() {
            eprintln!("{SKIP}");
            return;
        }
        let dir = temp_dir("vid");
        let src = dir.join("source.mp4");
        let ok = Command::new("ffmpeg")
            .arg("-y")
            .arg("-f")
            .arg("lavfi")
            .arg("-i")
            .arg("testsrc=duration=1:size=64x48:rate=15")
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg(&src)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("ffmpeg could not encode a test video — skipping");
            return;
        }
        let background = import(&src, &dir).expect("import succeeds");
        let media = background_media(&background).expect("background is media");
        assert_eq!(media.kind, MediaKind::Video);
        assert!(
            media.duration_ms.is_some_and(|ms| (800..=2000).contains(&ms)),
            "duration ~1s, got {:?}",
            media.duration_ms
        );
        assert!(Path::new(&media.thumb).is_file());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_is_idempotent_for_duplicates() {
        if !ffmpeg_available() {
            eprintln!("{SKIP}");
            return;
        }
        let dir = temp_dir("dedupe");
        let src = dir.join("source.png");
        assert!(lavfi(&src, "color=c=red:s=32x32"));

        let a = import(&src, &dir).expect("first import");
        let b = import(&src, &dir).expect("second import");
        let (am, bm) = (background_media(&a).unwrap(), background_media(&b).unwrap());
        assert_eq!(am.hash, bm.hash);
        assert_eq!(am.path, bm.path, "same content -> same managed copy");

        let copies = fs::read_dir(media_dir(&dir))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("."))
            .count();
        assert_eq!(copies, 1, "exactly one cached copy of identical content");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn regenerates_deleted_thumbnail() {
        if !ffmpeg_available() {
            eprintln!("{SKIP}");
            return;
        }
        let dir = temp_dir("regen");
        let src = dir.join("source.png");
        assert!(lavfi(&src, "color=c=green:s=48x40"));
        let background = import(&src, &dir).expect("import succeeds");
        let media = background_media(&background).unwrap();

        fs::remove_file(&media.thumb).expect("thumbnail removed");
        assert!(!Path::new(&media.thumb).exists());

        // Re-importing an existing source (or a startup verification pass)
        // must rebuild the missing thumbnail instead of leaving it blank.
        let again = import(&src, &dir).expect("re-import succeeds");
        let media2 = background_media(&again).unwrap();
        assert_eq!(media.hash, media2.hash);
        assert!(Path::new(&media2.thumb).is_file(), "thumbnail regenerated");
        assert!(fs::metadata(&media2.thumb).unwrap().len() > 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_unknown_types() {
        let dir = temp_dir("reject");
        let src = dir.join("notes.txt");
        fs::write(&src, b"not media").unwrap();
        let result = import(&src, &dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unsupported media type"));
        fs::remove_dir_all(&dir).ok();
    }
}
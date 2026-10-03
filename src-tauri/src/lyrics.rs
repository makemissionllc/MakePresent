//! Small LRCLIB client. Only these two fixed endpoints are reachable
//! from the renderer; callers cannot supply a URL.

use reqwest::header::RETRY_AFTER;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const BASE: &str = "https://lrclib.net/api";
const REQUEST_GAP: Duration = Duration::from_millis(400);
static NEXT_REQUEST: OnceLock<Mutex<Instant>> = OnceLock::new();

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibRecord {
    id: u64,
    track_name: String,
    artist_name: String,
    album_name: Option<String>,
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsHit {
    id: u64,
    title: String,
    artist: String,
    album: String,
    has_lyrics: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsRecord {
    id: u64,
    title: String,
    artist: String,
    album: String,
    lyrics: String,
    source: &'static str,
}

async fn request<T: serde::de::DeserializeOwned>(
    path: &str,
    query: Option<&str>,
) -> Result<T, String> {
    let gate = NEXT_REQUEST.get_or_init(|| Mutex::new(Instant::now()));
    let mut next = gate.lock().await;
    if *next > Instant::now() {
        tokio::time::sleep_until(tokio::time::Instant::from_std(*next)).await;
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("MakrStudio/0.1 (https://github.com/dwellpraise/makepresent)")
        .build()
        .map_err(|e| format!("Could not prepare lyrics search: {e}"))?;
    let mut call = client.get(format!("{BASE}{path}"));
    if let Some(q) = query {
        call = call.query(&[("q", q)]);
    }
    // Even failed connections should not produce an immediate request burst.
    *next = Instant::now() + REQUEST_GAP;
    let response = call
        .send()
        .await
        .map_err(|e| format!("Lyrics service unavailable: {e}"))?;
    let status = response.status();
    let retry_seconds = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1)
        .max(1);
    *next = Instant::now()
        + if status.as_u16() == 429 {
            Duration::from_secs(retry_seconds)
        } else {
            REQUEST_GAP
        };
    if status.as_u16() == 429 {
        return Err(format!(
            "Lyrics search is busy. Try again in {retry_seconds} seconds."
        ));
    }
    if !status.is_success() {
        return Err(format!("Lyrics service returned HTTP {status}."));
    }
    response
        .json::<T>()
        .await
        .map_err(|e| format!("Could not read lyrics results: {e}"))
}

#[tauri::command]
pub async fn search_lyrics(query: String) -> Result<Vec<LyricsHit>, String> {
    let query = query.trim();
    if query.chars().count() < 2 || query.chars().count() > 100 {
        return Err("Search using 2–100 characters.".into());
    }
    let records: Vec<LrclibRecord> = request("/search", Some(query)).await?;
    Ok(records
        .into_iter()
        .filter(|r| !r.instrumental)
        .map(|r| LyricsHit {
            id: r.id,
            title: r.track_name,
            artist: r.artist_name,
            album: r.album_name.unwrap_or_default(),
            has_lyrics: r
                .plain_lyrics
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty())
                || r.synced_lyrics
                    .as_deref()
                    .is_some_and(|s| !s.trim().is_empty()),
        })
        .collect())
}

#[tauri::command]
pub async fn get_lyrics(id: u64) -> Result<LyricsRecord, String> {
    let record: LrclibRecord = request(&format!("/get/{id}"), None).await?;
    let lyrics = record
        .plain_lyrics
        .filter(|s| !s.trim().is_empty())
        .or(record.synced_lyrics.filter(|s| !s.trim().is_empty()))
        .ok_or_else(|| "This result has no usable lyrics. Choose another recording.".to_string())?;
    if lyrics.chars().count() > 40_000 {
        return Err("This lyric is too long to format as slides.".into());
    }
    Ok(LyricsRecord {
        id: record.id,
        title: record.track_name,
        artist: record.artist_name,
        album: record.album_name.unwrap_or_default(),
        lyrics,
        source: "LRCLIB",
    })
}

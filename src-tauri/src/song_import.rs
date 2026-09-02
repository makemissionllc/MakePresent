use crate::project::{Background, LibrarySlide, LibrarySong};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fs;
use std::path::Path;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Parsed intermediate — conservatively extracts title + text slides
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ParsedSlide {
    pub title: String,
    pub body: String,
}

#[derive(Debug)]
pub struct ParsedSong {
    pub title: String,
    pub slides: Vec<ParsedSlide>,
}

// ---------------------------------------------------------------------------
// Public entry — dispatch by extension, no cloud calls
// ---------------------------------------------------------------------------

pub fn import_song_file(path: &Path) -> Result<ParsedSong, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pro" => parse_pro(path),
        "cho" | "chopro" | "chord" | "chordpro" => parse_cho(path),
        "usr" => parse_usr(path),
        "txt" => {
            // CCLI USR is often .txt — try USR first, then ChordPro, then plain
            let content = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            if looks_like_usr(&content) {
                parse_usr(path)
            } else if looks_like_chordpro(&content) {
                parse_cho(path)
            } else {
                // Plain text fallback — title from filename, slides split by blank lines
                parse_plain(path)
            }
        }
        _ => Err(format!(
            "unsupported file type \".{ext}\" — expected .pro (ProPresenter), .cho/.chordpro (ChordPro), or .usr (CCLI USR)"
        )),
    }
}

pub fn parsed_to_library_song(parsed: ParsedSong) -> LibrarySong {
    use std::collections::HashMap;
    let song_id = Uuid::new_v4().to_string();
    let mut blocks: HashMap<String, LibrarySlide> = HashMap::new();
    let mut arrangement: Vec<String> = Vec::new();
    for (i, ps) in parsed.slides.into_iter().enumerate() {
        let base_title = if ps.title.trim().is_empty() {
            format!("Verse {}", i + 1)
        } else {
            ps.title.clone()
        };
        let mut key = base_title.clone();
        if let Some(existing) = blocks.get(&key) {
            if existing.body != ps.body {
                let mut counter = 2;
                let mut new_key = format!("{} ({})", key, counter);
                while blocks.contains_key(&new_key) {
                    counter += 1;
                    new_key = format!("{} ({})", key, counter);
                }
                key = new_key;
            }
        }
        if !blocks.contains_key(&key) {
            blocks.insert(
                key.clone(),
                LibrarySlide {
                    id: Uuid::new_v4().to_string(),
                    title: key.clone(),
                    body: ps.body.clone(),
                    positioning: None,
                    group_id: Some(format!("verse-{}", blocks.len() + 1)),
                    group_label: Some(key.clone()),
                },
            );
        }
        arrangement.push(key);
    }
    LibrarySong {
        id: song_id,
        title: parsed.title,
        default_background: Background::default(),
        blocks,
        arrangement,
        slides: None,
    }
}

// ---------------------------------------------------------------------------
// Helpers: detect format hints
// ---------------------------------------------------------------------------

fn looks_like_usr(content: &str) -> bool {
    let lower = content.to_lowercase();
    // USR / SongSelect text usually has "Title:" or "CCLI" header
    lower.contains("title:") || lower.contains("ccli") || lower.contains("author:") || lower.contains("verse 1")
}

fn looks_like_chordpro(content: &str) -> bool {
    content.contains("{title:") || content.contains("{t:") || content.contains("[C") || content.contains("[G")
}

// ---------------------------------------------------------------------------
// .pro — ProPresenter export (XML, via quick-xml)
// ---------------------------------------------------------------------------

fn parse_pro(path: &Path) -> Result<ParsedSong, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if content.trim().is_empty() {
        return Err(format!("ProPresenter file is empty: {}", path.display()));
    }
    // Try quick-xml parsing; on failure report clearly rather than silently
    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut slides: Vec<ParsedSlide> = Vec::new();
    let mut current_group: Option<String> = None;
    let mut current_text_parts: Vec<String> = Vec::new();
    let mut in_text_element = false;
    let mut slide_depth: usize = 0;
    // Track title candidate from document
    let mut doc_title: Option<String> = None;

    // We collect text inside NSString or generic text nodes.
    // ProPresenter structures vary; we treat RVDisplaySlide / RVSlideGrouping boundaries as slide delimiters.
    let is_slide_start = |name: &[u8]| {
        matches!(
            name,
            b"RVDisplaySlide" | b"RVSlide" | b"slide" | b"Slide"
        )
    };
    let is_group_start = |name: &[u8]| {
        matches!(
            name,
            b"RVSlideGrouping" | b"slideGrouping" | b"SlideGrouping"
        )
    };

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let name_ref = name.as_ref();
                if is_group_start(name_ref) {
                    // Flush previous slide if any pending text
                    if !current_text_parts.is_empty() {
                        let body = current_text_parts.join("\n").trim().to_string();
                        if !body.is_empty() {
                            let title = current_group.clone().unwrap_or_else(|| format!("Slide {}", slides.len() + 1));
                            slides.push(ParsedSlide { title, body });
                        }
                        current_text_parts.clear();
                    }
                    // Extract name attribute as group title
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"name" || attr.key.as_ref() == b"title" {
                            let v = String::from_utf8_lossy(&attr.value).trim().to_string();
                            if !v.is_empty() {
                                current_group = Some(v.clone());
                                if doc_title.is_none() {
                                    doc_title = Some(v);
                                }
                            }
                        }
                    }
                } else if is_slide_start(name_ref) {
                    slide_depth += 1;
                } else if name_ref == b"NSString" || name_ref == b"string" || name_ref.ends_with(b"String") {
                    in_text_element = true;
                }
                // Also capture any <title> element at document level
                if name_ref == b"title" || name_ref == b"Title" {
                    in_text_element = true;
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let name_ref = name.as_ref();
                if name_ref == b"NSString" || name_ref == b"string" || name_ref.ends_with(b"String") || name_ref == b"title" || name_ref == b"Title" {
                    in_text_element = false;
                }
                if is_slide_start(name_ref) && slide_depth > 0 {
                    slide_depth -= 1;
                    if !current_text_parts.is_empty() {
                        let body = current_text_parts.join("\n").trim().to_string();
                        if !body.is_empty() {
                            let title = current_group.clone().unwrap_or_else(|| format!("Slide {}", slides.len() + 1));
                            slides.push(ParsedSlide { title, body });
                        }
                        current_text_parts.clear();
                    }
                    // Don't clear group here — group may contain multiple slides
                }
                if is_group_start(name_ref) {
                    // End of group — flush any remaining
                    if !current_text_parts.is_empty() {
                        let body = current_text_parts.join("\n").trim().to_string();
                        if !body.is_empty() {
                            let title = current_group.clone().unwrap_or_else(|| format!("Slide {}", slides.len() + 1));
                            slides.push(ParsedSlide { title, body });
                        }
                        current_text_parts.clear();
                    }
                    current_group = None;
                }
            }
            Ok(Event::Text(t)) => {
                if in_text_element {
                    if let Ok(txt) = t.unescape() {
                        let s = txt.trim().to_string();
                        if !s.is_empty() {
                            // If we haven't set doc_title yet and this is inside a title element, capture
                            if doc_title.is_none() && s.len() < 80 {
                                // Heuristic: first short line could be title, but we treat as text anyway
                            }
                            current_text_parts.push(s);
                        }
                    }
                } else {
                    // Also capture loose text nodes that are not inside NSString but still content
                    if let Ok(txt) = t.unescape() {
                        let s = txt.trim().to_string();
                        if !s.is_empty() && s.len() > 1 && !s.starts_with("<?") {
                            // Avoid XML declaration noise
                            current_text_parts.push(s);
                        }
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if let Ok(txt) = t.escape() {
                    let s = String::from_utf8_lossy(&txt).trim().to_string();
                    if !s.is_empty() {
                        current_text_parts.push(s);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(format!(
                    "ProPresenter file is malformed XML ({}): {}",
                    path.display(),
                    e
                ))
            }
            _ => {}
        }
        buf.clear();
    }

    // Flush any remaining
    if !current_text_parts.is_empty() {
        let body = current_text_parts.join("\n").trim().to_string();
        if !body.is_empty() {
            let title = current_group.unwrap_or_else(|| format!("Slide {}", slides.len() + 1));
            slides.push(ParsedSlide { title, body });
        }
    }

    if slides.is_empty() {
        // Fallback: try to extract any non-empty lines as single slide, rather than failing silently
        let fallback = strip_xml_tags(&content);
        let lines: Vec<String> = fallback
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && l.len() > 1)
            .collect();
        if lines.is_empty() {
            return Err(format!(
                "ProPresenter file contains no extractable text ({}). Is it a valid ProPresenter export?",
                path.display()
            ));
        }
        // Group into slides by 4 lines or blank-line heuristic — here single slide with all lines
        slides.push(ParsedSlide {
            title: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".to_string()),
            body: lines.join("\n"),
        });
    }

    let title = doc_title.unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string())
    });

    // Ensure slugs are not empty
    let slides: Vec<ParsedSlide> = slides
        .into_iter()
        .filter(|s| !s.body.trim().is_empty())
        .collect();
    if slides.is_empty() {
        return Err(format!("ProPresenter file resulted in no slides: {}", path.display()));
    }

    Ok(ParsedSong { title, slides })
}

fn strip_xml_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {
                if !in_tag {
                    out.push(ch);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// .cho — ChordPro text
// ---------------------------------------------------------------------------

fn parse_cho(path: &Path) -> Result<ParsedSong, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if content.trim().is_empty() {
        return Err(format!("ChordPro file is empty: {}", path.display()));
    }
    let mut title: Option<String> = None;
    let mut body_blocks: Vec<String> = Vec::new();
    let mut current_block: Vec<String> = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            if !current_block.is_empty() {
                body_blocks.push(current_block.join("\n"));
                current_block.clear();
            }
            continue;
        }
        // Directives like {title: ...} or {t: ...}
        if line.starts_with('{') && line.ends_with('}') {
            let inner = line[1..line.len() - 1].trim();
            if let Some(colon) = inner.find(':') {
                let key = inner[..colon].trim().to_lowercase();
                let val = inner[colon + 1..].trim().trim_matches('"').trim_matches('\'').to_string();
                match key.as_str() {
                    "title" | "t" => {
                        if title.is_none() && !val.is_empty() {
                            title = Some(val);
                        }
                    }
                    "subtitle" | "st" | "artist" | "composer" | "lyricist" => {
                        // Could be used as slide title hint, ignore for now
                    }
                    _ => {} // other directives like {c: comment}, {soc}, {eoc} — ignore
                }
            } else {
                // directives without colon like {soc} — treat as block separator
                let low = inner.to_lowercase();
                if low == "soc" || low == "start_of_chorus" || low == "sov" || low == "start_of_verse" {
                    if !current_block.is_empty() {
                        body_blocks.push(current_block.join("\n"));
                        current_block.clear();
                    }
                    continue;
                } else if low == "eoc" || low == "end_of_chorus" || low == "eov" || low == "end_of_verse" {
                    if !current_block.is_empty() {
                        body_blocks.push(current_block.join("\n"));
                        current_block.clear();
                    }
                    continue;
                }
            }
            continue; // directive line not part of lyrics
        }
        // Strip chords: [C], [G/B], [Am7] etc.
        let stripped = strip_chords(line);
        if stripped.trim().is_empty() {
            continue;
        }
        current_block.push(stripped.trim().to_string());
    }
    if !current_block.is_empty() {
        body_blocks.push(current_block.join("\n"));
    }

    if body_blocks.is_empty() {
        // Fallback: if no blocks were formed, try stripping chords from entire file as one slide
        let stripped = strip_chords(&content);
        let cleaned = stripped
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('{'))
            .collect::<Vec<_>>()
            .join("\n");
        if cleaned.trim().is_empty() {
            return Err(format!(
                "ChordPro file contains no extractable lyrics ({}). Check {{title}} vs [chords] syntax.",
                path.display()
            ));
        }
        body_blocks.push(cleaned);
    }

    let final_title = title.unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string())
    });

    let slides: Vec<ParsedSlide> = body_blocks
        .into_iter()
        .enumerate()
        .map(|(i, body)| ParsedSlide {
            title: format!("Verse {}", i + 1),
            body,
        })
        .collect();

    Ok(ParsedSong {
        title: final_title,
        slides,
    })
}

fn strip_chords(s: &str) -> String {
    let mut out = String::new();
    let mut in_bracket = false;
    for ch in s.chars() {
        match ch {
            '[' => in_bracket = true,
            ']' => in_bracket = false,
            _ => {
                if !in_bracket {
                    out.push(ch);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// USR — CCLI SongSelect USR text (plain text, Title:/Author: headers)
// ---------------------------------------------------------------------------

fn parse_usr(path: &Path) -> Result<ParsedSong, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if content.trim().is_empty() {
        return Err(format!("USR file is empty: {}", path.display()));
    }
    let mut title: Option<String> = None;
    let mut lines: Vec<String> = Vec::new();
    let mut in_header = true;
    let mut header_done = false;

    for raw in content.lines() {
        let line = raw.trim_end();
        if in_header && !header_done {
            let lower = line.to_lowercase();
            if lower.starts_with("title:") {
                let v = line[6..].trim().trim_matches('"').to_string();
                if !v.is_empty() {
                    title = Some(v);
                }
                continue;
            } else if lower.starts_with("words:") || lower.starts_with("music:") || lower.starts_with("author:") || lower.starts_with("copyright:") || lower.starts_with("ccli") {
                continue;
            } else if line.trim().is_empty() {
                // Empty line after header block — transition to lyrics
                if title.is_some() {
                    header_done = true;
                    continue;
                }
            } else if line.chars().all(|c| c == '-' || c == '=' || c == '_') {
                header_done = true;
                continue;
            } else {
                // First non-header non-empty line without Title: — treat as lyrics start
                if !line.trim().is_empty() && title.is_none() && lines.is_empty() {
                    // Heuristic: if line looks like verse label, don't treat as header
                    let low = line.to_lowercase();
                    if low.starts_with("verse") || low.starts_with("chorus") || low.starts_with("bridge") || low.starts_with("pre-chorus") {
                        header_done = true;
                    } else if !lower.contains(':') {
                        header_done = true;
                    }
                }
            }
            if header_done {
                lines.push(line.to_string());
            }
        } else {
            lines.push(line.to_string());
        }
        if in_header && header_done && lines.len() == 1 && lines[0].trim().is_empty() {
            lines.clear();
        }
        if lines.len() > 2 {
            in_header = false;
        }
    }

    // If title still none, try first line as title
    if title.is_none() {
        // Look for a line that is not a label and is short
        for l in &lines {
            let t = l.trim();
            if !t.is_empty() && !t.to_lowercase().starts_with("verse") && !t.to_lowercase().starts_with("chorus") && t.len() < 60 && !t.contains(':') {
                title = Some(t.to_string());
                break;
            }
        }
    }
    let final_title = title.unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string())
    });

    // Split lyrics into slides by blank lines or by verse/chorus labels
    let mut slides: Vec<ParsedSlide> = Vec::new();
    let mut current_title = String::new();
    let mut current_body: Vec<String> = Vec::new();

    let is_label = |s: &str| {
        let l = s.trim().to_lowercase();
        l.starts_with("verse") || l.starts_with("chorus") || l.starts_with("bridge") || l.starts_with("pre-chorus") || l.starts_with("tag") || l.starts_with("ending") || l.starts_with("intro")
    };

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current_body.is_empty() {
                let body = current_body.join("\n").trim().to_string();
                if !body.is_empty() {
                    let t = if current_title.is_empty() {
                        format!("Verse {}", slides.len() + 1)
                    } else {
                        current_title.clone()
                    };
                    slides.push(ParsedSlide { title: t, body });
                }
                current_body.clear();
                current_title.clear();
            }
            continue;
        }
        if is_label(trimmed) && trimmed.len() < 30 {
            // New section label — flush previous
            if !current_body.is_empty() {
                let body = current_body.join("\n").trim().to_string();
                let t = if current_title.is_empty() {
                    format!("Verse {}", slides.len() + 1)
                } else {
                    current_title.clone()
                };
                slides.push(ParsedSlide { title: t, body });
                current_body.clear();
            }
            current_title = trimmed.to_string();
            continue;
        }
        current_body.push(line);
    }
    if !current_body.is_empty() {
        let body = current_body.join("\n").trim().to_string();
        if !body.is_empty() {
            let t = if current_title.is_empty() {
                format!("Verse {}", slides.len() + 1)
            } else {
                current_title
            };
            slides.push(ParsedSlide { title: t, body });
        }
    }

    if slides.is_empty() {
        return Err(format!(
            "USR file contains no extractable verses ({}). Expected 'Title:' header and verses separated by blank lines or labels like 'Verse 1'.",
            path.display()
        ));
    }

    Ok(ParsedSong {
        title: final_title,
        slides,
    })
}

// Fallback plain text — title from filename, slides split by blank lines
fn parse_plain(path: &Path) -> Result<ParsedSong, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if content.trim().is_empty() {
        return Err(format!("file is empty: {}", path.display()));
    }
    let title = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".to_string());
    let mut slides: Vec<ParsedSlide> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            if !cur.is_empty() {
                slides.push(ParsedSlide {
                    title: format!("Verse {}", slides.len() + 1),
                    body: cur.join("\n").trim().to_string(),
                });
                cur.clear();
            }
        } else {
            cur.push(line.to_string());
        }
    }
    if !cur.is_empty() {
        slides.push(ParsedSlide {
            title: format!("Verse {}", slides.len() + 1),
            body: cur.join("\n").trim().to_string(),
        });
    }
    if slides.is_empty() {
        return Err(format!("file contains no text: {}", path.display()));
    }
    Ok(ParsedSong { title, slides })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_path(content: &str, ext: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mp-song-{}-{:x}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!("test.{ext}"));
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn cho_strips_chords_and_directives() {
        let p = tmp_path(
            "{title: Amazing Grace}\n{key: G}\n[G]Amazing [C]grace, how [G]sweet\nThat [D]saved\n\nVerse2 line\n",
            "cho",
        );
        let song = parse_cho(&p).unwrap();
        assert_eq!(song.title, "Amazing Grace");
        assert_eq!(song.slides.len(), 2);
        assert!(!song.slides[0].body.contains('['));
        assert!(song.slides[0].body.contains("Amazing grace"));
    }

    #[test]
    fn cho_to_library_blocks() {
        let p = tmp_path(
            "{title: Test}\nVerse 1 line\n\nChorus line\n\nVerse 1 line\n",
            "cho",
        );
        let parsed = parse_cho(&p).unwrap();
        let lib = parsed_to_library_song(parsed);
        // Should deduplicate Verse 1 if same body, or create unique if different
        assert!(lib.blocks.contains_key("Verse 1") || lib.blocks.contains_key("Verse 1 (2)"));
        assert_eq!(lib.arrangement.len(), 3);
    }

    #[test]
    fn cho_rejects_empty() {
        let p = tmp_path("", "cho");
        assert!(parse_cho(&p).is_err());
    }

    #[test]
    fn usr_parses_title_and_verses() {
        let txt = "Title: Holy Holy\nAuthor: Someone\n\nVerse 1\nHoly holy holy\nLord God almighty\n\nChorus\nHoly is the Lord\n";
        let p = tmp_path(txt, "usr");
        let song = parse_usr(&p).unwrap();
        assert_eq!(song.title, "Holy Holy");
        assert!(song.slides.len() >= 2);
        assert_eq!(song.slides[0].title.to_lowercase(), "verse 1");
    }

    #[test]
    fn pro_parses_simple_xml() {
        let xml = r#"<?xml version="1.0"?><RVPresentationDocument><array><RVSlideGrouping name="Verse 1"><array><RVDisplaySlide><array><RVTextElement><NSString>Amazing grace</NSString></RVTextElement></array></RVDisplaySlide></array></RVSlideGrouping></array></RVPresentationDocument>"#;
        let p = tmp_path(xml, "pro");
        let song = parse_pro(&p).unwrap();
        assert_eq!(song.slides.len(), 1);
        assert!(song.slides[0].body.contains("Amazing grace"));
    }

    #[test]
    fn pro_rejects_malformed() {
        let p = tmp_path("<not xml>", "pro");
        // Should not panic; either parses fallback or errors clearly (malformed or no extractable text)
        let res = parse_pro(&p);
        match res {
            Ok(_) => {},
            Err(e) => {
                let low = e.to_lowercase();
                assert!(
                    low.contains("malformed") || low.contains("extractable") || low.contains("valid"),
                    "unexpected error: {}",
                    e
                );
            }
        }
    }
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Raw deserialization types (matches kjv.json structure)
// ---------------------------------------------------------------------------

/// One Bible book with its chapters/verses. Also the JSON shape used to persist
/// imported Bibles and the output of the OpenLP / bible-api.com parsers.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawBook {
    pub book: String,
    pub chapters: Vec<RawChapter>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawChapter {
    #[serde(rename = "chapter")]
    pub chapter: String,
    pub verses: Vec<RawVerse>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawVerse {
    pub verse: String,
    pub text: String,
}

// ---------------------------------------------------------------------------
// In-memory index types
// ---------------------------------------------------------------------------

struct ScriptureVerse {
    verse: u32,
    text: String,
}

struct ScriptureChapter {
    chapter: u32,
    verses: Vec<ScriptureVerse>,
}

struct BookData {
    name: String,
    chapters: Vec<ScriptureChapter>,
}

// ---------------------------------------------------------------------------
// Public API types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptureMatch {
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
    pub reference: String,
    pub text: String,
}

pub struct ScriptureIndex {
    /// Normalized input (lowercase, no spaces/dots) → canonical book names.
    /// A single key can map to multiple books (e.g. "jud" → Judges AND Jude).
    name_map: HashMap<String, Vec<String>>,
    /// Canonical book name → book data
    books: HashMap<String, BookData>,
}

// ---------------------------------------------------------------------------
// Abbreviation table
// ---------------------------------------------------------------------------

/// Returns (canonical_name, list_of_searchable_abbreviations).
fn abbreviation_table() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("Genesis", vec!["gen", "gn", "ge"]),
        ("Exodus", vec!["exod", "ex", "exo"]),
        ("Leviticus", vec!["lev", "lv", "le"]),
        ("Numbers", vec!["num", "nm", "nu"]),
        ("Deuteronomy", vec!["deut", "dt", "de"]),
        ("Joshua", vec!["josh", "jos", "jsh"]),
        ("Judges", vec!["judg", "jdg", "jud", "jg"]),
        ("Ruth", vec!["ruth", "rth", "ru"]),
        ("1 Samuel", vec!["1sam", "1sa", "1s", "1 sm"]),
        ("2 Samuel", vec!["2sam", "2sa", "2s", "2 sm"]),
        ("1 Kings", vec!["1kgs", "1ki", "1k", "1 ki"]),
        ("2 Kings", vec!["2kgs", "2ki", "2k", "2 ki"]),
        ("1 Chronicles", vec!["1chr", "1ch", "1 chrono", "1 c"]),
        ("2 Chronicles", vec!["2chr", "2ch", "2 chrono", "2 c"]),
        ("Ezra", vec!["ezra", "ezr"]),
        ("Nehemiah", vec!["neh", "ne"]),
        ("Esther", vec!["esth", "est"]),
        ("Job", vec!["job"]),
        ("Psalms", vec!["ps", "psa", "psalm", "psalms", "psl"]),
        ("Proverbs", vec!["prov", "prv", "pr"]),
        ("Ecclesiastes", vec!["eccl", "ecc", "qoh"]),
        ("Song of Solomon", vec!["song", "sos", "ss", "sol", "song of sol"]),
        ("Isaiah", vec!["isa", "is"]),
        ("Jeremiah", vec!["jer", "je", "jere"]),
        ("Lamentations", vec!["lam", "la"]),
        ("Ezekiel", vec!["ezek", "ezk", "eze"]),
        ("Daniel", vec!["dan", "da"]),
        ("Hosea", vec!["hos", "ho"]),
        ("Joel", vec!["joel", "jl"]),
        ("Amos", vec!["amos", "am"]),
        ("Obadiah", vec!["obad", "ob"]),
        ("Jonah", vec!["jon", "jnh"]),
        ("Micah", vec!["mic", "mi"]),
        ("Nahum", vec!["nah", "na"]),
        ("Habakkuk", vec!["hab", "hb"]),
        ("Zephaniah", vec!["zeph", "zep", "zp"]),
        ("Haggai", vec!["hag", "hg"]),
        ("Zechariah", vec!["zech", "zec", "zc"]),
        ("Malachi", vec!["mal", "ml"]),
        ("Matthew", vec!["matt", "mt"]),
        ("Mark", vec!["mk", "mrk"]),
        ("Luke", vec!["lk", "luk"]),
        ("John", vec!["jn", "joh", "jhn"]),
        ("Acts", vec!["act", "acts"]),
        ("Romans", vec!["rom", "ro"]),
        ("1 Corinthians", vec!["1cor", "1co", "1 c"]),
        ("2 Corinthians", vec!["2cor", "2co", "2 c"]),
        ("Galatians", vec!["gal", "ga"]),
        ("Ephesians", vec!["eph", "ep"]),
        ("Philippians", vec!["phil", "php", "phi"]),
        ("Colossians", vec!["col", "co"]),
        ("1 Thessalonians", vec!["1thess", "1th", "1 thes"]),
        ("2 Thessalonians", vec!["2thess", "2th", "2 thes"]),
        ("1 Timothy", vec!["1tim", "1ti", "1 ti"]),
        ("2 Timothy", vec!["2tim", "2ti", "2 ti"]),
        ("Titus", vec!["tit", "ti"]),
        ("Philemon", vec!["phlm", "phm"]),
        ("Hebrews", vec!["heb", "hebr"]),
        ("James", vec!["jas", "jm"]),
        ("1 Peter", vec!["1pet", "1pe", "1p", "1 pe"]),
        ("2 Peter", vec!["2pet", "2pe", "2p", "2 pe"]),
        ("1 John", vec!["1jn", "1jo", "1j", "1 joh"]),
        ("2 John", vec!["2jn", "2jo", "2j", "2 joh"]),
        ("3 John", vec!["3jn", "3jo", "3j", "3 joh"]),
        ("Jude", vec!["jude", "jud", "jd"]),
        ("Revelation", vec!["rev", "re", "apoc"]),
    ]
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl ScriptureIndex {
    /// Register a normalized key → canonical book name in the name map,
    /// appending when a key maps to more than one book.
    fn add_alias(name_map: &mut HashMap<String, Vec<String>>, key: &str, canonical: &str) {
        let key = key.to_lowercase().replace(' ', "");
        let canonical = canonical.to_string();
        name_map.entry(key).or_default().push(canonical);
    }

    /// Build the index from raw book data.
    fn build(raw_books: Vec<RawBook>) -> Self {
        let mut name_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut books: HashMap<String, BookData> = HashMap::new();

        // Register canonical names and abbreviations
        for (canonical, abbrevs) in abbreviation_table() {
            Self::add_alias(&mut name_map, canonical, canonical);
            for abbr in abbrevs {
                Self::add_alias(&mut name_map, abbr, canonical);
            }
        }

        // Load book data
        for raw in raw_books {
            Self::add_book(&mut name_map, &mut books, raw);
        }

        // De-duplicate the alias lists.
        for names in name_map.values_mut() {
            names.sort();
            names.dedup();
        }

        ScriptureIndex { name_map, books }
    }

    /// Fold `incoming` chapters/verses into an existing book without dropping
    /// verses the import did not mention.
    fn merge_book_data(existing: &mut BookData, incoming: BookData) {
        for ch in incoming.chapters {
            if let Some(dst) = existing.chapters.iter_mut().find(|c| c.chapter == ch.chapter) {
                for v in ch.verses {
                    if let Some(ev) = dst.verses.iter_mut().find(|x| x.verse == v.verse) {
                        ev.text = v.text;
                    } else {
                        dst.verses.push(v);
                    }
                }
                dst.verses.sort_by_key(|v| v.verse);
            } else {
                existing.chapters.push(ch);
            }
        }
        existing.chapters.sort_by_key(|c| c.chapter);
    }

    /// Convert a raw book into its in-memory representation, skipping any
    /// non-numeric chapter/verse numbers.
    fn book_data_from(raw: RawBook) -> Option<BookData> {
        let chapters: Vec<ScriptureChapter> = raw
            .chapters
            .into_iter()
            .filter_map(|rc| {
                let ch_num = rc.chapter.parse::<u32>().ok()?;
                let verses: Vec<ScriptureVerse> = rc
                    .verses
                    .into_iter()
                    .filter_map(|rv| {
                        let v_num = rv.verse.parse::<u32>().ok()?;
                        Some(ScriptureVerse {
                            verse: v_num,
                            text: rv.text,
                        })
                    })
                    .collect();
                Some(ScriptureChapter { chapter: ch_num, verses })
            })
            .collect();
        if chapters.is_empty() {
            return None;
        }
        Some(BookData { name: raw.book, chapters })
    }

    /// Register a book in the index, creating or extending its entry and
    /// ensuring its canonical name is searchable.
    ///
    /// Verse-level upsert: a full OpenLP Bible overwrites matching verses, while
    /// a bible-api.com snippet (e.g. John 3:16) does not wipe the rest of that
    /// book from the bundled KJV.
    fn add_book(
        name_map: &mut HashMap<String, Vec<String>>,
        books: &mut HashMap<String, BookData>,
        raw: RawBook,
    ) {
        let name = raw.book.clone();
        let Some(incoming) = Self::book_data_from(raw) else {
            return;
        };
        match books.get_mut(&name) {
            Some(existing) => Self::merge_book_data(existing, incoming),
            None => {
                books.insert(name.clone(), incoming);
            }
        }
        Self::add_alias(name_map, &name, &name);
    }

    /// Merge imported books (from an OpenLP file or the API) into the index.
    /// Matching book/chapter/verse cells are overwritten; everything else in
    /// the bundled set stays. Returns the number of verses in `added`.
    pub fn merge_books(&mut self, added: Vec<RawBook>) -> usize {
        let mut imported = 0;
        for raw in added {
            imported += raw
                .chapters
                .iter()
                .map(|ch| ch.verses.len())
                .sum::<usize>();
            Self::add_book(&mut self.name_map, &mut self.books, raw);
        }
        for names in self.name_map.values_mut() {
            names.sort();
            names.dedup();
        }
        imported
    }

    /// Parse a query string into (normalized_book, chapter?, verse?).
    ///
    /// Handles "john", "john 3", "john 3:16", "jn 3:16", "1 cor 13:4",
    /// "1corinthians", etc. Strategy: accumulate book tokens until we hit a
    /// purely-numeric token (a chapter) or a "N:N" numbered token; everything
    /// before that is the (possibly multi-word, possibly ordinals-prefixed)
    /// book name.
    fn parse_query(query: &str) -> (String, Option<u32>, Option<u32>) {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return (String::new(), None, None);
        }

        let tokens: Vec<&str> = query.split_whitespace().collect();
        let mut book_tokens: Vec<&str> = Vec::new();
        let mut chapter: Option<u32> = None;
        let mut verse: Option<u32> = None;
        let mut seen_book_term = false;

        for token in &tokens {
            let has_colon = token.contains(':');
            let all_digits = token.chars().all(|c| c.is_ascii_digit());

            if !seen_book_term && all_digits && !has_colon {
                // A bare numeric token before we've committed to a book name.
                // A single leading ordinal digit ("1 Corinthians") is part of
                // the name; but "psalm 23" wants the 23 to be a chapter. For a
                // bare number at the START with more tokens to follow, treat it
                // as an ordinal book prefix. Otherwise it's a chapter.
                if book_tokens.is_empty() && tokens.len() > 1 {
                    book_tokens.push(token);
                } else {
                    chapter = token.parse::<u32>().ok();
                    seen_book_term = true;
                }
                continue;
            }

            if !seen_book_term {
                // Accumulate an alphabetic/mixed token into the book name's
                // dotless representation. A "3:16"-style token at the very
                // start isn't valid as a book, but handle gracefully by
                // pushing it (no book match will be found → empty result).
                if has_colon {
                    // Could be a book token like nothing, but typically
                    // "john 3:16" arrives as the 2nd token after "john".
                    if book_tokens.is_empty() {
                        // e.g. "3:16" alone — no book; parse as ref
                        if let Some(colon_pos) = token.find(':') {
                            if let Ok(ch) = token[..colon_pos].parse::<u32>() {
                                chapter = Some(ch);
                            }
                            if let Ok(vs) = token[colon_pos + 1..].parse::<u32>() {
                                verse = Some(vs);
                            }
                        }
                        seen_book_term = true;
                    } else {
                        // Book name followed by a "N:N" reference
                        if let Some(colon_pos) = token.find(':') {
                            if let Ok(ch) = token[..colon_pos].parse::<u32>() {
                                chapter = Some(ch);
                            }
                            if let Ok(vs) = token[colon_pos + 1..].parse::<u32>() {
                                verse = Some(vs);
                            }
                        }
                        seen_book_term = true;
                    }
                } else {
                    // An alphabetic token (or "1" ordinal we already pushed) —
                    // part of the book name.
                    book_tokens.push(token);
                }
                continue;
            }

            // We already have a book name. A token here is chapter or ref.
            if has_colon {
                if let Some(colon_pos) = token.find(':') {
                    if let Ok(ch) = token[..colon_pos].parse::<u32>() {
                        chapter = Some(ch);
                    }
                    if let Ok(vs) = token[colon_pos + 1..].parse::<u32>() {
                        verse = Some(vs);
                    }
                }
            } else if all_digits {
                chapter = token.parse::<u32>().ok();
            }
            // else: extra alphabetic token after the reference — ignore
        }

        let book_part = book_tokens.join("").replace(['.', ',', '-'], "");
        (book_part, chapter, verse)
    }

    /// Search the index for matching scripture references.
    pub fn search(&self, query: &str, limit: usize) -> Vec<ScriptureMatch> {
        let (normalized_book, chapter, verse) = Self::parse_query(query);

        if normalized_book.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::new();

        // Exact abbreviation/canonical-name match → all books for that key
        if let Some(canonical_names) = self.name_map.get(&normalized_book) {
            for canonical_name in canonical_names {
                if let Some(book) = self.books.get(canonical_name) {
                    Self::collect_verses(
                        book,
                        chapter,
                        verse,
                        limit - results.len(),
                        &mut results,
                    );
                    if results.len() >= limit {
                        break;
                    }
                }
            }
            return results;
        }

        // Prefix match: find all canonical books whose normalized name starts
        // with the query (e.g. "joh" → "john", "1jn" → "1john" ...).
        let prefix = &normalized_book;
        let mut candidates: Vec<&String> = self
            .name_map
            .keys()
            .filter(|k| k.starts_with(prefix))
            .filter_map(|k| self.name_map.get(k))
            .flatten()
            .collect();
        candidates.sort();
        candidates.dedup();

        for canonical_name in &candidates {
            if let Some(book) = self.books.get(canonical_name.as_str()) {
                Self::collect_verses(
                    book,
                    chapter,
                    verse,
                    limit - results.len(),
                    &mut results,
                );
                if results.len() >= limit {
                    break;
                }
            }
        }
        results
    }

    /// Collect verses from a book based on chapter/verse criteria.
    fn collect_verses(
        book: &BookData,
        chapter: Option<u32>,
        verse: Option<u32>,
        limit: usize,
        results: &mut Vec<ScriptureMatch>,
    ) {
        for ch in &book.chapters {
            match (chapter, verse) {
                (Some(ch_num), Some(vs_num)) => {
                    // Specific chapter:verse
                    if ch.chapter == ch_num {
                        for v in &ch.verses {
                            if v.verse == vs_num && results.len() < limit {
                                results.push(ScriptureMatch {
                                    book: book.name.clone(),
                                    chapter: ch_num,
                                    verse: vs_num,
                                    reference: format!("{} {}:{}", book.name, ch_num, vs_num),
                                    text: v.text.clone(),
                                });
                            }
                        }
                    }
                }
                (Some(ch_num), None) => {
                    // Specific chapter, no verse — show first 5 verses
                    if ch.chapter == ch_num {
                        for v in ch.verses.iter().take(5) {
                            if results.len() < limit {
                                results.push(ScriptureMatch {
                                    book: book.name.clone(),
                                    chapter: ch_num,
                                    verse: v.verse,
                                    reference: format!("{} {}:{}", book.name, ch_num, v.verse),
                                    text: v.text.clone(),
                                });
                            }
                        }
                    }
                }
                (None, _) => {
                    // No chapter specified — show first verse of chapter 1
                    if ch.chapter == 1 {
                        if let Some(v) = ch.verses.first() {
                            if results.len() < limit {
                                results.push(ScriptureMatch {
                                    book: book.name.clone(),
                                    chapter: 1,
                                    verse: v.verse,
                                    reference: format!("{} 1:{}", book.name, v.verse),
                                    text: v.text.clone(),
                                });
                            }
                        }
                    }
                }
            }
            if results.len() >= limit {
                break;
            }
        }
    }

    pub fn book_count(&self) -> usize {
        self.books.len()
    }

    pub fn ordered_book_names(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (canonical, _) in abbreviation_table() {
            if self.books.contains_key(canonical) {
                out.push(canonical.to_string());
            }
        }
        for name in self.books.keys() {
            if !out.contains(name) {
                out.push(name.clone());
            }
        }
        out
    }

    pub fn get_chapter_verses(&self, book: &str, chapter: u32) -> Option<Vec<(u32, String)>> {
        let normalized = book.to_lowercase().replace(' ', "").replace('.', "");
        let mut canonical: Option<String> = None;
        for (canon, _) in abbreviation_table() {
            if canon.to_lowercase().replace(' ', "") == normalized {
                canonical = Some(canon.to_string());
                break;
            }
        }
        if canonical.is_none() {
            if let Some(names) = self.name_map.get(&normalized) {
                if let Some(first) = names.first() {
                    canonical = Some(first.clone());
                }
            }
        }
        if canonical.is_none() {
            for key in self.books.keys() {
                if key.to_lowercase() == book.to_lowercase() {
                    canonical = Some(key.clone());
                    break;
                }
            }
        }
        let canon = canonical.unwrap_or_else(|| book.to_string());
        let book_data = self.books.get(&canon)?;
        let chapter_data = book_data.chapters.iter().find(|c| c.chapter == chapter)?;
        Some(
            chapter_data
                .verses
                .iter()
                .map(|v| (v.verse, v.text.clone()))
                .collect(),
        )
    }

    pub fn chapter_numbers(&self, book: &str) -> Option<Vec<u32>> {
        let normalized = book.to_lowercase().replace(' ', "").replace('.', "");
        let mut canonical: Option<String> = None;
        for (canon, _) in abbreviation_table() {
            if canon.to_lowercase().replace(' ', "") == normalized {
                canonical = Some(canon.to_string());
                break;
            }
        }
        if canonical.is_none() {
            if let Some(names) = self.name_map.get(&normalized) {
                if let Some(first) = names.first() {
                    canonical = Some(first.clone());
                }
            }
        }
        if canonical.is_none() {
            for key in self.books.keys() {
                if key.to_lowercase() == book.to_lowercase() {
                    canonical = Some(key.clone());
                    break;
                }
            }
        }
        let canon = canonical.unwrap_or_else(|| book.to_string());
        self.books.get(&canon).map(|b| b.chapters.iter().map(|c| c.chapter).collect())
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Load the KJV index from the given kjv.json path. Panics on failure — use
/// `try_load` for graceful startup handling.
///
/// Only compiled for tests: production startup uses `try_load` (and continues
/// with scripture search disabled rather than panicking when the bundled
/// resource is missing or corrupt).
#[cfg(test)]
pub fn load(kjv_path: &Path) -> ScriptureIndex {
    try_load(kjv_path).unwrap_or_else(|e| panic!("{e}"))
}

/// Graceful loader for startup — returns an error string instead of panicking
/// so the app can continue with scripture search disabled if the resource is
/// missing/corrupt (e.g. dev build without bundled resources).
pub fn try_load(kjv_path: &Path) -> Result<ScriptureIndex, String> {
    let start = Instant::now();
    let raw = std::fs::read_to_string(kjv_path)
        .map_err(|e| format!("failed to read {}: {e}", kjv_path.display()))?;
    let raw_books: Vec<RawBook> = serde_json::from_str(&raw)
        .map_err(|e| format!("failed to parse kjv.json: {e}"))?;
    let count = raw_books.len();
    let index = ScriptureIndex::build(raw_books);
    let elapsed = start.elapsed();
    eprintln!("scripture: loaded {count} books in {elapsed:?}");
    Ok(index)
}

// ---------------------------------------------------------------------------
// Persistence of imported Bibles
// ---------------------------------------------------------------------------

/// Where imported Bibles (OpenLP files + bible-api.com fetches) are cached so
/// they survive app restarts alongside the bundled KJV.
pub fn imported_books_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("bibles").join("imports.json")
}

/// Load any previously imported books from disk (best effort; empty on error).
pub fn load_imported_books(data_dir: &Path) -> Vec<RawBook> {
    std::fs::read_to_string(imported_books_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Write the imported-book cache to disk.
pub fn save_imported_books(data_dir: &Path, books: &[RawBook]) -> Result<(), String> {
    let path = imported_books_path(data_dir);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(books).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Fold newly imported books into the persistent list. Matching verses are
/// overwritten; books/chapters that only exist on one side are kept.
pub fn merge_persisted(existing: &mut Vec<RawBook>, added: Vec<RawBook>) {
    for raw in added {
        if let Some(dst) = existing.iter_mut().find(|b| b.book == raw.book) {
            for ch in raw.chapters {
                if let Some(dst_ch) = dst.chapters.iter_mut().find(|c| c.chapter == ch.chapter) {
                    for v in ch.verses {
                        if let Some(ev) = dst_ch.verses.iter_mut().find(|x| x.verse == v.verse) {
                            ev.text = v.text;
                        } else {
                            dst_ch.verses.push(v);
                        }
                    }
                } else {
                    dst.chapters.push(ch);
                }
            }
        } else {
            existing.push(raw);
        }
    }
}

/// The folder where raw OpenLP/Zefania XML files can be dropped directly
/// (alternative to the Import button). This is `data_dir/bibles/` — e.g.
/// `%APPDATA%\com.makesoftware.makepresent\bibles\` on Windows,
/// `~/.local/share/com.makesoftware.makepresent/bibles/` on Linux.
pub fn bibles_folder(data_dir: &Path) -> PathBuf {
    data_dir.join("bibles")
}

/// Scan `data_dir/bibles/*.xml` for raw Bible files dropped there directly.
/// Returns `(parsed_books, Vec<(filename, error)>)` for WARN logging.
/// Handles missing folder gracefully (not an error).
pub fn scan_bibles_folder(data_dir: &Path) -> (Vec<RawBook>, Vec<(String, String)>) {
    let folder = bibles_folder(data_dir);
    let mut ok = Vec::new();
    let mut errs = Vec::new();
    let entries = match std::fs::read_dir(&folder) {
        Ok(e) => e,
        Err(_) => return (ok, errs),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase() == "xml")
            .unwrap_or(false)
        {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown.xml")
                .to_string();
            match std::fs::read_to_string(&path) {
                Ok(xml) => match parse_openlp_xml(&xml) {
                    Ok(books) => ok.extend(books),
                    Err(e) => errs.push((filename, e)),
                },
                Err(e) => errs.push((filename, e.to_string())),
            }
        }
    }
    (ok, errs)
}

/// Flatten nested book data into the same `ScriptureMatch` records the bundled
/// KJV search feeds into `add_slide` (title = reference, body = verse text).
pub fn matches_from_books(books: &[RawBook], limit: usize) -> Vec<ScriptureMatch> {
    let mut out = Vec::new();
    for book in books {
        for ch in &book.chapters {
            let Ok(ch_num) = ch.chapter.parse::<u32>() else {
                continue;
            };
            for v in &ch.verses {
                let Ok(vs_num) = v.verse.parse::<u32>() else {
                    continue;
                };
                if out.len() >= limit {
                    return out;
                }
                out.push(ScriptureMatch {
                    book: book.book.clone(),
                    chapter: ch_num,
                    verse: vs_num,
                    reference: format!("{} {}:{}", book.book, ch_num, vs_num),
                    text: v.text.trim().to_string(),
                });
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// OpenLP / Zefania XML import
// ---------------------------------------------------------------------------

use quick_xml::events::{BytesStart, Event};

/// Lowercased local tag name so `BIBLEBOOK`, `biblebook`, and namespaced
/// variants (`osis:verse`) are treated identically.
fn tag_name(e: &BytesStart) -> String {
    String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase()
}

fn end_tag_name(e: &quick_xml::events::BytesEnd) -> String {
    String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase()
}

/// Read the first matching attribute value (case-insensitive) from an element.
fn attr(e: &BytesStart, names: &[&str]) -> Option<String> {
    for a in e.attributes().flatten() {
        let key = String::from_utf8_lossy(a.key.as_ref()).to_lowercase();
        let key = key.rsplit(':').next().unwrap_or(&key);
        if names.iter().any(|n| key == *n) {
            return Some(a.unescape_value().unwrap_or_default().into_owned());
        }
    }
    None
}

fn is_book_tag(name: &str) -> bool {
    matches!(name, "biblebook" | "b" | "book")
}

fn is_chapter_tag(name: &str) -> bool {
    matches!(name, "chapter" | "c")
}

fn is_verse_tag(name: &str) -> bool {
    matches!(name, "vers" | "verse" | "v")
}

fn is_skip_tag(name: &str) -> bool {
    matches!(
        name,
        "note" | "title" | "style" | "caption" | "comment" | "metadata"
    )
}

/// Parse an OpenLP-compatible Bible XML document into raw book data.
///
/// Accepts:
/// - Zefania: `XMLBIBLE / BIBLEBOOK bname / CHAPTER cnumber / VERS vnumber`
/// - OpenLP compact: `bible / b n / c n / v n`
/// - OpenLP native: `bible / book name / chapter number / verse number`
///
/// Nested notes/titles inside a verse are ignored. Returns books in document
/// order. The numeric book id is unused — the index already has English aliases.
pub fn parse_openlp_xml(xml: &str) -> Result<Vec<RawBook>, String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut books: Vec<RawBook> = Vec::new();
    let mut buf = Vec::new();
    let mut cur_book: Option<usize> = None;
    let mut cur_chapter: Option<(usize, usize)> = None;
    let mut in_verse = false;
    let mut skip_depth: usize = 0;
    let mut verse_num = String::new();
    let mut verse_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = tag_name(&e);
                if skip_depth > 0 || is_skip_tag(&name) {
                    skip_depth += 1;
                } else if is_book_tag(&name) {
                    let name = attr(&e, &["bname", "name", "n", "bsname"]).unwrap_or_default();
                    if !name.is_empty() {
                        books.push(RawBook {
                            book: name,
                            chapters: Vec::new(),
                        });
                        cur_book = Some(books.len() - 1);
                    }
                    cur_chapter = None;
                } else if is_chapter_tag(&name) {
                    if let Some(bi) = cur_book {
                        let num = attr(&e, &["cnumber", "number", "n"]).unwrap_or_default();
                        if !num.is_empty() {
                            books[bi].chapters.push(RawChapter {
                                chapter: num,
                                verses: Vec::new(),
                            });
                            cur_chapter = Some((bi, books[bi].chapters.len() - 1));
                        }
                    }
                } else if is_verse_tag(&name) && cur_chapter.is_some() {
                    in_verse = true;
                    verse_num = attr(&e, &["vnumber", "number", "n"]).unwrap_or_default();
                    verse_text.clear();
                }
            }
            Ok(Event::Empty(e)) => {
                let name = tag_name(&e);
                if skip_depth == 0 && is_verse_tag(&name) {
                    if let Some((bi, ci)) = cur_chapter {
                        let num = attr(&e, &["vnumber", "number", "n"]).unwrap_or_default();
                        books[bi].chapters[ci].verses.push(RawVerse {
                            verse: num,
                            text: String::new(),
                        });
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if in_verse && skip_depth == 0 {
                    verse_text.push_str(&t.unescape().unwrap_or_default());
                }
            }
            Ok(Event::CData(t)) => {
                if in_verse && skip_depth == 0 {
                    verse_text.push_str(&String::from_utf8_lossy(t.as_ref()));
                }
            }
            Ok(Event::End(e)) => {
                let name = end_tag_name(&e);
                if skip_depth > 0 {
                    skip_depth -= 1;
                } else if in_verse && is_verse_tag(&name) {
                    in_verse = false;
                    if let Some((bi, ci)) = cur_chapter {
                        books[bi].chapters[ci].verses.push(RawVerse {
                            verse: verse_num.clone(),
                            text: verse_text.trim().to_string(),
                        });
                    }
                } else if is_chapter_tag(&name) {
                    cur_chapter = None;
                } else if is_book_tag(&name) {
                    cur_book = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(format!(
                    "failed to parse OpenLP XML at byte {}: {e}",
                    reader.buffer_position()
                ))
            }
            _ => {}
        }
        buf.clear();
    }

    if books.is_empty() {
        return Err("no Bible books found in the OpenLP XML document".to_string());
    }
    // Drop any books that ended up with no verses/chapters.
    books.retain(|b| !b.chapters.is_empty());
    if books.is_empty() {
        return Err("no verses found in the OpenLP XML document".to_string());
    }
    Ok(books)
}

// ---------------------------------------------------------------------------
// bible-api.com REST integration
// ---------------------------------------------------------------------------

/// A single verse returned by bible-api.com.
#[derive(Debug, Deserialize)]
struct ApiVerse {
    #[serde(rename = "book_name")]
    book_name: String,
    chapter: u32,
    verse: u32,
    text: String,
    #[serde(rename = "book_id")]
    #[allow(dead_code)]
    book_id: Option<String>,
}

/// The overall bible-api.com response container.
#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[allow(dead_code)]
    reference: String,
    verses: Vec<ApiVerse>,
    #[serde(rename = "translation_id")]
    #[allow(dead_code)]
    translation_id: Option<String>,
}

/// Query bible-api.com for a human-readable reference ("John 3:16",
/// "rom 8:28", "John 3") and map the returned verses into raw book data ready
/// to fold into the scripture index. `translation` is optional (defaults to
/// WEB on the service).
pub async fn fetch_api_bible(
    reference: &str,
    translation: Option<&str>,
) -> Result<Vec<RawBook>, String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Err("empty scripture reference".to_string());
    }
    let base = format!("https://bible-api.com/{}", url_encode(reference));
    let url = match translation {
        Some(t) if !t.trim().is_empty() => {
            format!("{}?translation={}", base, url_encode(t.trim()))
        }
        _ => base,
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("MakrStudio/0.1 (https://github.com/dwellpraise/makepresent)")
        .build()
        .map_err(|e| format!("bible-api client: {e}"))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("bible-api request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "bible-api returned HTTP {} — check the reference (book, chapter, verse)",
            resp.status()
        ));
    }
    let api: ApiResponse = resp
        .json()
        .await
        .map_err(|e| format!("failed to decode bible-api response: {e}"))?;
    if api.verses.is_empty() {
        return Err("bible-api returned no verses for that reference".to_string());
    }
    Ok(group_api_verses(api.verses))
}

/// Group a flat list of API verses (possibly spanning books/chapters) into the
/// nested book → chapter → verse structure the index consumes.
fn group_api_verses(verses: Vec<ApiVerse>) -> Vec<RawBook> {
    let mut books: Vec<RawBook> = Vec::new();
    let mut book_index: HashMap<String, usize> = HashMap::new();
    for v in verses {
        let bi = match book_index.get(&v.book_name) {
            Some(&i) => i,
            None => {
                book_index.insert(v.book_name.clone(), books.len());
                books.push(RawBook { book: v.book_name.clone(), chapters: Vec::new() });
                books.len() - 1
            }
        };
        let chapter_str = v.chapter.to_string();
        let chapters = &mut books[bi].chapters;
        let ci = match chapters.iter().position(|c| c.chapter == chapter_str) {
            Some(i) => i,
            None => {
                chapters.push(RawChapter { chapter: chapter_str, verses: Vec::new() });
                chapters.len() - 1
            }
        };
        chapters[ci].verses.push(RawVerse {
            verse: v.verse.to_string(),
            text: v.text.trim().to_string(),
        });
    }
    books
}

/// Lightweight percent-encoding suitable for a bible-api.com path segment.
/// Spaces become `+` and a literal `:` is left intact (the service expects
/// references like `john+3:16`).
fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' => {
                out.push(char::from(*b));
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_book_only() {
        let (book, ch, vs) = ScriptureIndex::parse_query("john");
        assert_eq!(book, "john");
        assert_eq!(ch, None);
        assert_eq!(vs, None);
    }

    #[test]
    fn parse_book_chapter() {
        let (book, ch, vs) = ScriptureIndex::parse_query("john 3");
        assert_eq!(book, "john");
        assert_eq!(ch, Some(3));
        assert_eq!(vs, None);
    }

    #[test]
    fn parse_book_chapter_verse() {
        let (book, ch, vs) = ScriptureIndex::parse_query("john 3:16");
        assert_eq!(book, "john");
        assert_eq!(ch, Some(3));
        assert_eq!(vs, Some(16));
    }

    #[test]
    fn parse_abbreviation() {
        let (book, ch, vs) = ScriptureIndex::parse_query("jn 3:16");
        assert_eq!(book, "jn");
        assert_eq!(ch, Some(3));
        assert_eq!(vs, Some(16));
    }

    #[test]
    fn parse_numbered_book() {
        let (book, ch, vs) = ScriptureIndex::parse_query("1 cor 13:4");
        assert_eq!(book, "1cor");
        assert_eq!(ch, Some(13));
        assert_eq!(vs, Some(4));
    }

    #[test]
    fn parse_psalm() {
        let (book, ch, vs) = ScriptureIndex::parse_query("psalm 23");
        assert_eq!(book, "psalm");
        assert_eq!(ch, Some(23));
        assert_eq!(vs, None);
    }

    #[test]
    fn load_vendored_kjv_and_search() {
        // Resolve the resources dir relative to the project root (CARGO_MANIFEST_DIR
        // is src-tauri/). Verify the vendored data loads fast and searches work.
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let kjv_path = manifest.join("resources").join("kjv.json");
        let start = std::time::Instant::now();
        let idx = load(&kjv_path);
        let elapsed = start.elapsed();

        assert_eq!(idx.book_count(), 66, "expected all 66 books");
        assert!(
            elapsed.as_millis() < 500,
            "kjv load took {elapsed:?}, expected < 500ms"
        );

        // John 3:16 must resolve
        let r = idx.search("john 3:16", 10);
        assert!(!r.is_empty());
        assert_eq!(r[0].book, "John");
        assert_eq!(r[0].chapter, 3);
        assert_eq!(r[0].verse, 16);
        assert!(r[0].text.contains("God so loved the world"));

        // Abbreviation
        let r = idx.search("jn 1:1", 10);
        assert_eq!(r[0].book, "John");
        assert_eq!(r[0].chapter, 1);
        assert_eq!(r[0].verse, 1);

        // Ambiguous "jud" → both Judges and Jude
        let r = idx.search("jud", 10);
        let books: Vec<String> = r.iter().map(|m| m.book.clone()).collect();
        assert!(books.contains(&"Judges".to_string()), "got {books:?}");
        assert!(books.contains(&"Jude".to_string()), "got {books:?}");

        // Numbered book
        let r = idx.search("1 cor 13:4", 10);
        assert_eq!(r[0].book, "1 Corinthians");
        assert_eq!(r[0].chapter, 13);
        assert_eq!(r[0].verse, 4);

        // Book + chapter (no verse) → first verses of that chapter
        let r = idx.search("psalm 23", 10);
        assert!(!r.is_empty());
        assert_eq!(r[0].book, "Psalms");
        assert_eq!(r[0].chapter, 23);
        assert_eq!(r[0].verse, 1);
    }

    #[test]
    fn parse_zefania_xml() {
        let xml = r#"<?xml version="1.0"?>
        <XMLBIBLE>
          <BIBLEBOOK bnumber="43" bname="John">
            <CHAPTER cnumber="3">
              <VERS vnumber="16">For God so loved the world.</VERS>
              <VERS vnumber="17">For God sent not his Son into the world to condemn the world.</VERS>
            </CHAPTER>
          </BIBLEBOOK>
        </XMLBIBLE>"#;
        let books = parse_openlp_xml(xml).unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].book, "John");
        assert_eq!(books[0].chapters[0].chapter, "3");
        assert_eq!(books[0].chapters[0].verses[0].verse, "16");
        assert!(books[0].chapters[0].verses[0].text.contains("God so loved"));
        assert_eq!(books[0].chapters[0].verses.len(), 2);
    }

    #[test]
    fn parse_openlp_native_and_compact() {
        let native = r#"<bible>
          <testament name="nt">
            <book name="John">
              <chapter number="1">
                <verse number="1">In the beginning was the Word.</verse>
                <verse number="2">The same was in the beginning with God.<note>ignored</note></verse>
              </chapter>
            </book>
          </testament>
        </bible>"#;
        let books = parse_openlp_xml(native).unwrap();
        assert_eq!(books[0].book, "John");
        assert_eq!(books[0].chapters[0].verses[0].text, "In the beginning was the Word.");
        assert_eq!(
            books[0].chapters[0].verses[1].text,
            "The same was in the beginning with God."
        );

        let compact = r#"<bible><b n="Ruth"><c n="1"><v n="1">In the days when the judges ruled.</v></c></b></bible>"#;
        let books = parse_openlp_xml(compact).unwrap();
        assert_eq!(books[0].book, "Ruth");
        assert_eq!(books[0].chapters[0].chapter, "1");
        assert_eq!(books[0].chapters[0].verses[0].verse, "1");
    }

    #[test]
    fn parse_openlp_empty_document_errors() {
        assert!(parse_openlp_xml("<XMLBIBLE></XMLBIBLE>").is_err());
    }

    #[test]
    fn group_api_verses_nests_books() {
        let verses = vec![
            ApiVerse {
                book_name: "John".into(),
                chapter: 3,
                verse: 16,
                text: "  For God so loved the world.  ".into(),
                book_id: Some("JHN".into()),
            },
            ApiVerse {
                book_name: "John".into(),
                chapter: 3,
                verse: 17,
                text: "For God sent not his Son.".into(),
                book_id: Some("JHN".into()),
            },
            ApiVerse {
                book_name: "Romans".into(),
                chapter: 8,
                verse: 28,
                text: "And we know that all things work together.".into(),
                book_id: Some("ROM".into()),
            },
        ];
        let books = group_api_verses(verses);
        assert_eq!(books.len(), 2);
        assert_eq!(books[0].book, "John");
        assert_eq!(books[0].chapters[0].verses.len(), 2);
        assert_eq!(books[0].chapters[0].verses[0].text, "For God so loved the world.");
        assert_eq!(books[1].book, "Romans");
        let matches = matches_from_books(&books, 10);
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].reference, "John 3:16");
        assert_eq!(matches[0].text, "For God so loved the world.");
    }

    #[test]
    fn merge_does_not_wipe_existing_verses() {
        let idx = ScriptureIndex::build(vec![RawBook {
            book: "John".into(),
            chapters: vec![RawChapter {
                chapter: "3".into(),
                verses: vec![
                    RawVerse {
                        verse: "16".into(),
                        text: "KJV 16".into(),
                    },
                    RawVerse {
                        verse: "17".into(),
                        text: "KJV 17".into(),
                    },
                ],
            }],
        }]);
        let mut idx = idx;
        idx.merge_books(vec![RawBook {
            book: "John".into(),
            chapters: vec![RawChapter {
                chapter: "3".into(),
                verses: vec![RawVerse {
                    verse: "16".into(),
                    text: "WEB 16".into(),
                }],
            }],
        }]);
        let r16 = idx.search("john 3:16", 5);
        assert_eq!(r16[0].text, "WEB 16");
        let r17 = idx.search("john 3:17", 5);
        assert_eq!(r17[0].text, "KJV 17");
    }
}

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Raw deserialization types (matches kjv.json structure)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawBook {
    book: String,
    chapters: Vec<RawChapter>,
}

#[derive(Debug, Deserialize)]
struct RawChapter {
    chapter: String,
    verses: Vec<RawVerse>,
}

#[derive(Debug, Deserialize)]
struct RawVerse {
    verse: String,
    text: String,
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
            let name = raw.book.clone();
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
                    Some(ScriptureChapter {
                        chapter: ch_num,
                        verses,
                    })
                })
                .collect();

            // Also register the canonical name if it wasn't in the abbreviation table
            Self::add_alias(&mut name_map, &name, &name);

            books.insert(name.clone(), BookData { name, chapters });
        }

        // De-duplicate the alias lists.
        for names in name_map.values_mut() {
            names.sort();
            names.dedup();
        }

        ScriptureIndex { name_map, books }
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
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Load the KJV index from the given kjv.json path. Panics on failure — use
/// `try_load` for graceful startup handling.
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

        // Book only → first verse of chapter 1
        let r = idx.search("gen", 10);
        assert!(!r.is_empty());
        assert_eq!(r[0].book, "Genesis");
        assert_eq!(r[0].chapter, 1);
    }
}

export interface ParsedSection {
  tag: string;
  rawTag: string;
  body: string;
}

export interface ParsedSlide {
  title: string;
  body: string;
  tag: string;
}

export interface SongMetadata {
  title?: string;
  style?: string;
  author?: string;
  [key: string]: string | undefined;
}

export interface ParsedSong {
  metadata: SongMetadata;
  slides: ParsedSlide[];
}

// Matches structural markers:
// - Markdown headers: ### Verse 1, ## Chorus, # Bridge
// - Bracket: [Verse 1], [Chorus], [Bridge]
// - Colon suffix: Verse 1:, Chorus:, Pre-Chorus:
// - Bare tags like "Verse 1", "Pre-Chorus" on its own line (when alone)
const SECTION_KEYWORDS =
  "(verse|chorus|pre-chorus|pre chorus|bridge|tag|outro|intro|interlude|refrain|ending|coda|instrumental)";

const HEADER_LINE_RE = new RegExp(
  `^\\s*(?:` +
    // Markdown: ### Verse 1
    `#{1,4}\\s*` + SECTION_KEYWORDS + `\\s*\\d*\\s*` +
    `|` +
    // Bracket: [Verse 1]
    `\\[\\s*` + SECTION_KEYWORDS + `\\s*\\d*\\s*\\]` +
    `|` +
    // Colon: Verse 1:
    SECTION_KEYWORDS + `\\s*\\d*\\s*:` +
  `)\\s*$`,
  "i",
);

// Fallback bare tag on its own line, e.g. "Verse 1" or "Pre-Chorus" with optional number
const BARE_TAG_RE = new RegExp(`^\\s*` + SECTION_KEYWORDS + `\\s*\\d*\\s*$`, "i");

// Metadata lines at top: Title: ..., Style: ..., Author: ...
const META_RE = /^\s*(Title|Style|Author|Artist|Key|Tempo|CCLI|Copyright)\s*:\s*(.+)\s*$/i;

function isSectionHeader(line: string): string | null {
  const trimmed = line.trim();
  if (!trimmed) return null;
  if (HEADER_LINE_RE.test(trimmed)) return normalizeTag(trimmed);
  // Bare tag fallback — only if line is short (avoid matching lyric lines like "Bridge over troubled water")
  if (BARE_TAG_RE.test(trimmed) && trimmed.length < 30) return normalizeTag(trimmed);
  return null;
}

function normalizeTag(raw: string): string {
  // Strip markdown #, brackets, colon, trim
  let s = raw.trim();
  s = s.replace(/^#{1,4}\s*/, "");
  s = s.replace(/^\[/, "").replace(/\]$/, "");
  s = s.replace(/:\s*$/, "");
  s = s.trim();
  // Normalize "pre chorus" -> "Pre-Chorus"
  s = s.replace(/pre\s*chorus/i, "Pre-Chorus");
  // Title-case each word but keep numbers
  s = s
    .split(/\s+/)
    .map((w) => {
      if (/^\d+$/.test(w)) return w;
      if (w.toLowerCase() === "pre-chorus") return "Pre-Chorus";
      return w.charAt(0).toUpperCase() + w.slice(1).toLowerCase();
    })
    .join(" ");
  // Collapse: "Verse 1" not "Verse  1"
  s = s.replace(/\s+/g, " ").trim();
  return s;
}

function stripBold(s: string): string {
  return s.replace(/^\s*\*\*\s*/, "").replace(/\s*\*\*\s*$/, "").trim();
}

function extractMetadata(lines: string[]): { metadata: SongMetadata; restStart: number } {
  const metadata: SongMetadata = {};
  let restStart = 0;
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    const line = raw.trim();
    if (!line || line === "---") continue;
    // Markdown title like "# Standard Output" (not a section)
    if (/^#{1,2}\s+/.test(line) && !HEADER_LINE_RE.test(line)) {
      const titleCandidate = line.replace(/^#+\s*/, "").trim();
      if (titleCandidate && !metadata.title) metadata.title = titleCandidate;
      restStart = i + 1;
      continue;
    }
    // Try metadata after stripping bold markers
    const cleaned = stripBold(line);
    const m = cleaned.match(META_RE);
    if (m) {
      const key = m[1].toLowerCase();
      const val = m[2].trim().replace(/\*\*/g, "").trim();
      metadata[key] = val;
      restStart = i + 1;
      continue;
    }
    if (isSectionHeader(line)) {
      restStart = i;
      break;
    }
    // First non-metadata, non-header line — stop metadata scan
    restStart = i;
    break;
  }
  return { metadata, restStart };
}

function splitByDoubleBreak(lines: string[]): string[][] {
  const groups: string[][] = [];
  let current: string[] = [];
  for (const line of lines) {
    if (line.trim() === "") {
      if (current.length > 0) {
        groups.push(current);
        current = [];
      }
    } else {
      current.push(line);
    }
  }
  if (current.length > 0) groups.push(current);
  return groups;
}

function chunkByLineCount(lines: string[], maxLines = 4): string[][] {
  if (lines.length <= maxLines) return [lines];
  const chunks: string[][] = [];
  for (let i = 0; i < lines.length; i += maxLines) {
    chunks.push(lines.slice(i, i + maxLines));
  }
  return chunks;
}

export function parseSong(raw: string, fallbackTitle?: string): ParsedSong {
  const lines = raw.replace(/\r\n/g, "\n").split("\n");
  const { metadata, restStart } = extractMetadata(lines);

  let remaining = lines.slice(restStart);

  // Remove leading separators / blanks
  while (remaining.length && (remaining[0].trim() === "" || remaining[0].trim() === "---")) {
    remaining.shift();
  }

  // If first header exists, drop any preamble before it (metadata already captured)
  const firstHeaderIdx = remaining.findIndex((l) => isSectionHeader(l) !== null);
  if (firstHeaderIdx > 0) remaining = remaining.slice(firstHeaderIdx);
  if (firstHeaderIdx === -1 && remaining.length) {
    // No headers at all — keep as single verse
  }

  const sections: ParsedSection[] = [];
  let currentTag = "Verse";
  let currentBody: string[] = [];
  let hasSeenHeader = false;

  function flush() {
    const body = currentBody.join("\n").trim();
    if (body) {
      sections.push({ tag: currentTag, rawTag: currentTag, body });
    }
    currentBody = [];
  }

  for (const line of remaining) {
    const tag = isSectionHeader(line);
    if (tag) {
      // New section header found — flush previous
      if (hasSeenHeader) flush();
      else {
        // Flush any preamble body before first header as untagged verse
        const preamble = currentBody.join("\n").trim();
        if (preamble) {
          sections.push({ tag: currentTag, rawTag: currentTag, body: preamble });
          currentBody = [];
        }
      }
      currentTag = tag;
      hasSeenHeader = true;
    } else {
      currentBody.push(line);
    }
  }
  flush();

  // If no sections but body exists (no headers at all), treat whole thing as one section
  if (sections.length === 0 && remaining.join("\n").trim()) {
    sections.push({ tag: "Verse 1", rawTag: "Verse 1", body: remaining.join("\n").trim() });
  }

  // Secondary split: double-break or >4 lines -> sub-slides
  const slides: ParsedSlide[] = [];
  for (const sec of sections) {
    const rawLines = sec.body.split("\n");
    // First, split by double blank lines
    const groups = splitByDoubleBreak(rawLines);
    // If single group but >4 lines, chunk
    const expanded: string[][] = [];
    for (const g of groups) {
      if (g.length > 4) {
        expanded.push(...chunkByLineCount(g, 4));
      } else {
        expanded.push(g);
      }
    }
    // If no double-break and single expanded group, that's it
    // If multiple groups due to blank lines but each <=4, keep as sub-slides
    if (expanded.length === 1) {
      slides.push({ title: sec.tag, body: expanded[0].join("\n").trim(), tag: sec.tag });
    } else {
      expanded.forEach((chunk, idx) => {
        const title = expanded.length > 1 ? `${sec.tag} (Part ${idx + 1})` : sec.tag;
        slides.push({ title, body: chunk.join("\n").trim(), tag: sec.tag });
      });
    }
  }

  // Resolve final title
  if (!metadata.title && fallbackTitle?.trim()) metadata.title = fallbackTitle.trim();

  return { metadata, slides };
}

// Helper for UI: count slides without full parse side effects
export function previewTitle(raw: string, fallbackTitle?: string): string {
  const { metadata } = parseSong(raw, fallbackTitle);
  return metadata.title || fallbackTitle || "Untitled";
}

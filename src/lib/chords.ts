/**
 * ChordPro chord utilities — lightweight, no new backend needed.
 * Stored body text keeps raw "[G]Am[zing" etc.; we strip only at render time
 * per-view (Output = clean, Stage = stacked). This keeps the same stored
 * content serving both views differently.
 */

export interface ChordSegment {
  chord: string | null;
  lyric: string;
}

/** Does the text contain any ChordPro bracketed chord? */
export function hasChords(text: string | null | undefined): boolean {
  if (!text) return false;
  return /\[[^\]\n]+\]/.test(text);
}

/** Strip all bracketed chords, e.g. "[G]Amazing [D]grace" → "Amazing grace" . */
export function stripChords(text: string | null | undefined): string {
  if (!text) return "";
  // Remove anything in brackets, including the brackets themselves
  return text.replace(/\[[^\]]*\]/g, "");
}

/**
 * Parse a single lyric line with inline chords into segments.
 * Each segment is a chord + the lyric chunk that follows it until the next chord.
 * E.g. "[G]Amazing [D]grace" → [{chord:"G", lyric:"Amazing "}, {chord:"D", lyric:"grace"}]
 * Plain line "Amazing grace" → [{chord:null, lyric:"Amazing grace"}]
 */
export function parseChordLine(line: string): ChordSegment[] {
  const segments: ChordSegment[] = [];
  let currentChord: string | null = null;
  let currentLyric = "";
  let i = 0;
  while (i < line.length) {
    if (line[i] === "[") {
      const close = line.indexOf("]", i);
      if (close !== -1) {
        // Flush previous segment before starting new chord
        if (currentLyric !== "" || currentChord !== null) {
          segments.push({ chord: currentChord, lyric: currentLyric });
          currentLyric = "";
        }
        const raw = line.slice(i + 1, close).trim();
        currentChord = raw.length > 0 ? raw : null;
        i = close + 1;
        continue;
      }
    }
    currentLyric += line[i];
    i++;
  }
  // Push last segment
  segments.push({ chord: currentChord, lyric: currentLyric });
  // Filter out completely empty segments (both chord and lyric empty) but keep chord-only segments
  return segments.filter((s) => s.lyric !== "" || s.chord !== null);
}

/** Parse a multi-line body into lines of segments. */
export function parseChordBody(body: string): ChordSegment[][] {
  if (!body) return [];
  return body.split("\n").map((line) => parseChordLine(line));
}

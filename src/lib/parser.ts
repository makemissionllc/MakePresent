/**
 * Refactored Song Parser — FreeShow-style 2-line auto-splitting.
 * Canonical parser engine. songParser.ts re-exports for compat.
 */
export type VAlign = "top" | "center" | "bottom";
export type HAlign = "left" | "center" | "right";

export interface SongSlide {
  id: string;
  groupId: string;
  groupLabel: string;
  subIndex: number;
  lines: string[];
  positioning: {
    vAlign: VAlign;
    hAlign: HAlign;
  };
}

export interface ParserOptions {
  maxLinesPerSlide?: 1 | 2 | 4;
  defaultVAlign?: VAlign;
  defaultHAlign?: HAlign;
}

import { parseSong as legacyParse } from "./songParser";

// Re-export metadata types for convenience
export type { SongMetadata, ParsedSong, ParsedSlide } from "./songParser";

function slugify(tag: string): string {
  return tag.toLowerCase().replace(/\s+/g, "-").replace(/[^a-z0-9-]/g, "");
}

function makeId(): string {
  // Use crypto if available, fallback to random
  try {
    // @ts-ignore
    if (typeof crypto !== "undefined" && crypto.randomUUID) return crypto.randomUUID();
  } catch {}
  return Math.random().toString(36).slice(2, 9);
}

export function parseToSongSlides(
  raw: string,
  fallbackTitle?: string,
  opts: ParserOptions = {},
): { metadata: import("./songParser").SongMetadata; slides: SongSlide[]; title: string } {
  const maxLines = opts.maxLinesPerSlide ?? 2;
  const vAlign = opts.defaultVAlign ?? "center";
  const hAlign = opts.defaultHAlign ?? "center";

  // Delegate section splitting to legacy parser but with custom chunking
  // We re-implement chunking here to respect maxLines instead of legacy 4
  // To avoid double logic, call legacy with patched chunk size via direct import of internal?
  // Simpler: call legacy parse then re-chunk each slide's body to maxLines
  const legacy = legacyParse(raw, fallbackTitle);

  const slides: SongSlide[] = [];
  // Group counter for groupId uniqueness per label
  const groupCounts = new Map<string, number>();

  for (const ls of legacy.slides) {
    // ls.body may be > maxLines if legacy used 4; re-split to maxLines
    const lines = ls.body.split("\n").map((l) => l.trimEnd()).filter((l) => l.length > 0 || ls.body.includes("\n"));
    // Actually preserve original lines including empty? Better split on "\n" and filter empty trimmed?
    const rawLines = ls.body.split("\n").filter((l) => l.trim().length > 0);
    // Chunk rawLines to maxLines
    const chunks: string[][] = [];
    for (let i = 0; i < rawLines.length; i += maxLines) {
      chunks.push(rawLines.slice(i, i + maxLines));
    }
    if (chunks.length === 0) chunks.push([]);

    const groupId = slugify(ls.tag);
    chunks.forEach((chunk, idx) => {
      // For subIndex, count per group
      const key = groupId;
      const count = groupCounts.get(key) ?? 0;
      // subIndex is 1-based per group occurrence
      const subIndex = count + idx + 1;
      // We will update groupCounts after loop
      slides.push({
        id: makeId(),
        groupId,
        groupLabel: ls.tag,
        subIndex: chunks.length > 1 ? idx + 1 : 1,
        lines: chunk,
        positioning: { vAlign, hAlign },
      });
    });
    // Update count for next section with same groupId
    const prev = groupCounts.get(groupId) ?? 0;
    groupCounts.set(groupId, prev + chunks.length);
  }

  // Fix subIndex for single-chunk groups: keep 1, for multi-chunk already set
  // Also handle case where legacy already split "Verse 1 (Part 1)" — we merged via rawLines, so groupLabel still original
  // To preserve correct Part numbering across double-break groups, our re-chunk already handled.

  const title = legacy.metadata.title || fallbackTitle || "Untitled";
  return { metadata: legacy.metadata, slides, title };
}

// Default parse with 2 lines for convenience
export function parseSong2(raw: string, fallbackTitle?: string): ReturnType<typeof parseToSongSlides> {
  return parseToSongSlides(raw, fallbackTitle, { maxLinesPerSlide: 2 });
}

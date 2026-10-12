import type { SongSlide } from "./parser";

/** Partition a lyric section in O(lines × maxLines), preserving every line.
 * Prefer readable character counts, full slides, and punctuation boundaries.
 * These are layout heuristics; the operator reviews the result before saving.
 */
function partition(lines: string[], maxLines: number): string[][] {
  const cost = new Float64Array(lines.length + 1);
  cost.fill(Infinity);
  cost[lines.length] = 0;
  const sizes = new Uint8Array(lines.length);
  for (let i = lines.length - 1; i >= 0; i--) {
    let characters = 0;
    for (let size = 1; size <= maxLines && i + size <= lines.length; size++) {
      characters += lines[i + size - 1].length;
      const end = i + size;
      const overflow = Math.max(0, characters - 100);
      const underfill = Math.max(0, Math.min(maxLines, lines.length) - size);
      const boundary = end < lines.length && !/[.!?;:]["'’”)]?$/.test(lines[end - 1].trim()) ? 12 : 0;
      const orphan = size === 1 && lines.length > 1 && maxLines > 1 ? 22 : 0;
      const candidate = 40 + underfill * 6 + overflow * overflow / 25 + boundary + orphan + cost[end];
      if (candidate < cost[i]) { cost[i] = candidate; sizes[i] = size; }
    }
  }
  const chunks: string[][] = [];
  for (let i = 0; i < lines.length;) {
    const size = sizes[i] || 1;
    chunks.push(lines.slice(i, i + size));
    i += size;
  }
  return chunks;
}

export function smartSongLayout(slides: SongSlide[], maxLines: 1 | 2 | 4): SongSlide[] {
  const output: SongSlide[] = [];
  for (let i = 0; i < slides.length;) {
    const first = slides[i];
    const lines: string[] = [];
    // Respect section boundaries and intentional alignment changes.
    let end = i;
    while (end < slides.length && slides[end].groupId === first.groupId &&
      slides[end].positioning.vAlign === first.positioning.vAlign &&
      slides[end].positioning.hAlign === first.positioning.hAlign) {
      lines.push(...slides[end].lines);
      end++;
    }
    const chunks = partition(lines, maxLines);
    if (!chunks.length) chunks.push([]);
    chunks.forEach((chunk, index) => output.push({
      ...first, id: globalThis.crypto?.randomUUID?.() ?? `layout-${Date.now()}-${Math.random().toString(36).slice(2)}`, subIndex: index + 1,
      lines: chunk, positioning: { ...first.positioning },
    }));
    i = end;
  }
  return output;
}

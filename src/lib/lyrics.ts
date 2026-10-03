/** Turn plain or time-coded lyrics into the Song Editor's section format.
 * The result stays editable: uncertain section names are only suggestions. */
export function formatImportedLyrics(raw: string): string {
  const lines = raw.replace(/\r\n?/g, "\n").split("\n")
    .map((line) => line
      .replace(/^(?:\[\d{1,2}:\d{2}(?:[.:]\d{1,3})?\])+\s*/g, "")
      .trim())
    .filter((line) => !/^\[(?:ar|al|ti|by|offset|length):/i.test(line));

  const cleaned = lines.join("\n").replace(/\n{3,}/g, "\n\n").trim();
  if (!cleaned) return "";

  const section = /^(?:#{1,4}\s*)?(?:\[\s*)?(?:verse|chorus|pre[- ]chorus|bridge|refrain|intro|outro|tag|ending|coda)(?:\s+\d+)?(?:\s*\])?\s*:?$/i;
  if (lines.some((line) => section.test(line))) {
    return lines.map((line) => section.test(line)
      ? `### ${line.replace(/^#{1,4}\s*|^\[\s*|\s*\]$|:$/g, "").trim()}`
      : line).join("\n").replace(/\n{3,}/g, "\n\n").trim();
  }

  let stanzas = cleaned.split(/\n\s*\n/).map((part) => part.trim()).filter(Boolean);
  // Synchronized lyrics often have no stanza breaks; offer manageable blocks.
  if (stanzas.length === 1) {
    const all = stanzas[0].split("\n").filter(Boolean);
    if (all.length > 4) {
      stanzas = [];
      for (let i = 0; i < all.length; i += 4) stanzas.push(all.slice(i, i + 4).join("\n"));
    }
  }

  const counts = new Map<string, number>();
  for (const stanza of stanzas) {
    const key = stanza.replace(/\s+/g, " ").toLowerCase();
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  let verse = 0;
  return stanzas.map((stanza) => {
    const key = stanza.replace(/\s+/g, " ").toLowerCase();
    const label = (counts.get(key) ?? 0) > 1 ? "Chorus" : `Verse ${++verse}`;
    return `### ${label}\n${stanza}`;
  }).join("\n\n");
}

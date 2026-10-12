import type { ScriptureMatch } from "./types";

function normalize(text: string): string {
  return text
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .toLocaleLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
}

function oneEditApart(a: string, b: string): boolean {
  if (Math.abs(a.length - b.length) > 1 || a.length > 80 || b.length > 80) return false;
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  if (i === Math.min(a.length, b.length)) return true;
  if (a.length > b.length) return a.slice(i + 1) === b.slice(i);
  if (a.length < b.length) return a.slice(i) === b.slice(i + 1);
  return (
    a.slice(i + 1) === b.slice(i + 1) ||
    (a[i] === b[i + 1] && a[i + 1] === b[i] && a.slice(i + 2) === b.slice(i + 2))
  );
}

/**
 * Intelligent ranking for Scripture hits.
 *
 * Exact and prefix reference matches come first (e.g. "john 3:16" finds
 * John 3:16 before John 3 commentary text), then full token coverage of the
 * reference, then small spelling mistakes, then verse-text matches.
 * Bounded: token lists are capped and comparison is O(tokens × words).
 */
export function rankScriptureMatches(
  matches: ScriptureMatch[],
  query: string,
  limit = 12,
): ScriptureMatch[] {
  const q = normalize(query.slice(0, 120));
  if (!q) return matches.slice(0, limit);
  const tokens = [...new Set(q.split(" ").filter(Boolean))].slice(0, 8);
  const scored = matches.map((match, order) => {
    const ref = normalize(match.reference);
    const refWords = [...new Set(ref.split(" ").filter(Boolean))].slice(0, 24);
    const text = normalize(match.text ?? "").slice(0, 2000);
    let score = 0;
    if (ref === q) score = 1000;
    else if (ref.startsWith(q)) score = 900;
    else if (ref.includes(q)) score = 800;
    else if (tokens.length > 0 && tokens.every((t) => refWords.some((w) => w.startsWith(t)))) score = 600;
    else if (
      tokens.length > 0 &&
      tokens.every((t) => refWords.some((w) => w.startsWith(t) || (t.length >= 4 && oneEditApart(t, w))))
    )
      score = 500;
    else if (text && text.includes(q)) score = 300;
    else if (tokens.length > 0 && tokens.every((t) => text.includes(t))) score = 200;
    return { match, score, order };
  });
  return scored
    .filter((s) => s.score > 0)
    .sort((a, b) => b.score - a.score || a.order - b.order)
    .slice(0, limit)
    .map((s) => s.match);
}

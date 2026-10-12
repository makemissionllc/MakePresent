export interface DuplicateHit {
  title: string;
  reason: string;
}

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
 * Intelligent near-duplicate detection for song titles.
 *
 * Exact and prefix matches score highest, then full token coverage, then
 * single-edit fuzzy token matches, then Jaccard token overlap. Bounded:
 * titles are capped and token lists are truncated, so a large library stays
 * cheap to scan on each keystroke.
 */
export function findDuplicateSongs(
  title: string,
  existingTitles: string[],
  limit = 3,
): DuplicateHit[] {
  const q = normalize(title.slice(0, 120));
  if (!q) return [];
  const tokens = [...new Set(q.split(" ").filter(Boolean))].slice(0, 8);
  const seen = new Set<string>();
  const scored: { title: string; reason: string; score: number; order: number }[] = [];
  existingTitles.slice(0, 2000).forEach((raw, order) => {
    const candidate = (raw ?? "").slice(0, 120);
    const n = normalize(candidate);
    if (!n || seen.has(n)) return;
    seen.add(n);
    const words = [...new Set(n.split(" ").filter(Boolean))].slice(0, 16);
    let score = 0;
    let reason = "Same title";
    if (n === q) score = 100;
    else if (n.startsWith(q) || q.startsWith(n)) {
      score = 90;
      reason = "Overlapping title";
    } else if (tokens.length > 0 && tokens.every((t) => words.some((w) => w.startsWith(t)))) {
      score = 70;
      reason = "Same words, different order";
    } else if (
      tokens.length > 0 &&
      tokens.every((t) => words.some((w) => w.startsWith(t) || (t.length >= 4 && oneEditApart(t, w))))
    ) {
      score = 60;
      reason = "Similar title (possible typo)";
    } else if (tokens.length > 0 && words.length > 0) {
      const overlap = tokens.filter((t) => words.includes(t)).length;
      const union = new Set([...tokens, ...words]).size;
      if (union > 0 && overlap / union >= 0.5) {
        score = 50;
        reason = "Shares most words";
      }
    }
    if (score > 0) scored.push({ title: candidate, reason, score, order });
  });
  return scored
    .sort((a, b) => b.score - a.score || a.order - b.order)
    .slice(0, limit)
    .map(({ title, reason }) => ({ title, reason }));
}

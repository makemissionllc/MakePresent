import type { LibrarySong } from "./types";

function normalize(text: string): string {
  return text.normalize("NFKD").replace(/\p{M}/gu, "")
    .toLocaleLowerCase().replace(/[^\p{L}\p{N}]+/gu, " ").trim();
}

/** A bounded single-edit comparison, including an adjacent letter swap. */
function oneEditApart(a: string, b: string): boolean {
  if (Math.abs(a.length - b.length) > 1 || a.length > 80 || b.length > 80) return false;
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  if (i === Math.min(a.length, b.length)) return true;
  if (a.length > b.length) return a.slice(i + 1) === b.slice(i);
  if (a.length < b.length) return a.slice(i) === b.slice(i + 1);
  return a.slice(i + 1) === b.slice(i + 1) ||
    (a[i] === b[i + 1] && a[i + 1] === b[i] && a.slice(i + 2) === b.slice(i + 2));
}

export function indexSongs(songs: LibrarySong[]) {
  return songs.map((song) => {
    const title = normalize(song.title);
    const blocks = song.blocks ? Object.values(song.blocks) : song.slides ?? [];
    return {
      song, title,
      words: [...new Set(title.split(" "))].slice(0, 64),
      lyrics: normalize(blocks.map((block) => `${block.title} ${block.body}`).join(" ")),
    };
  });
}

export function rankSongs(index: ReturnType<typeof indexSongs>, query: string, limit = 8) {
  const q = normalize(query.slice(0, 120));
  if (!q) return index.slice(0, limit).map(({ song }) => ({ song, reason: "Library" }));
  const tokens = [...new Set(q.split(" "))].slice(0, 8);
  return index.flatMap((entry, order) => {
    let score = 0;
    let reason = "Title match";
    if (entry.title === q) score = 1000;
    else if (entry.title.startsWith(q)) score = 900;
    else if (entry.title.includes(q)) score = 800;
    else if (tokens.every((token) => entry.words.some((word) => word.startsWith(token)))) score = 600;
    else if (tokens.every((token) => entry.words.some((word) =>
      word.startsWith(token) || (token.length >= 4 && oneEditApart(token, word))))) {
      score = 500;
      reason = "Similar title";
    } else if (entry.lyrics.includes(q)) {
      score = 300;
      reason = "Lyrics match";
    } else if (tokens.every((token) => entry.lyrics.includes(token))) {
      score = 200;
      reason = "Lyrics match";
    }
    return score ? [{ song: entry.song, reason, score, order }] : [];
  }).sort((a, b) => b.score - a.score || a.order - b.order).slice(0, limit);
}

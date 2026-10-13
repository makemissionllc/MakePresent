export interface ReadingPlace {
  bibleId: string;
  book: string;
  chapter: number;
  verse: number;
  version: string;
}

export interface ReaderPreferences {
  layout: "page" | "columns";
  tone: "dark" | "paper";
}

export const READING_PLACE_KEY = "makrstudio.scripture-reading-place.v1";
const PREFERENCES_KEY = "makrstudio.scripture-reader-preferences.v1";

export function loadReadingPlace(): ReadingPlace | null {
  try {
    const value = JSON.parse(localStorage.getItem(READING_PLACE_KEY) ?? "null");
    if (!value || typeof value.bibleId !== "string" || !value.bibleId.trim() ||
      typeof value.book !== "string" || !value.book.trim() ||
      !Number.isInteger(value.chapter) || value.chapter < 1 || value.chapter > 200 ||
      !Number.isInteger(value.verse) || value.verse < 1 || value.verse > 999) return null;
    return { bibleId: value.bibleId, book: value.book, chapter: value.chapter, verse: value.verse, version: typeof value.version === "string" ? value.version : "" };
  } catch {
    return null;
  }
}

export function loadReaderPreferences(): ReaderPreferences {
  try {
    const value = JSON.parse(localStorage.getItem(PREFERENCES_KEY) ?? "null");
    return { layout: value?.layout === "columns" ? "columns" : "page", tone: value?.tone === "paper" ? "paper" : "dark" };
  } catch {
    return { layout: "page", tone: "dark" };
  }
}

export function saveReaderPreferences(preferences: ReaderPreferences): void {
  try { localStorage.setItem(PREFERENCES_KEY, JSON.stringify(preferences)); } catch { /* Session preferences still work. */ }
}

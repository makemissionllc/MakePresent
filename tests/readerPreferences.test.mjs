import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

// Exercise the actual TypeScript storage boundary without a browser or a new runner.
const source = await readFile(new URL("../src/lib/readerPreferences.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 },
});
const reader = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`);
let storage;
beforeEach(() => {
  storage = new Map();
  globalThis.localStorage = {
    getItem: (key) => storage.get(key) ?? null,
    setItem: (key, value) => storage.set(key, value),
  };
});

test("unreadable or incomplete saved places cannot break reader startup", () => {
  const invalid = ["bad JSON", "null", "[]", "{}",
    JSON.stringify({ bibleId: "kjv", book: "John", chapter: -1, verse: 16 }),
    JSON.stringify({ bibleId: "kjv", book: "John", chapter: 1, verse: "16" }),
    JSON.stringify({ bibleId: "", book: "John", chapter: 1, verse: 16 }),
  ];
  for (const value of invalid) {
    storage.set(reader.READING_PLACE_KEY, value);
    assert.equal(reader.loadReadingPlace(), null);
  }
});

test("a saved place restores the exact translation, chapter, and verse", () => {
  const place = { bibleId: "kjv", book: "John", chapter: 1, verse: 16, version: "KJV" };
  storage.set(reader.READING_PLACE_KEY, JSON.stringify(place));
  assert.deepEqual(reader.loadReadingPlace(), place);
});

test("reader preferences persist without replacing a saved reading place", () => {
  storage.set(reader.READING_PLACE_KEY, "existing place");
  reader.saveReaderPreferences({ layout: "columns", tone: "paper" });
  assert.deepEqual(reader.loadReaderPreferences(), { layout: "columns", tone: "paper" });
  assert.equal(storage.get(reader.READING_PLACE_KEY), "existing place");
});

test("old or unknown preference values fall back to a readable page", () => {
  assert.deepEqual(reader.loadReaderPreferences(), { layout: "page", tone: "dark" });
  reader.saveReaderPreferences({ layout: "unknown", tone: "unknown" });
  assert.deepEqual(reader.loadReaderPreferences(), { layout: "page", tone: "dark" });
});

test("blocked device storage leaves session reading usable", () => {
  globalThis.localStorage = {
    getItem() { throw new Error("Storage blocked"); },
    setItem() { throw new Error("Storage blocked"); },
  };
  assert.equal(reader.loadReadingPlace(), null);
  assert.deepEqual(reader.loadReaderPreferences(), { layout: "page", tone: "dark" });
  assert.doesNotThrow(() => reader.saveReaderPreferences({ layout: "columns", tone: "paper" }));
});

import type { Chapter } from "./types";

const KEY = "novel-codex-chapter-locations";
const LIMIT = 128;
const validId = (value: unknown): value is string => typeof value === "string" && value.length > 0 && value.length <= 240;

function locations(): [string, string][] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw || raw.length > 128_000) return [];
    const value: unknown = JSON.parse(raw);
    return Array.isArray(value) ? value.slice(-LIMIT).filter((entry): entry is [string, string] =>
      Array.isArray(entry) && entry.length === 2 && validId(entry[0]) && validId(entry[1])) : [];
  } catch { return []; }
}

export function lastChapter(projectId: string, chapters: Chapter[]): string {
  const saved = locations().find(([project]) => project === projectId)?.[1];
  return chapters.find(chapter => chapter.id === saved)?.id ?? chapters[0]?.id ?? "";
}

export function rememberChapter(projectId: string, chapterId: string) {
  if (!validId(projectId) || !validId(chapterId)) return;
  try {
    localStorage.setItem(KEY, JSON.stringify([...locations().filter(([project]) => project !== projectId), [projectId, chapterId]].slice(-LIMIT)));
  } catch { /* Navigation remains available when local storage is unavailable. */ }
}

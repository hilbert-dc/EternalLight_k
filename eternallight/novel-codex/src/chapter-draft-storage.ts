import type { ChapterDraft } from "./merge-chapter-draft";
export type StoredChapterDraft = ChapterDraft & { baseProjectRevision: number; baseCandidateRevisionId: string | null; baseContent: string };
export const chapterDraftKey = (author: string, project: string, chapter: string) => `novel-codex:chapter-editor-draft:${JSON.stringify([author, project, chapter])}`;
export function readChapterDraft(key: string): StoredChapterDraft | null {
  const raw = localStorage.getItem(key);
  if (raw === null) return null;
  let value;
  try { value = JSON.parse(raw); } catch { throw new Error("本地章节草稿无法读取，原记录仍保留。请勿关闭应用，可先复制当前正文。"); }
  if (!value || typeof value.title !== "string" || typeof value.baseContent !== "string"
    || !Number.isSafeInteger(value.baseProjectRevision) || value.baseProjectRevision < 0
    || !(value.baseCandidateRevisionId === null || typeof value.baseCandidateRevisionId === "string")
    || !Array.isArray(value.sceneGroups) || value.sceneGroups.some((scene: any) => !scene || typeof scene.id !== "string" || !scene.id
      || typeof scene.body !== "string" || !(scene.sceneId === null || typeof scene.sceneId === "string") || typeof scene.locked !== "boolean")
    || new Set(value.sceneGroups.map((scene: any) => scene.id)).size !== value.sceneGroups.length) throw new Error("本地章节草稿无法读取，原记录仍保留。请勿关闭应用，可先复制当前正文。");
  return value;
}

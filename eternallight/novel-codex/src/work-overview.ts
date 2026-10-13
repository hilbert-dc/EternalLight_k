import type { CreativeWorkspace, DraftRevision } from "./types";

function characters(revision: DraftRevision | null) {
  if (!revision) return 0;
  const text = revision.sceneGroups.length
    ? revision.sceneGroups.map(group => group.body).join("") : revision.body;
  return Array.from(text.replace(/\s/gu, "")).length;
}

export function summarizeWorkOverview(workspace: CreativeWorkspace) {
  const unique = new Map<string, CreativeWorkspace["chapters"][number]>();
  for (const chapter of workspace.chapters) if (!unique.has(chapter.id)) unique.set(chapter.id, chapter);
  const chapters = [...unique.values()].sort((left, right) => left.number - right.number);
  const written = chapters.filter(chapter => characters(chapter.candidate ?? chapter.accepted) > 0);
  const times = written.map(chapter => {
    const value = (chapter.candidate ?? chapter.accepted)?.createdAt;
    return value ? Date.parse(value) : NaN;
  });
  const lastEditedAt = times.length && times.every(Number.isFinite)
    ? new Date(Math.max(...times)).toISOString() : null;
  const continuation = written.at(-1) ?? chapters[0];
  return {
    writtenChapters: written.length,
    characters: chapters.reduce((sum, chapter) => sum + characters(chapter.candidate ?? chapter.accepted), 0),
    publishedChapters: chapters.filter(chapter => characters(chapter.accepted) > 0).length,
    lastEditedAt,
    continueChapterId: continuation?.id ?? null,
    continueChapterNumber: continuation?.number ?? null,
  };
}

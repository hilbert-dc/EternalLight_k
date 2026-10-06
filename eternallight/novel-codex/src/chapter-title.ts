import type { Chapter, CreativeWorkspace } from "./types";

export type ChapterTitlePreview = { projectId: string; chapterId: string; title: string };

export function savedChapterTitle(chapter: CreativeWorkspace["chapters"][number]): string {
  return (chapter.candidate ?? chapter.accepted)?.title ?? chapter.title;
}

/** Display labels never replace the original chapter used by navigation or writing. */
export function displayChapters(chapters: Chapter[], savedTitles: { id: string; title: string }[], preview?: ChapterTitlePreview | null): Chapter[] {
  const titles = new Map(savedTitles.map(chapter => [chapter.id, chapter.title]));
  return chapters.map(chapter => {
    const title = preview?.chapterId === chapter.id ? preview.title : titles.get(chapter.id) ?? chapter.title;
    return title === chapter.title ? chapter : { ...chapter, title };
  });
}

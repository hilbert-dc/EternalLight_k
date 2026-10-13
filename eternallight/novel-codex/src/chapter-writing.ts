export { NEW_CHAPTER_WRITING_INTENT, CONTINUE_CHAPTER_WRITING_INTENT } from "../shared/chapter-writing-policy.mjs";

export type ChapterWritingRequest = {
  projectId: string;
  chapterId: string;
  chapterNumber: number;
  title: string;
  defaultIntent: string;
  prepareDiscussion: () => Promise<boolean>;
  submit: (intent: string) => Promise<boolean>;
};

export function chapterWritingMatches(
  request: ChapterWritingRequest | null,
  route: { projectId: string; view: string; chapterId: string },
): request is ChapterWritingRequest {
  return !!request && !!route.projectId && route.view === "chapters"
    && request.projectId === route.projectId && request.chapterId === route.chapterId;
}

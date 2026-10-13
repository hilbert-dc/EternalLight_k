export type ViewId = "brief" | "outline" | "plans" | "chapters" | "characters" | "plot" | "timeline" | "world";
export type DiscussionExcerpt = {
  id: string; projectId: string; view: ViewId; label: string; text: string;
  state: "displayed" | "unsaved"; locator: string;
  snapshotId: string | null; revisionId: string | null; objectId: string | null;
  start: number; end: number;
};
export const MAX_DISCUSSION_EXCERPTS: number;
export const MAX_EXCERPT_CHARACTERS: number;
export const MAX_EXCERPT_BYTES: number;
export function validateDiscussionExcerpts(value: unknown): readonly DiscussionExcerpt[];
export function discussionExcerptsError(value: unknown): string | null;

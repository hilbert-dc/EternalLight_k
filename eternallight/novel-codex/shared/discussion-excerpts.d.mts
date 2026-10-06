import type { DiscussionExcerpt } from "../src/types";
export const MAX_DISCUSSION_EXCERPTS: number;
export const MAX_EXCERPT_CHARACTERS: number;
export const MAX_EXCERPT_BYTES: number;
export function validateDiscussionExcerpts(value: unknown): readonly DiscussionExcerpt[];
export function discussionExcerptsError(value: unknown): string | null;

import type { DiscussionExcerpt } from "./discussion-excerpts.mjs";
export const MAX_DISCUSSION_EXCERPTS: number;
export const MAX_EXCERPT_CHARACTERS: number;
export const MAX_EXCERPT_BYTES: number;
export function validateDiscussionExcerptShape(value: unknown): readonly DiscussionExcerpt[];

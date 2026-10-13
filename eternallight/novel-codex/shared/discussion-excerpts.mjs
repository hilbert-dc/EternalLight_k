// @ts-check
import { modelInputTokenBudget } from "./author-input-budget.mjs";
import { validateDiscussionExcerptShape } from "./discussion-excerpt-shape.mjs";
export { MAX_DISCUSSION_EXCERPTS, MAX_EXCERPT_CHARACTERS, MAX_EXCERPT_BYTES } from "./discussion-excerpt-shape.mjs";
export function validateDiscussionExcerpts(value) {
  const result = validateDiscussionExcerptShape(value);
  if (modelInputTokenBudget(JSON.stringify(result)) > 6_000) throw Error("引用内容超过本轮文本预算，请缩小选区或移除部分引用。原文不会被截断。");
  return result;
}
export function discussionExcerptsError(value) {
  try { validateDiscussionExcerpts(value); return null; }
  catch (error) { return error.message; }
}

import { modelInputTokenBudget } from "./author-input-budget.mjs";

export const MAX_DISCUSSION_EXCERPTS = 8;
export const MAX_EXCERPT_CHARACTERS = 12_000;
export const MAX_EXCERPT_BYTES = 24_000;
const views = new Set(["brief", "outline", "plans", "chapters", "characters", "world", "plot", "timeline"]);
const keys = ["id", "projectId", "view", "label", "text", "state", "locator", "snapshotId", "revisionId", "objectId", "start", "end"];
const identifier = value => typeof value === "string" && value.length > 0 && value.trim() === value
  && new TextEncoder().encode(value).byteLength <= 160 && !/[\p{Cc}]/u.test(value);

export function validateDiscussionExcerpts(value) {
  if (!Array.isArray(value) || !value.length || value.length > MAX_DISCUSSION_EXCERPTS) throw Error("每轮最多引用 8 段内容。");
  const ids = new Set();
  let characters = 0, bytes = 0;
  const result = value.map(item => {
    if (!item || typeof item !== "object" || Array.isArray(item) || Reflect.ownKeys(item).length !== keys.length
      || keys.some(key => !Object.hasOwn(item, key)) || !identifier(item.id) || ids.has(item.id)
      || !identifier(item.projectId) || !views.has(item.view) || !["displayed", "unsaved"].includes(item.state)
      || typeof item.text !== "string" || !item.text.trim()
      || typeof item.label !== "string" || !item.label.trim() || item.label.length > 240 || /[\p{Cc}]/u.test(item.label)
      || typeof item.locator !== "string" || item.locator.length > 240 || /[\p{Cc}]/u.test(item.locator)
      || ["snapshotId", "revisionId", "objectId"].some(key => item[key] !== null && !identifier(item[key]))
      || !Number.isSafeInteger(item.start) || item.start < 0 || !Number.isSafeInteger(item.end)
      || item.end - item.start !== item.text.length) throw Error("选区引用字段无效，请重新选择内容。");
    ids.add(item.id);
    characters += item.text.length;
    bytes += new TextEncoder().encode(item.text).byteLength;
    return Object.freeze(Object.fromEntries(keys.map(key => [key, item[key]])));
  });
  if (characters > MAX_EXCERPT_CHARACTERS || bytes > MAX_EXCERPT_BYTES) throw Error("引用内容过长：每轮最多 12000 字符、24000 字节。请缩小选区或移除部分引用，原文不会被截断。");
  if (modelInputTokenBudget(JSON.stringify(result)) > 6_000) throw Error("引用内容超过本轮文本预算，请缩小选区或移除部分引用。原文不会被截断。");
  return Object.freeze(result);
}

export function discussionExcerptsError(value) {
  try { validateDiscussionExcerpts(value); return null; }
  catch (error) { return error.message; }
}

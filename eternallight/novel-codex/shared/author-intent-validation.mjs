// @ts-check
export const MAX_AUTHOR_INTENT_CHARACTERS = 32_000;
export const MAX_AUTHOR_INTENT_BYTES = 48_000;
export function authorIntentError(text, { imageAssetRevisionIds = [] } = {}) {
  const hasImages = Array.isArray(imageAssetRevisionIds) && imageAssetRevisionIds.length > 0 && imageAssetRevisionIds.length <= 5
    && imageAssetRevisionIds.every(id => typeof id === "string" && id.trim() === id && id.length > 0 && id.length <= 160 && !/[\p{Cc}]/u.test(id));
  if (typeof text !== "string" || !text.trim() && !hasImages) return "创作意图不能为空";
  const characters = text.length;
  const bytes = new TextEncoder().encode(text).byteLength;
  if (characters > MAX_AUTHOR_INTENT_CHARACTERS || bytes > MAX_AUTHOR_INTENT_BYTES) {
    return `创作意图过长：当前 ${characters} 字符、${bytes} 字节；最多 ${MAX_AUTHOR_INTENT_CHARACTERS} 字符、${MAX_AUTHOR_INTENT_BYTES} UTF-8 字节。请整理本轮要求后重试，原文不会被截断。`;
  }
  return null;
}

export function validateAuthorIntent(text, options) {
  const error = authorIntentError(text, options);
  if (error) throw new Error(error);
}

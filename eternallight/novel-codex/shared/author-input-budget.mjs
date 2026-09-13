import { Tiktoken } from "js-tiktoken/lite";
import o200kBase from "js-tiktoken/ranks/o200k_base";
import cl100kBase from "js-tiktoken/ranks/cl100k_base";

export const MAX_AUTHOR_INTENT_CHARACTERS = 32_000;
export const MAX_AUTHOR_INTENT_BYTES = 48_000;
export const MAX_AUTHOR_INTENT_TOKENS = 7_000;
let encodings;

export function modelInputTokenBudget(text) {
  encodings ??= [new Tiktoken(o200kBase), new Tiktoken(cl100kBase)];
  // This is an application safety budget, not a provider billing token count.
  // Bound BPE work on unbroken Unicode runs, which otherwise grows quadratically.
  // Count every fragment; wire splitting never resets the whole-message budget.
  const characters = Array.from(text);
  const counts = [0, 0];
  for (let offset = 0; offset < characters.length; offset += 256) {
    const fragment = characters.slice(offset, offset + 256).join("");
    encodings.forEach((encoding, index) => { counts[index] += encoding.encode(fragment, [], []).length; });
  }
  return Math.max(...counts);
}

export function authorIntentError(text) {
  if (typeof text !== "string" || !text.trim()) return "创作意图不能为空";
  const characters = text.length;
  const bytes = new TextEncoder().encode(text).byteLength;
  if (characters > MAX_AUTHOR_INTENT_CHARACTERS || bytes > MAX_AUTHOR_INTENT_BYTES) {
    return `创作意图过长：当前 ${characters} 字符、${bytes} 字节；最多 ${MAX_AUTHOR_INTENT_CHARACTERS} 字符、${MAX_AUTHOR_INTENT_BYTES} UTF-8 字节。请整理本轮要求后重试，原文不会被截断。`;
  }
  const tokens = bytes <= MAX_AUTHOR_INTENT_TOKENS ? bytes : modelInputTokenBudget(text);
  if (tokens > MAX_AUTHOR_INTENT_TOKENS) return `创作意图超过本轮文本预算：当前 ${tokens}，最多 ${MAX_AUTHOR_INTENT_TOKENS}。请整理本轮要求后重试，原文不会被截断。`;
  return null;
}

export function validateAuthorIntent(text) {
  const error = authorIntentError(text);
  if (error) throw new Error(error);
}

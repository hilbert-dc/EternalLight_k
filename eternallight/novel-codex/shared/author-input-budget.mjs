// @ts-check
import { Tiktoken } from "js-tiktoken/lite";
import o200kBase from "js-tiktoken/ranks/o200k_base";
import cl100kBase from "js-tiktoken/ranks/cl100k_base";

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


export { MAX_AUTHOR_INTENT_CHARACTERS, MAX_AUTHOR_INTENT_BYTES, authorIntentError, validateAuthorIntent } from "./author-intent-validation.mjs";

// Mirrors Native task_intents; the contract test guards this cross-language boundary.
export const MAX_AUTHOR_ANSWER_RECORD_BYTES = 16_000;
export const AUTHOR_ANSWER_CAPACITY_ERROR = "问题与回答超出单次保存容量，请精简回答后提交；原文仍保留在输入框中。";
export function authorAnswerCapacityError(serialized) {
  return new TextEncoder().encode(serialized).byteLength > MAX_AUTHOR_ANSWER_RECORD_BYTES
    ? AUTHOR_ANSWER_CAPACITY_ERROR : null;
}

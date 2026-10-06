export const MAX_GENERATION_CHAPTERS = 100;

export function generationPermitForChapters(targetChapters) {
  if (!Number.isSafeInteger(targetChapters) || targetChapters < 1 || targetChapters > MAX_GENERATION_CHAPTERS) {
    throw new Error("自动创作章节范围无效");
  }
  return Object.freeze({
    targetChapters,
    maxTaskAttempts: null,
  });
}

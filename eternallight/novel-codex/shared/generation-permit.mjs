export function generationPermitForChapters(targetChapters) {
  if (!Number.isInteger(targetChapters) || targetChapters < 1 || targetChapters > 3) {
    throw new Error("自动创作章节范围无效");
  }
  return Object.freeze({
    targetChapters,
    maxTaskAttempts: Math.min(32, 6 + targetChapters * 5),
    maxProviderRequests: 16 + targetChapters * 64,
    maxProviderTokens: (targetChapters + 1) * 1_000_000,
    maxDurationMinutes: 30,
  });
}

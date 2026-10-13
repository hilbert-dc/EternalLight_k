// @ts-check
export function remainingGenerationDurationMs(deadlineAt, nowMs = Date.now()) {
  const deadlineMs = deadlineAt * 1000;
  if (!Number.isSafeInteger(deadlineAt) || !Number.isSafeInteger(deadlineMs) || !Number.isFinite(nowMs)) {
    throw new Error("自动创作许可的截止时间无效");
  }
  const remaining = Math.floor(deadlineMs - nowMs);
  if (remaining < 1) {
    throw Object.assign(new Error("自动创作许可已过期，不能继续请求模型"), { code: "GENERATION_DEADLINE_EXPIRED" });
  }
  return remaining;
}

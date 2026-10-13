// @ts-check
// Core measures growth after each window's initial working set. This is a
// maintenance interval, not a source/task allowance or a proven cost optimum.
// Keep stable policy across turns to avoid unnecessary prompt-cache changes.
export const NOVEL_CONTEXT_MAINTENANCE = Object.freeze({
  // Host requests official thread compaction only before another turn starts,
  // before injecting its fresh input. This is not a custom Core configuration.
  // One-shot tasks therefore incur no unused end-of-task summary.
  // This is a calibration target, never a task allowance or proven cost optimum.
  contextMaintenanceTokenTarget: 48 * 1024,
  postTurnCompactThresholdPercent: 0,
  autoCompactTargetTokens: 96 * 1024,
  autoCompactTokenLimitScope: "body_after_prefix",
});

// Scheduling evidence only; the Core history and Engine usage ledger remain
// authoritative. Never use cumulative billed input as the current context size.
export class HistoryMaintenance {
  constructor(target = NOVEL_CONTEXT_MAINTENANCE.contextMaintenanceTokenTarget) {
    if (!Number.isSafeInteger(target) || target <= 0) throw new Error("Invalid history maintenance target");
    this.target = target;
    this.context = new Map();
  }
  observe(event) {
    if (event?.method !== "thread/tokenUsage/updated") return;
    const { threadId, tokenUsage } = event.params ?? {};
    const tokens = tokenUsage?.last?.totalTokens;
    if (typeof threadId === "string" && threadId && Number.isSafeInteger(tokens) && tokens >= 0) {
      this.context.set(threadId, tokens);
    }
  }
  due(threadId) { return (this.context.get(threadId) ?? 0) >= this.target; }
  completed(threadId) { this.context.delete(threadId); }
}

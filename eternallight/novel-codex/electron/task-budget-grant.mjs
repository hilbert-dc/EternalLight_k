import { randomUUID } from "node:crypto";
import { assertTaskCompletable } from "@eternallight/agent-engine";

export function grantNewRootBudget({ target, execution, databasePath, getWindow, workspace }) {
  if (target && ["runId", "resumeStepId", "requestedActionId", "requestedByTaskId", "sourceTaskId"]
    .some((field) => Object.hasOwn(target, field))) return undefined;
  if (!databasePath) throw new Error("任务缺少独立引擎预算库");
  const owner = getWindow();
  if (!owner || owner.isDestroyed()) throw new Error("创作窗口已关闭");
  assertTaskCompletable(execution);
  return createInitialTaskBudget({ workspace });
}

// Sending grants host-owned authority, not content adoption. Null limits mean metering only.
export function createInitialTaskBudget({ workspace, now = Date.now }) {
  if (!Number.isSafeInteger(workspace?.revision) || workspace.revision < 0
    || typeof workspace.headSnapshotId !== "string" || !workspace.headSnapshotId
    || typeof workspace.sourceDigest !== "string" || !workspace.sourceDigest) {
    throw new Error("作品来源不完整，无法建立执行额度");
  }
  const expectedBaseSnapshotId = workspace.headSnapshotId;
  const expectedSourceDigest = workspace.sourceDigest;
  const timestamp = now();
  if (!Number.isSafeInteger(timestamp) || timestamp < 0) throw new Error("当前时间无效");
  return { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId, expectedSourceDigest,
    maxRequests: null, maxTokens: null, deadlineAt: null };
}

import { randomUUID } from "node:crypto";
import { assertTaskCompletable } from "@eternallight/agent-engine";

export async function confirmNewRootBudget({ target, execution, databasePath, getWindow, showMessageBox, workspace, projectId, prompt }) {
  if (target && ["runId", "resumeStepId", "requestedActionId", "requestedByTaskId", "sourceTaskId"]
    .some((field) => Object.hasOwn(target, field))) return undefined;
  if (!databasePath) throw new Error("任务缺少独立引擎预算库");
  const owner = getWindow();
  if (!owner || owner.isDestroyed()) throw new Error("创作窗口已关闭");
  const budget = await confirmInitialTaskBudget({
    showMessageBox: (options) => showMessageBox(owner, options),
    workspace, projectId, model: execution.providerConfig?.model, prompt,
  });
  assertTaskCompletable(execution);
  if (owner.isDestroyed()) throw new Error("创作窗口已关闭");
  return budget;
}

// The host owns this finite offer; neither model output nor renderer fields grant it.
export async function confirmInitialTaskBudget({ showMessageBox, workspace, projectId, model, prompt, now = Date.now }) {
  if (!Number.isSafeInteger(workspace?.revision) || workspace.revision < 0
    || typeof workspace.headSnapshotId !== "string" || !workspace.headSnapshotId
    || typeof workspace.sourceDigest !== "string" || !workspace.sourceDigest) {
    throw new Error("作品来源不完整，无法确认执行额度");
  }
  const expectedBaseSnapshotId = workspace.headSnapshotId;
  const expectedSourceDigest = workspace.sourceDigest;
  const { response } = await showMessageBox({
    type: "question", title: "确认本次执行额度", message: "允许模型处理本次创作指令？",
    detail: `作品：${projectId}\n模型：${model ?? "未选择"}\n指令：${Array.from(prompt).slice(0, 300).join("")}${Array.from(prompt).length > 300 ? "…" : ""}\n\n上限：16 次模型请求、200,000 token、15 分钟。重试与后继任务共用余额；到达上限后停止。此上限不是费用报价，实际费用由供应商计费。\n\n这只允许执行，不代表正式采用或发布生成内容。`,
    buttons: ["取消", "确认执行"], defaultId: 0, cancelId: 0, noLink: true,
  });
  if (response !== 1) throw Object.assign(new Error("已取消本次执行"), { code: "TASK_CANCELLED" });
  const timestamp = now();
  if (!Number.isSafeInteger(timestamp) || timestamp < 0) throw new Error("当前时间无效");
  return { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId, expectedSourceDigest,
    maxRequests: 16, maxTokens: 200_000, deadlineAt: Math.floor(timestamp / 1000) + 900 };
}

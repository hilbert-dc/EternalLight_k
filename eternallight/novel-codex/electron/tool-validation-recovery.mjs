import { isExecutorValidatedTool } from "./executor-validation-policy.mjs";

const LIMIT_CODE = "tool_argument_correction_limit";

// Only the task-bound workspace executor can declare this terminal condition.
// Ordinary errors remain visible to Codex so it can correct them in the same turn.
export function toolCorrectionLimitError(message, waiter) {
  const params = message?.params;
  const item = params?.item;
  if (!waiter?.turnId || message.method !== "item/completed"
    || params?.threadId !== waiter.threadId || params?.turnId !== waiter.turnId
    || item?.type !== "mcpToolCall" || item.server !== "novel_workspace"
    || !isExecutorValidatedTool(item.tool) || item.status !== "failed") return null;
  const detail = item.result?.structuredContent;
  if (detail?.code !== LIMIT_CODE || detail.tool !== item.tool || !isExecutorValidatedTool(detail.originTool) || detail.limit !== 3) return null;
  return Object.assign(new Error("本次操作未能修正执行参数，已停止；已保存的内容仍保留。"), { code: LIMIT_CODE });
}

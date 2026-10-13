// @ts-check
import { isExecutorValidatedTool } from "./executor-validation-policy.mjs";
import { coreTurnFailure } from "./core-turn-failure.mjs";

const LIMIT_CODE = "tool_argument_correction_limit";

// The public explanation can be translated without changing the protocol
// failure persisted for Native recovery. Preserve every other diagnostic.
export function taskFailureDiagnostic(error) {
  const terminal = coreTurnFailure(error);
  if (terminal) return `core_turn_failed; ${terminal.message}`;
  if (typeof error?.message === "string" && /^core_turn_failed(?:[;: ]|$)/.test(error.message)) {
    return `execution_failed; ${error.message}`;
  }
  return error?.code === LIMIT_CODE ? LIMIT_CODE : error?.message;
}

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

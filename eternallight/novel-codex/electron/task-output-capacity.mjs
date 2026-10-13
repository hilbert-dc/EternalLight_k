// @ts-check
import { assertNovelTurnBudget } from "./codex-runtime.mjs";

// This describes the already-resolved route. It grants no task or budget authority.
export function taskOutputCapacityInstructions({ sessionPlan, modelRoute }) {
  if (typeof modelRoute !== "string" || !modelRoute || !Array.isArray(sessionPlan?.routes)) return null;
  const routes = sessionPlan.routes.filter(route => route?.logicalModel === modelRoute);
  if (routes.length !== 1) return null;
  const capacity = routes[0].outputBudget;
  if (!Number.isSafeInteger(capacity) || capacity <= 0) return null;
  return `单次模型输出容量：当前绑定路由上限 ${capacity} token，实际请求可更低。上限不是消耗目标；以完成任务所需为准，避免重复读取、重复输出和无依据重试。这是单次能力，作者总预算、任务范围和写入授权仍以原契约为准，不能增加上限或额度。按容量选择方法：公开说明保持简洁，正文等产物可直接提交现有保存工具的完整参数，避免把内部推敲写成公开长分析。容量不足时说明限制，按同Work恢复决策调整方法；不得截断正文或必填参数、把片段算作完整成果，冻结目标、CAS和保护范围不变。`;
}

export function withTaskOutputCapacity(turn, { task, sessionPlan, modelRoute }) {
  if (!task) return turn;
  const instructions = taskOutputCapacityInstructions({ sessionPlan, modelRoute });
  if (!instructions) return turn;
  assertNovelTurnBudget(turn);
  return assertNovelTurnBudget({ ...turn, additionalContext: {
    ...turn.additionalContext, novelOutputCapacity: { kind: "application", value: instructions },
  } });
}

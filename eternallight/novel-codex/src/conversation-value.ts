import type { AgentStep } from "./agent-events";

export function readingRecovery(steps: AgentStep[], step: AgentStep): "recovered" | "recovery" | null {
  const key = step.operation?.retryKey;
  if (!key) return null;
  const index = steps.indexOf(step);
  if (step.status === "failed" && steps.slice(index + 1).some(other => other.status === "completed"
    && other.operation?.outcome === "read" && other.operation.retryKey === key)) return "recovered";
  if (step.status === "completed" && step.operation?.outcome === "read" && steps.slice(0, index).some(other =>
    other.status === "failed" && other.operation?.retryKey === key)) return "recovery";
  return null;
}

// An operation separates intent from the next observation. Never merge public
// explanations across that boundary or infer equivalence from their wording.
export function meaningfulProgress(steps: AgentStep[]): AgentStep[] {
  const hidden = new Set<string>();
  let previous: AgentStep | undefined;
  for (const step of steps) {
    if (step.messagePhase !== "commentary") {
      previous = undefined;
      continue;
    }
    const text = step.text?.trim();
    if (!text) { hidden.add(step.id); continue; }
    if (previous?.text?.trim() === text) hidden.add(previous.id);
    previous = step;
  }
  return steps.filter(step => !hidden.has(step.id));
}

export type ConversationOperationIcon = "read" | "search" | "write" | "analysis" | "arrange" | "operation";

export function operationIcon(steps: AgentStep[]): ConversationOperationIcon {
  const step = [...steps].reverse().find(item => item.status === "running" || item.status === "waiting") ?? steps[0];
  const category = step?.operation?.category;
  if (category === "read") return /^(search_|query_|resolve_)/.test(step.operation!.tool) ? "search" : "read";
  return category && category !== "diagnostic" ? category : "operation";
}

function operationLabel(step: AgentStep) {
  const operation = step.operation;
  const action = operation?.category === "read" && /^read_/.test(operation.tool) ? `${operation.contentScope === "index" ? "查看" : "读取"}${operation.object}`
    : operation?.category === "write" && /^save_/.test(operation.tool) ? `保存${operation.object}`
      : step.title.replace(/^已/, "");
  if (step.status === "running") return `正在${action}`;
  if (step.status === "waiting") return `等待：${action}`;
  if (step.status !== "completed") return action;
  if (operation?.outcome === "saved") return `已保存${operation.object}`;
  if (operation?.outcome === "isolatedCandidate") return `已保存${operation.object}，待整合`;
  if (operation?.outcome === "paused") return `已暂停${operation.object}`;
  if (operation?.category === "write") return action;
  if (operation?.outcome === "read") return `已${action}`;
  if (!operation && step.title.startsWith("已")) return step.title;
  // A completed call alone does not prove that the requested source was read.
  return `已执行：${action}`;
}

export function operationSummary(steps: AgentStep[]) {
  const active = [...steps].reverse().find(step => step.status === "running" || step.status === "waiting");
  const ordered = active ? [active, ...steps.filter(step => step !== active)] : steps;
  const labels = [...new Set(ordered.map(operationLabel))];
  const content = `${labels.slice(0, 2).join(" · ")}${labels.length > 2 ? "等" : ""}`;
  const failed = steps.some(step => step.status === "failed") ? steps.length === 1 ? " · 未完成" : " · 部分操作未完成" : "";
  const cancelled = steps.some(step => step.status === "cancelled") ? " · 已停止" : "";
  const unconfirmed = steps.some(step => step.status === "completed" && step.operation?.category === "write"
    && !["saved", "isolatedCandidate"].includes(step.operation.outcome ?? "")) ? " · 保存结果待核实" : "";
  const pauseUnconfirmed = steps.some(step => step.status === "completed" && step.operation?.tool === "pause_generation_run"
    && step.operation.outcome !== "paused") ? " · 暂停状态待核对" : "";
  return `${content}${failed}${cancelled}${unconfirmed}${pauseUnconfirmed}`;
}

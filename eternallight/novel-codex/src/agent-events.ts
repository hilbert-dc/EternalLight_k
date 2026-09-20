const MAX_DISPLAY_TEXT = 4096;
const MAX_DETAIL_TEXT = 2048;

export type AgentRunState = "idle" | "running" | "waiting" | "completed" | "cancelled" | "error";
export type AgentStepKind = "system" | "assistant" | "tool" | "interaction" | "approval" | "error";
export type AgentStepStatus = "running" | "waiting" | "completed" | "failed";
export type AgentPhaseId = "context" | "planning" | "tools" | "response";
export type AgentPhaseStatus = "pending" | "running" | "waiting" | "completed" | "failed" | "skipped";
export type AgentMode = "novel-agent-engine";
export type AgentPlanStatus = "pending" | "inProgress" | "completed";
export type AgentConnectionState = "offline" | "connecting" | "ready" | "reconnecting" | "error";
export type AgentReviewState = "idle" | "running" | "completed" | "failed";

export interface AgentStep {
  id: string;
  kind: AgentStepKind;
  title: string;
  detail?: string;
  text?: string;
  status: AgentStepStatus;
}

export interface AgentPhase {
  id: AgentPhaseId;
  label: string;
  status: AgentPhaseStatus;
  detail?: string;
}

export interface AgentArtifact {
  id: string;
  kind: "file-change" | "chapter-change" | "other";
  title: string;
  detail?: string;
  status: "proposed" | "applied" | "declined" | "failed";
}

export interface AgentPlanStep {
  id: string;
  label: string;
  status: AgentPlanStatus;
}

export interface AgentRunSummary {
  id: string;
  mode: AgentMode;
  state: Exclude<AgentRunState, "idle" | "running" | "waiting">;
  prompt: string;
  summary: string;
  artifactCount: number;
  createdAt: number;
}

export interface AgentTokenUsage {
  inputTokens: number | null;
  outputTokens: number | null;
  totalTokens: number | null;
  contextWindow: number | null;
}

export interface AgentRun {
  state: AgentRunState;
  mode: AgentMode;
  threadId: string | null;
  turnId: string | null;
  prompt: string;
  answer: string;
  summary: string;
  error: string | null;
  phases: AgentPhase[];
  plan: AgentPlanStep[];
  steps: AgentStep[];
  artifacts: AgentArtifact[];
  diff: string;
  history: AgentRunSummary[];
  connectionState: AgentConnectionState;
  startedAt: number | null;
  completedAt: number | null;
  tokenUsage: AgentTokenUsage | null;
  contextCompactions: number;
  model: string | null;
  provider: string | null;
  review: AgentReviewState;
  diagnostics: string[];
}

type JsonRecord = Record<string, unknown>;

const phaseOrder: AgentPhaseId[] = ["context", "planning", "tools", "response"];
const phaseLabels: Record<AgentPhaseId, string> = {
  context: "读取上下文",
  planning: "制定计划",
  tools: "执行工具",
  response: "生成回复",
};

function record(value: unknown): JsonRecord | null {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as JsonRecord : null;
}

function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value : null;
}

function isUserInputMethod(method: string): boolean {
  return method === "item/tool/requestUserInput";
}

function userInputDetail(params: JsonRecord | null): string {
  const questions = Array.isArray(params?.questions) ? params.questions.slice(0, 3) : [];
  const lines = questions.map((entry, index) => {
    const question = record(entry);
    return `${index + 1}. ${bounded(stringValue(question?.question) ?? "请提供你的选择", 300)}`;
  });
  return lines.length > 0 ? lines.join("\n") : "Agent 正在等待你的回答";
}

function bounded(value: string, limit: number): string {
  return value.length <= limit ? value : `${value.slice(0, limit)}\n…内容已截断`;
}

function stepStatus(value: unknown, fallback: AgentStepStatus): AgentStepStatus {
  if (value === "failed" || value === "declined") return "failed";
  if (value === "completed") return "completed";
  return fallback;
}

function phaseStatus(value: unknown, fallback: AgentPhaseStatus): AgentPhaseStatus {
  if (value === "failed") return "failed";
  if (value === "completed") return "completed";
  if (value === "skipped") return "skipped";
  if (value === "waiting" || value === "pending" || value === "running") return value;
  return fallback;
}

function planStatus(value: unknown): AgentPlanStatus {
  if (value === "inProgress") return "inProgress";
  if (value === "completed") return "completed";
  return "pending";
}

function planSteps(value: unknown): AgentPlanStep[] {
  if (!Array.isArray(value)) return [];
  return value.slice(0, 8).map((entry, index) => {
    const item = record(entry);
    const label = stringValue(item?.step) ?? `计划步骤 ${index + 1}`;
    return { id: `plan:${index}`, label: bounded(label, 240), status: planStatus(item?.status) };
  });
}

function createPhases(): AgentPhase[] {
  return phaseOrder.map((id) => ({ id, label: phaseLabels[id], status: "pending" }));
}

function upsertStep(run: AgentRun, step: AgentStep): AgentRun {
  const index = run.steps.findIndex((item) => item.id === step.id);
  if (index < 0) return { ...run, steps: [...run.steps, step] };
  const steps = [...run.steps];
  steps[index] = { ...steps[index], ...step };
  return { ...run, steps };
}

function advancePhase(run: AgentRun, id: AgentPhaseId, status: AgentPhaseStatus, detail?: string): AgentRun {
  const targetIndex = phaseOrder.indexOf(id);
  const phases = run.phases.map((phase) => {
    const index = phaseOrder.indexOf(phase.id);
    if (index < targetIndex && phase.status === "pending") return { ...phase, status: (id === "response" && phase.id === "tools" ? "skipped" : "completed") as AgentPhaseStatus };
    if (phase.id === id) return { ...phase, status, detail: detail ? bounded(detail, 300) : phase.detail };
    return phase;
  });
  return { ...run, phases };
}

function completePhases(run: AgentRun): AgentRun {
  return { ...run, phases: run.phases.map((phase) => phase.status === "running" || phase.status === "waiting" ? { ...phase, status: "completed" } : phase) };
}

function failActivePhases(run: AgentRun): AgentRun {
  return { ...run, phases: run.phases.map((phase) => phase.status === "running" || phase.status === "waiting" ? { ...phase, status: "failed" } : phase) };
}

function completedSteps(run: AgentRun): AgentStep[] {
  return run.steps.map((step) => step.status === "running" || step.status === "waiting" ? { ...step, status: "completed" } : step);
}

function itemDetail(item: JsonRecord): string | undefined {
  const command = stringValue(item.command);
  const cwd = stringValue(item.cwd);
  const changes = Array.isArray(item.changes) ? item.changes.map(record).filter((change): change is JsonRecord => change !== null) : [];
  const paths = changes.map((change) => stringValue(change.path)).filter((path): path is string => path !== null);
  const detail = [command, cwd ? `工作目录：${cwd}` : null, ...paths.map((path) => `文件：${path}`)].filter(Boolean).join("\n");
  return detail ? bounded(detail, MAX_DETAIL_TEXT) : undefined;
}

function collabAgentTitle(tool: string | null): string {
  if (tool === "spawnAgent") return "委派小说审阅";
  if (tool === "wait") return "等待子 Agent";
  if (tool === "sendMessage" || tool === "followupTask") return "协调子 Agent";
  if (tool === "interruptAgent") return "停止子 Agent";
  if (tool === "listAgents") return "检查子 Agent 状态";
  return "协同小说分析";
}

function collabAgentDetail(item: JsonRecord): string | undefined {
  const receivers = Array.isArray(item.receiverThreadIds) ? item.receiverThreadIds.length : 0;
  const states = record(item.agentsStates);
  const stateCounts = new Map<string, number>();
  for (const value of Object.values(states ?? {})) {
    const status = stringValue(record(value)?.status) ?? "unknown";
    stateCounts.set(status, (stateCounts.get(status) ?? 0) + 1);
  }
  const statusLabels: Record<string, string> = {
    pendingInit: "准备中",
    running: "分析中",
    completed: "已完成",
    errored: "失败",
    interrupted: "已停止",
    shutdown: "已关闭",
  };
  const stateSummary = [...stateCounts.entries()]
    .map(([status, count]) => `${statusLabels[status] ?? status} ${count}`)
    .join(" · ");
  const prompt = stringValue(item.prompt);
  const detail = [receivers > 0 ? `${receivers} 位子 Agent` : null, stateSummary || null, prompt ? `任务：${prompt}` : null]
    .filter(Boolean)
    .join("\n");
  return detail ? bounded(detail, MAX_DETAIL_TEXT) : undefined;
}

function itemId(item: JsonRecord, fallback: string): string {
  return stringValue(item.id) ?? fallback;
}

function itemStep(item: JsonRecord, status: AgentStepStatus): AgentStep {
  const type = stringValue(item.type) ?? "unknown";
  const normalized = type.toLowerCase();
  if (normalized === "collabagenttoolcall") return { id: itemId(item, `item:${type}`), kind: "interaction", title: collabAgentTitle(stringValue(item.tool)), detail: collabAgentDetail(item), status };
  if (normalized.includes("contextcompaction")) return { id: itemId(item, `item:${type}`), kind: "system", title: "压缩上下文", detail: itemDetail(item) ?? "正在整理历史上下文", status };
  if (normalized.includes("hook")) return { id: itemId(item, `item:${type}`), kind: "system", title: "运行 Hook", detail: itemDetail(item), status };
  if (normalized.includes("enteredreview") || normalized.includes("exitedreview") || normalized.includes("review")) return { id: itemId(item, `item:${type}`), kind: "system", title: "审查任务", detail: itemDetail(item), status };
  if (normalized.includes("command") || normalized.includes("shell")) return { id: itemId(item, `item:${type}`), kind: "tool", title: "执行命令", detail: itemDetail(item), status };
  if (normalized.includes("file") || normalized.includes("patch") || normalized.includes("change")) return { id: itemId(item, `item:${type}`), kind: "tool", title: "修改文件", detail: itemDetail(item), status };
  if (normalized.includes("mcp") || normalized.includes("tool")) return { id: itemId(item, `item:${type}`), kind: "tool", title: "调用工具", detail: itemDetail(item), status };
  if (normalized.includes("reason")) return { id: itemId(item, `item:${type}`), kind: "system", title: "分析上下文", detail: itemDetail(item), status };
  if (normalized.includes("agentmessage") || normalized.includes("message")) return { id: "agent-message", kind: "assistant", title: "Agent 回复", text: bounded(stringValue(item.text) ?? "", MAX_DISPLAY_TEXT), status };
  return { id: itemId(item, `item:${type}`), kind: "system", title: "执行步骤", detail: itemDetail(item) ?? type, status };
}

function phaseForItem(item: JsonRecord): AgentPhaseId {
  const type = (stringValue(item.type) ?? "").toLowerCase();
  if (type === "collabagenttoolcall") return "tools";
  if (type.includes("agentmessage") || type.includes("message")) return "response";
  if (type.includes("reason") || type.includes("plan")) return "planning";
  return "tools";
}

function artifactFromItem(item: JsonRecord, status: "proposed" | "applied" | "declined" | "failed"): AgentArtifact | null {
  const type = (stringValue(item.type) ?? "").toLowerCase();
  const changes = Array.isArray(item.changes) ? item.changes.map(record).filter((change): change is JsonRecord => change !== null) : [];
  const path = changes.map((change) => stringValue(change.path)).find((value): value is string => value !== null) ?? null;
  const diff = changes.map((change) => stringValue(change.diff)).filter((value): value is string => value !== null).join("\n");
  if (!path && !type.includes("file") && !type.includes("change") && !type.includes("patch")) return null;
  return {
    id: itemId(item, `artifact:${path ?? type}`),
    kind: type.includes("chapter") ? "chapter-change" : path ? "file-change" : "other",
    title: path ?? "内容变更",
    detail: diff ? bounded(diff, MAX_DETAIL_TEXT) : undefined,
    status,
  };
}

function upsertArtifact(run: AgentRun, artifact: AgentArtifact): AgentRun {
  const index = run.artifacts.findIndex((item) => item.id === artifact.id);
  if (index < 0) return { ...run, artifacts: [...run.artifacts, artifact] };
  const artifacts = [...run.artifacts];
  artifacts[index] = { ...artifacts[index], ...artifact };
  return { ...run, artifacts };
}

function errorMessage(message: JsonRecord, params: JsonRecord | null): string {
  const nestedError = record(params?.error) ?? record(message.error);
  return bounded(stringValue(nestedError?.message) ?? stringValue(params?.message) ?? stringValue(message.message) ?? "创作引擎执行失败", MAX_DETAIL_TEXT);
}

function numberValue(value: unknown): number | null { return typeof value === "number" && Number.isFinite(value) && value >= 0 ? Math.round(value) : null; }

function tokenUsage(value: unknown, previous: AgentTokenUsage | null): AgentTokenUsage | null {
  const usage = record(value);
  const total = record(usage?.total);
  if (!usage || !total) return previous;
  const inputTokens = numberValue(total.inputTokens);
  const outputTokens = numberValue(total.outputTokens);
  const totalTokens = numberValue(total.totalTokens);
  const contextWindow = numberValue(usage.modelContextWindow);
  if (inputTokens === null && outputTokens === null && totalTokens === null && contextWindow === null) return previous;
  return { inputTokens, outputTokens, totalTokens, contextWindow };
}

function artifactStatus(item: JsonRecord, isCompleted: boolean): AgentArtifact["status"] {
  if (!isCompleted) return "proposed";
  const status = stringValue(item.status);
  if (status === "declined") return "declined";
  if (status === "failed") return "failed";
  return "applied";
}

export function createAgentRun(prompt = "", history: AgentRunSummary[] = []): AgentRun {
  const mode: AgentMode = "novel-agent-engine";
  if (!prompt) return { state: "idle", mode, threadId: null, turnId: null, prompt: "", answer: "", summary: "", error: null, phases: createPhases(), plan: [], steps: [], artifacts: [], diff: "", history, connectionState: "offline", startedAt: null, completedAt: null, tokenUsage: null, contextCompactions: 0, model: null, provider: null, review: "idle", diagnostics: [] };
  return {
    state: "running",
    mode,
    threadId: null,
    turnId: null,
    prompt,
    answer: "",
    summary: "",
    error: null,
    phases: createPhases(),
    plan: [],
    steps: [{ id: "request", kind: "system", title: "已发送创作指令", detail: "等待 Novel Agent Engine 响应", status: "running" }],
    artifacts: [],
    diff: "",
    history,
    connectionState: "connecting",
    startedAt: null,
    completedAt: null,
    tokenUsage: null,
    contextCompactions: 0,
    model: null,
    provider: null,
    review: "idle",
    diagnostics: [],
  };
}

export function summarizeAgentRun(run: AgentRun, createdAt = Date.now()): AgentRunSummary | null {
  if (run.state === "idle" || run.state === "running" || run.state === "waiting") return null;
  const summary = run.summary || run.error || run.answer || "本轮没有可显示的结果。";
  return {
    id: `${createdAt}-${run.prompt.slice(0, 24)}`,
    mode: run.mode,
    state: run.state,
    prompt: bounded(run.prompt, 240),
    summary: bounded(summary, 320),
    artifactCount: run.artifacts.length,
    createdAt,
  };
}

export function reduceAgentEvent(run: AgentRun, event: unknown): AgentRun {
  const message = record(event);
  const method = stringValue(message?.method);
  if (!message || !method) return run;
  const params = record(message.params);
  const eventThread = record(params?.thread);
  const eventThreadId = stringValue(params?.threadId) ?? stringValue(eventThread?.id);
  if (eventThreadId && run.threadId && eventThreadId !== run.threadId) return run;

  if (method === "agent/connection") {
    const state = params?.state;
    const connectionState = state === "connecting" || state === "ready" || state === "reconnecting" || state === "error" || state === "offline" ? state : run.connectionState;
    return { ...run, connectionState };
  }

  if (method === "agent/mode") {
    return { ...run, mode: "novel-agent-engine" };
  }

  if (method === "thread/started") {
    const thread = record(params?.thread);
    const threadId = stringValue(params?.threadId) ?? stringValue(thread?.id);
    return threadId ? { ...run, threadId, connectionState: "ready" } : run;
  }

  if (method === "thread/closed") {
    const threadId = stringValue(params?.threadId);
    if (threadId && run.threadId && threadId !== run.threadId) return run;
    return { ...run, connectionState: "reconnecting", diagnostics: [...run.diagnostics, "当前线程已关闭，可恢复后继续。"].slice(-6) };
  }

  if (method === "thread/tokenUsage/updated") {
    return { ...run, tokenUsage: tokenUsage(params?.tokenUsage, run.tokenUsage) };
  }

  if (method === "model/rerouted") {
    const fromModel = stringValue(params?.fromModel) ?? "当前模型";
    const toModel = stringValue(params?.toModel) ?? "备用模型";
    const reason = stringValue(params?.reason);
    return upsertStep({ ...run, model: toModel }, { id: `model-rerouted:${Date.now()}`, kind: "system", title: "模型已切换", detail: `${fromModel} → ${toModel}${reason ? `\n原因：${reason}` : ""}`, status: "completed" });
  }

  if (method === "agent/review") {
    return upsertStep({ ...run, review: "running" }, { id: "review:start", kind: "system", title: "开始审查任务", detail: "正在检查当前工作区的文件变更。", status: "running" });
  }

  if (method === "hook/started" || method === "hook/completed") {
    const hook = record(params?.run) ?? params;
    const hookId = stringValue(params?.hookId) ?? stringValue(hook?.id) ?? method;
    const status = method === "hook/completed" ? stepStatus(hook?.status, "completed") : "running";
    return upsertStep(run, { id: `hook:${hookId}`, kind: "system", title: "运行 Hook", detail: stringValue(hook?.name) ?? stringValue(hook?.command) ?? undefined, status });
  }

  if (method === "turn/plan/updated") {
    const plan = planSteps(params?.plan);
    const turnId = stringValue(params?.turnId) ?? run.turnId;
    const hasActiveStep = plan.some((step) => step.status === "inProgress");
    const isComplete = plan.length > 0 && plan.every((step) => step.status === "completed");
    const status: AgentPhaseStatus = hasActiveStep ? "running" : isComplete ? "completed" : "pending";
    return advancePhase({ ...run, turnId, plan }, "planning", status, hasActiveStep ? "正在执行计划中的下一步" : undefined);
  }

  if (method === "agent/workflow") {
    const workflow = record(params?.workflow);
    const context = record(workflow?.context);
    const task = record(workflow?.task);
    const factCount = Array.isArray(context?.facts) ? context.facts.length : 0;
    const purpose = stringValue(context?.purpose) ?? "planner";
    return upsertStep(run, { id: "workflow:prepared", kind: "system", title: "准备创作上下文", detail: `${purpose} · ${factCount} 条时态事实 · 任务 ${stringValue(task?.id) ?? "已绑定"}`, status: "completed" });
  }

  if (method === "turn/diff/updated") {
    const diff = stringValue(params?.diff);
    const turnId = stringValue(params?.turnId) ?? run.turnId;
    return diff ? { ...run, turnId, diff: bounded(diff, MAX_DETAIL_TEXT) } : { ...run, turnId };
  }

  if (method === "agent/compactionStarted") {
    return upsertStep({ ...run, contextCompactions: run.contextCompactions + 1 }, { id: "context-compaction", kind: "system", title: "压缩上下文", detail: "正在整理历史上下文，保留当前故事约束。", status: "running" });
  }

  if (method === "agent/compactionCompleted") {
    return upsertStep(run, { id: "context-compaction", kind: "system", title: "压缩上下文", detail: "上下文压缩完成。", status: "completed" });
  }

  if (method === "agent/steer") {
    const prompt = stringValue(params?.prompt);
    if (!prompt) return run;
    return upsertStep({ ...run, state: "running", error: null }, { id: `steer:${Date.now()}`, kind: "system", title: "已追加创作指令", detail: bounded(prompt, MAX_DETAIL_TEXT), status: "completed" });
  }

  if (method === "agent/phase" || method === "agent/phase/started" || method === "agent/phase/completed") {
    const id = params?.id;
    if (!phaseOrder.includes(id as AgentPhaseId)) return run;
    const phaseId = id as AgentPhaseId;
    const fallback = method.endsWith("/completed") ? "completed" : method.endsWith("/started") ? "running" : "running";
    const status = phaseStatus(params?.status, fallback);
    return advancePhase(run, phaseId, status, stringValue(params?.detail) ?? undefined);
  }

  if (method === "turn/started") {
    const turn = record(params?.turn);
    const turnId = stringValue(turn?.id) ?? "active";
    const threadId = stringValue(params?.threadId) ?? run.threadId;
    return advancePhase(upsertStep({ ...run, state: "running", error: null, summary: "", threadId, turnId, connectionState: "ready", startedAt: run.startedAt ?? Date.now(), completedAt: null }, { id: `turn:${turnId}`, kind: "system", title: "Agent 开始执行", detail: "正在读取故事上下文并规划下一步", status: "running" }), "context", "running", "正在读取故事上下文");
  }

  if (method === "item/agentMessage/delta") {
    const delta = stringValue(params?.delta);
    if (!delta) return run;
    const existing = run.steps.find((step) => step.id === "agent-message");
    const text = bounded(`${existing?.text ?? run.answer}${delta}`, MAX_DISPLAY_TEXT);
    return advancePhase(upsertStep({ ...run, state: "running", answer: text, error: null }, { id: "agent-message", kind: "assistant", title: "Agent 回复", text, status: "running" }), "response", "running", "正在接收 Agent 回复");
  }

  if (method.endsWith("/delta") || method.endsWith("outputDelta")) {
    const delta = stringValue(params?.delta);
    if (!delta) return run;
    const item = record(params?.item) ?? {};
    const id = stringValue(params?.itemId) ?? itemId(item, `stream:${method}`);
    const existing = run.steps.find((step) => step.id === id);
    const detail = bounded(`${existing?.detail ? `${existing.detail}\n` : ""}${delta}`, MAX_DETAIL_TEXT);
    return advancePhase(upsertStep({ ...run, state: "running", error: null }, { id, kind: "tool", title: "执行工具", detail, status: "running" }), "tools", "running", "正在接收工具输出");
  }

  if (method === "item/started" || method === "item/completed") {
    const item = record(params?.item) ?? {};
    const isCompleted = method === "item/completed";
    const status = isCompleted ? stepStatus(item.status, "completed") : "running";
    const step = itemStep(item, status);
    const normalizedType = (stringValue(item.type) ?? "").toLowerCase();
    const isReviewItem = normalizedType.includes("review");
    const next = advancePhase(upsertStep({ ...run, state: isReviewItem ? run.state : "running", error: null }, step), phaseForItem(item), status === "failed" ? "failed" : status === "completed" ? "completed" : "running", itemDetail(item));
    const text = stringValue(item.text);
    const withAnswer = step.kind === "assistant" && text ? { ...next, answer: bounded(text, MAX_DISPLAY_TEXT) } : next;
    const withMetadata = {
      ...withAnswer,
      contextCompactions: withAnswer.contextCompactions + (isCompleted && normalizedType.includes("contextcompaction") ? 1 : 0),
      review: normalizedType.includes("enteredreview") ? "running" : normalizedType.includes("exitedreview") ? "completed" : withAnswer.review,
    } as AgentRun;
    const artifact = artifactFromItem(item, artifactStatus(item, isCompleted));
    return artifact ? upsertArtifact(withMetadata, artifact) : withMetadata;
  }

  if (isUserInputMethod(method)) {
    return advancePhase(upsertStep({ ...run, state: "waiting", error: null }, { id: `user-input:${String(message.id ?? "active")}`, kind: "interaction", title: "等待你的回答", detail: bounded(userInputDetail(params), MAX_DETAIL_TEXT), status: "waiting" }), "tools", "waiting", "等待回答后继续");
  }

  if (method === "agent/softwarePermissionDenied") {
    const requestId = String(params?.requestId ?? "unknown");
    const requestMethod = stringValue(params?.requestMethod) ?? "未知操作";
    const reason = stringValue(params?.reason) ?? "产品引擎禁止请求软件权限";
    return upsertStep(run, { id: `permission-denied:${requestId}`, kind: "system", title: "已阻止软件权限请求", detail: bounded(`${reason}\n请求：${requestMethod}`, MAX_DETAIL_TEXT), status: "completed" });
  }

  if (method === "serverRequest/resolved") {
    const requestId = stringValue(params?.requestId) ?? "active";
    const next = run.state === "waiting" ? advancePhase({ ...run, state: "running" }, "tools", "running", "等待请求已处理，继续执行") : run;
    return upsertStep(next, { id: `user-input-resolved:${requestId}`, kind: "system", title: "作者回答已提交", detail: "Novel Agent Engine 已收到你的回答，并继续处理。", status: "completed" });
  }

  if (method === "turn/completed") {
    const turn = record(params?.turn);
    if (stringValue(turn?.status) === "failed") return reduceAgentEvent(run, { method: "error", params: { error: turn?.error } });
    if (stringValue(turn?.status) === "interrupted") {
      return upsertStep({ ...completePhases(run), state: "cancelled", error: null, steps: completedSteps(run), summary: "本轮已停止，可重新发送指令。" }, { id: "turn:completed", kind: "system", title: "本轮已停止", detail: "已停止继续执行，可重新发送指令", status: "completed" });
    }
    const items = Array.isArray(turn?.items) ? turn.items : [];
    const finalItem = items.map(record).find((item) => item && stringValue(item.type)?.toLowerCase().includes("agentmessage"));
    const finalText = finalItem ? stringValue(finalItem.text) : null;
    const answer = bounded(finalText ?? run.answer, MAX_DISPLAY_TEXT);
    let next: AgentRun = { ...completePhases(run), state: "completed", answer, error: null, steps: completedSteps(run), summary: bounded(stringValue(turn?.summary) ?? "创作引擎已完成本轮创作。", MAX_DETAIL_TEXT), connectionState: "ready", completedAt: Date.now(), review: run.review === "running" ? "completed" : run.review };
    if (finalText) next = upsertStep(next, { id: "agent-message", kind: "assistant", title: "Agent 回复", text: answer, status: "completed" });
    return upsertStep(next, { id: "turn:completed", kind: "system", title: "本轮执行完成", detail: next.summary, status: "completed" });
  }

  if (method === "error" || method === "agent/error") {
    const error = errorMessage(message, params);
    return upsertStep({ ...failActivePhases(run), state: "error", error, summary: "本轮未完成，可检查错误后重试。", steps: run.steps.map((step) => step.status === "running" || step.status === "waiting" ? { ...step, status: "failed" } : step), connectionState: method === "agent/error" ? "error" : run.connectionState, completedAt: Date.now(), diagnostics: [...run.diagnostics, error].slice(-6) }, { id: `error:${Date.now()}`, kind: "error", title: "执行失败", detail: error, status: "failed" });
  }

  if (method === "agent/stderr") {
    const text = stringValue(params?.text);
    return text ? upsertStep(run, { id: `stderr:${Date.now()}`, kind: "system", title: "运行时提示", detail: bounded(text, MAX_DETAIL_TEXT), status: "completed" }) : run;
  }

  if (method === "thread/status/changed") {
    const status = record(params?.status);
    if (stringValue(status?.type) === "notLoaded" && run.threadId) return { ...run, connectionState: "reconnecting" };
    if (stringValue(status?.type) === "idle" && run.state === "running") return { ...completePhases(run), state: "completed", summary: run.summary || "创作引擎已结束当前轮次。", steps: completedSteps(run) };
  }

  return run;
}

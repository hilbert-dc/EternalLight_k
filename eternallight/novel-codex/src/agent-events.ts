import { snapshotExecution, retainRecentExecutions, type DisplayExecution } from "./conversation-process";
import { collaborationFromItem, type Collaborator } from "./collaboration-progress";
import type { DiscussionExcerpt } from "./types";

const MAX_DISPLAY_TEXT = 4096;
const MAX_DETAIL_TEXT = 2048;

export type AgentRunState = "idle" | "running" | "waiting" | "completed" | "cancelled" | "error";
export type AgentStepKind = "system" | "assistant" | "tool" | "interaction" | "approval" | "error";
export type AgentStepStatus = "running" | "waiting" | "completed" | "failed" | "cancelled";
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
  messagePhase?: "commentary" | "final_answer" | null;
  sourceType?: "reasoning";
  collaborators?: Collaborator[];
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
  excerpts?: readonly DiscussionExcerpt[];
  origin?: "chapterStructure";
  id: string;
  threadId: string | null;
  turnId: string | null;
  taskId?: string | null;
  mode: AgentMode;
  state: Exclude<AgentRunState, "idle" | "running" | "waiting">;
  prompt: string;
  answer: string;
  summary: string;
  artifactCount: number;
  createdAt: number;
  displayExecution?: DisplayExecution | null;
}

export interface AgentTokenUsage {
  inputTokens: number | null;
  outputTokens: number | null;
  totalTokens: number | null;
  contextWindow: number | null;
}

export interface AgentRun {
  excerpts?: readonly DiscussionExcerpt[];
  origin?: "chapterStructure";
  state: AgentRunState;
  mode: AgentMode;
  threadId: string | null;
  turnId: string | null;
  taskId?: string | null;
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
  if (value === "cancelled" || value === "interrupted") return "cancelled";
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
  if (step.collaborators) steps[index].collaborators = step.collaborators.map(agent => ({ ...agent,
    task: agent.task ?? run.steps[index].collaborators?.find(previous => previous.id === agent.id)?.task }));
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
  const truncated = item.displayTruncated === true;
  const detail = [truncated ? "输出过长，已省略" : null, command, cwd ? `工作目录：${cwd}` : null, ...paths.map((path) => `文件：${path}`)].filter(Boolean).join("\n");
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
    notFound: "未找到",
  };
  const stateSummary = [...stateCounts.entries()]
    .map(([status, count]) => `${statusLabels[status] ?? status} ${count}`)
    .join(" · ");
  const prompt = stringValue(item.prompt);
  const results = Object.entries(states ?? {}).flatMap(([id, value]) => {
    const state = record(value);
    const status = stringValue(state?.status);
    const message = stringValue(state?.message);
    if (!message || (status !== "completed" && status !== "errored")) return [];
    return [`${bounded(id, 80)} · ${statusLabels[status]}：${bounded(message, 1200)}`];
  });
  const detail = [receivers > 0 ? `${receivers} 位子 Agent` : null, stateSummary || null, prompt ? `任务：${bounded(prompt, 1200)}` : null, ...results]
    .filter(Boolean)
    .join("\n");
  return detail ? bounded(detail, MAX_DETAIL_TEXT) : undefined;
}

function itemId(item: JsonRecord, fallback: string): string {
  return stringValue(item.id) ?? fallback;
}

const TOOL_TITLES: Record<string, string> = {
  read_context: "读取故事上下文",
  read_task_source: "读取写作资料",
  read_work_status: "核对创作进展",
  request_work_action: "安排下一步",
  request_generation_handoff: "准备续写安排",
  read_chapter_structure_source: "读取删前章节",
  save_brief: "保存创作简报",
  save_outline: "保存大纲",
  save_chapter_plans: "保存章纲",
  save_draft_checkpoint: "保存章节",
  publish_knowledge_batch: "保存故事设定分析",
  query_story: "查询故事证据",
  read_creative_strategy: "参考创作方法",
  read_quality_notebook: "读取创作回看",
  read_quality_review: "核对阅读反馈",
  record_quality_note: "记录作品质量线索",
  read_quality_packet: "阅读匿名文本",
  submit_quality_reading: "保存读者体验报告",
  analyze_story_impact: "检查故事关联影响",
  resolve_story_evidence: "定位正文依据",
  simulate_story_hypothesis: "分析故事假设",
  request_planning_review: "安排多视角规划检查",
  request_candidate_task: "登记本轮创作任务",
  submit_commitment_extraction: "保存伏笔候选",
  search_story: "查找故事内容",
  read_commitment: "读取故事伏笔与承诺",
  search_relations: "查询人物关系",
  read_reading_progress: "读取阅读进度",
  checkpoint_reading: "保存阅读进度",
  submit_revision_outcome: "保存修改检查结果",
  submit_planning_review_report: "提交审阅报告",
  complete_task: "完成任务",
};

function shortLine(value: string, limit = 42): string {
  const line = value.replace(/\s+/g, " ").trim();
  return line.length <= limit ? line : `${line.slice(0, limit - 1)}…`;
}

function commandText(item: JsonRecord): string | null {
  const command = item.command;
  if (typeof command === "string" && command.trim()) return command;
  if (!Array.isArray(command)) return null;
  const parts = command.filter((part): part is string => typeof part === "string" && part.trim().length > 0);
  return parts.length > 0 ? parts.join(" ") : null;
}

function pathLeaf(path: string): string {
  return shortLine(path.split(/[/\\]/).filter(Boolean).pop() ?? path, 32);
}

function toolLeaf(name: string): string {
  const dotted = name.split(".").pop() ?? name;
  const parts = dotted.split("__").filter(Boolean);
  return parts[parts.length - 1] ?? dotted;
}

function toolTitle(item: JsonRecord): string {
  const name = stringValue(item.tool);
  if (!name) return "调用工具";
  const leaf = toolLeaf(name);
  return TOOL_TITLES[leaf] ?? shortLine(leaf.replace(/_/g, " "));
}

function itemStep(item: JsonRecord, status: AgentStepStatus): AgentStep {
  const type = stringValue(item.type) ?? "unknown";
  const normalized = type.toLowerCase();
  if (normalized === "collabagenttoolcall") return { id: itemId(item, `item:${type}`), kind: "interaction", title: collabAgentTitle(stringValue(item.tool)), detail: collabAgentDetail(item), collaborators: collaborationFromItem(item), status };
  if (normalized.includes("contextcompaction")) return { id: itemId(item, `item:${type}`), kind: "system", title: "压缩上下文", detail: itemDetail(item) ?? "正在整理历史上下文", status };
  if (normalized.includes("hook")) return { id: itemId(item, `item:${type}`), kind: "system", title: "运行 Hook", detail: itemDetail(item), status };
  if (normalized.includes("enteredreview") || normalized.includes("exitedreview") || normalized.includes("review")) return { id: itemId(item, `item:${type}`), kind: "system", title: "审查任务", detail: itemDetail(item), status };
  if (normalized.includes("command") || normalized.includes("shell")) {
    const command = commandText(item);
    return { id: itemId(item, `item:${type}`), kind: "tool", title: command ? shortLine(command) : "执行命令", detail: itemDetail(item), status };
  }
  if (normalized.includes("file") || normalized.includes("patch") || normalized.includes("change")) {
    const changes = Array.isArray(item.changes) ? item.changes.map(record).filter((change): change is JsonRecord => change !== null) : [];
    const path = changes.map((change) => stringValue(change.path)).find((value): value is string => value !== null);
    return { id: itemId(item, `item:${type}`), kind: "tool", title: path ? `修改 ${pathLeaf(path)}` : "修改文件", detail: itemDetail(item), status };
  }
  if (normalized.includes("mcp") || normalized.includes("tool")) return { id: itemId(item, `item:${type}`), kind: "tool", title: toolTitle(item), detail: itemDetail(item), status };
  if (normalized === "reasoning") return { id: itemId(item, "item:reasoning"), kind: "system", title: "正在分析", sourceType: "reasoning", status };
  if (type === "agentMessage") return { id: `message:${itemId(item, "unknown")}`, kind: item.phase === "commentary" ? "system" : "assistant", title: item.phase === "commentary" ? "Agent 进度" : "Agent 回复", text: bounded(typeof item.text === "string" ? item.text : "", MAX_DISPLAY_TEXT), status, messagePhase: item.phase === "commentary" || item.phase === "final_answer" ? item.phase : null };
  return { id: itemId(item, `item:${type}`), kind: "system", title: "执行步骤", detail: itemDetail(item) ?? type, status };
}

function phaseForItem(item: JsonRecord): AgentPhaseId {
  const type = (stringValue(item.type) ?? "").toLowerCase();
  if (type === "collabagenttoolcall") return "tools";
  if (type === "agentmessage" && item.phase === "commentary") return "planning";
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

// Each message owns its stream; a commentary item cannot append to or replace
// an answer. A completed explicit final reply takes priority over null-phase
// messages, which are also part of the current app-server contract.
function applyAgentMessage(run: AgentRun, step: AgentStep): AgentRun {
  const next = upsertStep({ ...run, steps: run.steps.filter((entry) => entry.id !== step.id) }, step);
  const messages = next.steps.filter((entry) => entry.id !== "agent-message" && entry.messagePhase !== undefined);
  const final = messages.filter((entry) => entry.messagePhase === "final_answer" && entry.status === "completed").at(-1);
  const selected = final ?? messages.filter((entry) => entry.messagePhase === "final_answer").at(-1)
    ?? messages.filter((entry) => entry.messagePhase === null).at(-1);
  const withoutAnswer = { ...next, steps: next.steps.filter((entry) => entry.id !== "agent-message"), answer: selected?.text ?? "" };
  return selected ? upsertStep(withoutAnswer, { ...selected, id: "agent-message", kind: "assistant", title: "Agent 回复" }) : withoutAnswer;
}

function activeRun(run: AgentRun): boolean {
  return run.state === "running" || run.state === "waiting";
}

export function createAgentRun(prompt = "", history: AgentRunSummary[] = []): AgentRun {
  history = retainRecentExecutions(history);
  const mode: AgentMode = "novel-agent-engine";
  if (!prompt) return { state: "idle", mode, threadId: null, turnId: null, taskId: null, prompt: "", answer: "", summary: "", error: null, phases: createPhases(), plan: [], steps: [], artifacts: [], diff: "", history, connectionState: "offline", startedAt: null, completedAt: null, tokenUsage: null, contextCompactions: 0, model: null, provider: null, review: "idle", diagnostics: [] };
  return {
    state: "running",
    mode,
    threadId: null,
    turnId: null,
    taskId: null,
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
    startedAt: Date.now(),
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
  // Background task recovery has its own durable record; without author input
  // it does not constitute a new local conversation turn.
  if (!run.prompt.trim() || run.state === "idle" || run.state === "running" || run.state === "waiting") return null;
  const summary = run.summary || run.error || run.answer || "本轮没有可显示的结果。";
  return {
    id: `${createdAt}-${run.prompt.slice(0, 24)}`,
    threadId: run.threadId,
    turnId: run.turnId,
    taskId: run.taskId ?? null,
    ...(run.origin ? { origin: run.origin } : {}),
    mode: run.mode,
    state: run.state,
    prompt: bounded(run.prompt, 32_000),
    ...(run.excerpts?.length ? { excerpts: run.excerpts } : {}),
    answer: bounded(run.answer.trim(), MAX_DISPLAY_TEXT),
    summary: bounded(summary, 320),
    artifactCount: run.artifacts.length,
    createdAt,
    displayExecution: snapshotExecution(run),
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
  const turn = record(params?.turn);
  const directTurnId = stringValue(params?.turnId);
  const nestedTurnId = stringValue(turn?.id);
  if (directTurnId && nestedTurnId && directTurnId !== nestedTurnId) return run;
  const eventTurnId = directTurnId ?? nestedTurnId;
  if (eventTurnId && method !== "turn/started" && eventTurnId !== run.turnId) return run;
  if (method === "turn/started" && run.turnId) return run;
  // Native turn/item notifications are scoped. Local control events deliberately
  // use agent/*; the App also synthesizes unscoped turn/completed for stop/skip.
  if ((method.startsWith("item/") || method.endsWith("/delta") || method.endsWith("outputDelta") || method === "turn/plan/updated" || method === "turn/diff/updated"
    || method === "thread/tokenUsage/updated" || method === "model/rerouted" || method === "error") && (!eventTurnId || !eventThreadId)) return run;
  if (method === "turn/completed" && eventThreadId && !eventTurnId) return run;
  if (eventTurnId && method !== "thread/tokenUsage/updated" && method !== "turn/started" && !activeRun(run)) return run;

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
    // The adapter acknowledges inline review after its managed turn completes;
    // it is not a second request to prepare another turn.
    if (params?.delivery === "inline") return upsertStep({ ...run, review: "completed" }, { id: "review:start", kind: "system", title: "审查任务已完成", status: "completed" });
    if (activeRun(run) && run.turnId) return run;
    return upsertStep({ ...run, turnId: null, review: "running" }, { id: "review:start", kind: "system", title: "开始审查任务", detail: "正在检查当前工作区的文件变更。", status: "running" });
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
    if (!activeRun(run)) return run;
    const workflow = record(params?.workflow);
    const context = record(workflow?.context);
    const task = record(workflow?.task);
    const taskId = stringValue(task?.id);
    const threadId = stringValue(task?.threadId);
    if (!taskId || taskId.length > 160 || taskId !== taskId.trim()
      || !threadId || threadId.length > 160 || threadId !== threadId.trim()
      || (run.threadId && run.threadId !== threadId) || (eventThreadId && eventThreadId !== threadId)
      || (run.taskId && run.taskId !== taskId)) return run;
    const factCount = Array.isArray(context?.facts) ? context.facts.length : 0;
    const purpose = stringValue(context?.purpose) ?? "planner";
    return upsertStep({ ...run, taskId, threadId }, { id: "workflow:prepared", kind: "system", title: "准备创作上下文", detail: `${purpose} · ${factCount} 条时态事实 · 任务 ${taskId}`, status: "completed" });
  }

  if (method === "turn/diff/updated") {
    const diff = stringValue(params?.diff);
    const turnId = stringValue(params?.turnId) ?? run.turnId;
    return diff ? { ...run, turnId, diff: bounded(diff, MAX_DETAIL_TEXT) } : { ...run, turnId };
  }

  if (method === "agent/compactionStarted") {
    if (activeRun(run) && run.turnId) return run;
    return upsertStep({ ...run, turnId: null, contextCompactions: run.contextCompactions + 1 }, { id: "context-compaction", kind: "system", title: "压缩上下文", detail: "正在整理历史上下文，保留当前故事约束。", status: "running" });
  }

  if (method === "agent/compactionCompleted") {
    return upsertStep(run, { id: "context-compaction", kind: "system", title: "压缩上下文", detail: "上下文压缩完成。", status: "completed" });
  }

  if (method === "agent/steer") {
    const prompt = stringValue(params?.prompt);
    if (!prompt || run.state !== "running") return run;
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
    const turnId = eventTurnId;
    if (!turnId) return run;
    const threadId = stringValue(params?.threadId) ?? run.threadId;
    const prepared = activeRun(run) ? run : { ...createAgentRun(run.prompt, run.history), ...(run.excerpts ? { excerpts: run.excerpts } : {}), review: run.review, contextCompactions: run.contextCompactions };
    return advancePhase(upsertStep({ ...prepared, state: "running", error: null, summary: "", threadId, turnId, connectionState: "ready", startedAt: prepared.startedAt ?? Date.now(), completedAt: null }, { id: `turn:${turnId}`, kind: "system", title: "Agent 开始执行", detail: "正在读取故事上下文并规划下一步", status: "running" }), "context", "running", "正在读取故事上下文");
  }

  if (method.startsWith("item/reasoning/")) {
    const id = stringValue(params?.itemId);
    if (!id) return run;
    const existing = run.steps.find((step) => step.id === id);
    if (existing && existing.status !== "running" && existing.status !== "waiting") return run;
    return advancePhase(upsertStep(run, itemStep({ id, type: "reasoning" }, "running")), "planning", "running");
  }

  if (method === "item/agentMessage/delta") {
    const delta = typeof params?.delta === "string" ? params.delta : null;
    const id = stringValue(params?.itemId);
    if (!delta || !id) return run;
    const existing = run.steps.find((step) => step.id === `message:${id}`);
    if (existing?.status === "completed") return run;
    const text = bounded(`${existing?.text ?? ""}${delta}`, MAX_DISPLAY_TEXT);
    const step = { ...itemStep({ id, type: "agentMessage", phase: existing?.messagePhase }, "running"), text };
    const next = applyAgentMessage({ ...run, error: null }, step);
    return step.messagePhase === "commentary" ? next : advancePhase(next, "response", "running", "正在接收 Agent 回复");
  }

  if (method.endsWith("/delta") || method.endsWith("outputDelta")) {
    const delta = stringValue(params?.delta);
    if (!delta) return run;
    const item = record(params?.item) ?? {};
    const id = stringValue(params?.itemId) ?? itemId(item, `stream:${method}`);
    const existing = run.steps.find((step) => step.id === id);
    if (existing && existing.status !== "running" && existing.status !== "waiting") return run;
    if (item.type === "reasoning" || existing?.sourceType === "reasoning") return advancePhase(upsertStep(run, itemStep({ id, type: "reasoning" }, "running")), "planning", "running");
    const detail = bounded(`${existing?.detail ? `${existing.detail}\n` : ""}${delta}`, MAX_DETAIL_TEXT);
    return advancePhase(upsertStep({ ...run, error: null }, { id, kind: "tool", title: existing?.title ?? itemStep(item, "running").title, detail, status: "running" }), "tools", run.state === "waiting" ? "waiting" : "running", "正在接收工具输出");
  }

  if (method === "item/started" || method === "item/completed") {
    const item = record(params?.item) ?? {};
    const isCompleted = method === "item/completed";
    const status = isCompleted ? stepStatus(item.status, "completed") : "running";
    const step = itemStep(item, status);
    const existing = run.steps.find((entry) => entry.id === step.id);
    if (existing && existing.status !== "running" && existing.status !== "waiting") return run;
    if (item.type === "agentMessage") {
      if (!stringValue(item.id) || (item.phase != null && item.phase !== "commentary" && item.phase !== "final_answer")) return run;
      // item/started can repeat after deltas; never erase the current item's text.
      const messageStep = isCompleted ? step : { ...step, text: step.text || existing?.text || "" };
      const next = applyAgentMessage({ ...run, error: null }, messageStep);
      return messageStep.messagePhase === "commentary" ? next : advancePhase(next, "response", status === "completed" ? "completed" : "running");
    }
    const normalizedType = (stringValue(item.type) ?? "").toLowerCase();
    const next = advancePhase(upsertStep({ ...run, error: null }, step), phaseForItem(item), status === "failed" ? "failed" : status === "completed" ? "completed" : status === "cancelled" ? "skipped" : "running", normalizedType === "reasoning" ? undefined : itemDetail(item));
    const withMetadata = {
      ...next,
      contextCompactions: next.contextCompactions + (isCompleted && normalizedType.includes("contextcompaction") ? 1 : 0),
      review: normalizedType.includes("enteredreview") ? "running" : normalizedType.includes("exitedreview") ? "completed" : next.review,
    } as AgentRun;
    const artifact = artifactFromItem(item, artifactStatus(item, isCompleted));
    return artifact ? upsertArtifact(withMetadata, artifact) : withMetadata;
  }

  if (isUserInputMethod(method)) {
    if (typeof message.id !== "string" && (typeof message.id !== "number" || !Number.isSafeInteger(message.id))) return run;
    const id = `user-input:${message.id}`;
    if (run.steps.some((step) => step.id === id)) return run;
    return advancePhase(upsertStep({ ...run, state: "waiting", error: null }, { id, kind: "interaction", title: "等待你的回答", detail: bounded(userInputDetail(params), MAX_DETAIL_TEXT), status: "waiting" }), "tools", "waiting", "等待回答后继续");
  }

  if (method === "agent/softwarePermissionDenied") {
    const requestId = String(params?.requestId ?? "unknown");
    const requestMethod = stringValue(params?.requestMethod) ?? "未知操作";
    const reason = stringValue(params?.reason) ?? "产品引擎禁止请求软件权限";
    return upsertStep(run, { id: `permission-denied:${requestId}`, kind: "system", title: "已阻止软件权限请求", detail: bounded(`${reason}\n请求：${requestMethod}`, MAX_DETAIL_TEXT), status: "completed" });
  }

  if (method === "serverRequest/resolved") {
    const requestId = typeof params?.requestId === "string" || typeof params?.requestId === "number" ? String(params.requestId) : null;
    if (!requestId || !activeRun(run)) return run;
    const pending = run.steps.find((step) => step.id === `user-input:${requestId}` && step.status === "waiting");
    if (!pending) return run;
    const resolved = upsertStep(run, { ...pending, status: "completed" });
    const stillWaiting = resolved.steps.some((step) => step.kind === "interaction" && step.status === "waiting");
    const next = run.state === "waiting" && !stillWaiting ? advancePhase({ ...resolved, state: "running" }, "tools", "running", "等待请求已处理，继续执行") : resolved;
    return upsertStep(next, { id: `user-input-resolved:${requestId}`, kind: "system", title: "作者回答已提交", detail: "Novel Agent Engine 已收到你的回答，并继续处理。", status: "completed" });
  }

  if (method === "turn/completed") {
    const turn = record(params?.turn);
    if (!turn || !["completed", "failed", "interrupted"].includes(String(turn.status))) return run;
    if (stringValue(turn?.status) === "failed") return reduceAgentEvent(run, { method: "agent/error", params: { threadId: eventThreadId, turnId: eventTurnId, error: turn?.error } });
    if (stringValue(turn?.status) === "interrupted") {
      return upsertStep({ ...run, state: "cancelled", error: null,
        phases: run.phases.map(phase => phase.status === "running" || phase.status === "waiting" ? { ...phase, status: "skipped" } : phase),
        steps: run.steps.map(step => step.status === "running" || step.status === "waiting" ? { ...step, status: "cancelled" } : step),
        completedAt: Date.now(), summary: "本轮已停止，可重新发送指令。" }, { id: "turn:completed", kind: "system", title: "本轮已停止", detail: "已停止继续执行，可重新发送指令", status: "completed" });
    }
    const items = Array.isArray(turn?.items) ? turn.items : [];
    let reconciled = run;
    for (const entry of items) {
      const item = record(entry);
      if (item?.type === "agentMessage" && stringValue(item.id) && (item.phase == null || item.phase === "commentary" || item.phase === "final_answer")
        && (item.status == null || item.status === "completed")) reconciled = applyAgentMessage(reconciled, itemStep(item, "completed"));
    }
    // An unfinished stream is not final evidence. Completion may omit items when
    // the authoritative item/completed notification was already sent.
    const reply = reconciled.steps.find((step) => step.id === "agent-message" && step.status === "completed");
    const answer = reply?.text ?? "";
    const next: AgentRun = { ...completePhases(reconciled), state: "completed", answer, error: null, steps: completedSteps(reconciled).filter((step) => step.id !== "agent-message" || reply !== undefined), summary: bounded(stringValue(turn?.summary) ?? "创作引擎已完成本轮创作。", MAX_DETAIL_TEXT), connectionState: "ready", completedAt: Date.now(), review: run.review === "running" ? "completed" : run.review };
    return upsertStep(next, { id: "turn:completed", kind: "system", title: "本轮执行完成", detail: next.summary, status: "completed" });
  }

  if (method === "error" || method === "agent/error") {
    const error = errorMessage(message, params);
    if (method === "error" && params?.willRetry === true) {
      return upsertStep({ ...run, diagnostics: [...run.diagnostics, error].slice(-6) }, { id: `retry:${run.turnId}`, kind: "system", title: "正在重试当前请求", detail: error, status: "running" });
    }
    return upsertStep({ ...failActivePhases(run), state: "error", error, summary: "本轮未完成，可检查错误后重试。", steps: run.steps.map((step) => step.status === "running" || step.status === "waiting" ? { ...step, status: "failed" } : step), completedAt: Date.now(), diagnostics: [...run.diagnostics, error].slice(-6) }, { id: `error:${Date.now()}`, kind: "error", title: "执行失败", detail: error, status: "failed" });
  }

  if (method === "agent/stderr") {
    const text = stringValue(params?.text);
    return text ? upsertStep(run, { id: `stderr:${Date.now()}`, kind: "system", title: "运行时提示", detail: bounded(text, MAX_DETAIL_TEXT), status: "completed" }) : run;
  }

  if (method === "thread/status/changed") {
    const status = record(params?.status);
    if (stringValue(status?.type) === "notLoaded" && run.threadId) return { ...run, connectionState: "reconnecting" };
  }

  return run;
}

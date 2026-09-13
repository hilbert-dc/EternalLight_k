import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { createAgentRun, reduceAgentEvent, summarizeAgentRun, type AgentPhaseStatus, type AgentRun } from "./agent-events";
import { ExportDialog } from "./ExportDialog";
import { CreativeWorkspaceView } from "./CreativeWorkspaceView";
import { MetricsPanel } from "./MetricsPanel";
import type { BackgroundTerminal, Chapter, ChapterDiff, ProjectSummary, RuntimeState, RuntimeStatus, Scene, SearchResult, StoryTestResult, ThreadDetail, ThreadSummary, ViewId } from "./types";

import { authorIntentError } from "../shared/author-input-budget.mjs";

const DEFAULT_PROJECT_ID = "project-last-lamp";
const MAX_PENDING_INTERACTIONS = 4;
type AgentStatus = { configured: boolean; state: string; mode: "codex-app-server"; error: string | null; provider?: string; model?: string; commandSource?: string | null; cliPath?: string | null; isolatedHome?: string | null };
type ApprovalRequest = { requestId: string | number; method: string; params: Record<string, unknown>; threadId?: string | null; turnId?: string | null; itemId?: string | null };
type ApprovalDecision = "accept" | "acceptForSession" | "decline" | "cancel";
type ApprovalResponse = { decision: ApprovalDecision } | { answers: Record<string, string | string[]> };
type UserInputQuestion = { id: string; header: string | null; question: string; options: { label: string; description: string | null }[]; isOther: boolean };
type ResolveApproval = (response: ApprovalResponse) => Promise<boolean> | boolean | void;
type PendingNavigation = { kind: "view"; view: ViewId } | { kind: "chapter"; chapterId: string } | { kind: "project"; projectId: string } | { kind: "diffs" } | { kind: "new-project" } | { kind: "close" };
const emptyState: RuntimeState = {
  project: { id: DEFAULT_PROJECT_ID, title: "正在加载作品…", logline: "", updated_at: "" },
  canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] },
  narrativeIr: { version: "1.0", scenes: [], anchors: [] },
  novelEdition: { chapters: [] },
};

const navGroups: { label: string; items: { id: ViewId; label: string; icon: string }[] }[] = [
  { label: "STORY", items: [{ id: "overview", label: "总览", icon: "⌂" }, { id: "chapters", label: "章节草稿", icon: "▤" }, { id: "characters", label: "角色", icon: "♙" }, { id: "scenes", label: "场景", icon: "◈" }] },
  { label: "PLANNING", items: [{ id: "brief", label: "创作简报", icon: "◫" }, { id: "outline", label: "长篇大纲", icon: "≡" }, { id: "plans", label: "近章规划", icon: "⋮" }, { id: "plot", label: "情节线", icon: "⌁" }, { id: "timeline", label: "时间线", icon: "◷" }] },
  { label: "REFERENCE", items: [{ id: "world", label: "世界与规则", icon: "◇" }] },
];

function isResolvedApprovalEvent(event: unknown): event is { method: "serverRequest/resolved"; params: { requestId: string | number } } {
  if (event === null || typeof event !== "object") return false;
  const message = event as { method?: unknown; params?: { requestId?: unknown } };
  return message.method === "serverRequest/resolved" && (typeof message.params?.requestId === "string" || typeof message.params?.requestId === "number");
}

function isUserInputRequest(request: { method: string }): boolean { return request.method === "item/tool/requestUserInput"; }
function userInputQuestions(params: Record<string, unknown>): UserInputQuestion[] { const questions = Array.isArray(params.questions) ? params.questions : []; return questions.slice(0, 3).flatMap((entry) => { const question = eventRecord(entry); if (!question || typeof question.id !== "string" || !question.id.trim() || typeof question.question !== "string") return []; const options = Array.isArray(question.options) ? question.options.slice(0, 6).flatMap((entry) => { const option = eventRecord(entry); return option && typeof option.label === "string" && option.label.trim() ? [{ label: option.label.slice(0, 240), description: typeof option.description === "string" && option.description.trim() ? option.description.slice(0, 240) : null }] : []; }) : []; return [{ id: question.id.slice(0, 240), header: typeof question.header === "string" && question.header.trim() ? question.header.slice(0, 120) : null, question: question.question.slice(0, 240), options, isOther: question.isOther === true }]; }); }

function eventRecord(value: unknown): Record<string, unknown> | null { return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null; }
function eventText(value: unknown): string | null { return typeof value === "string" && value.trim() ? value.slice(0, 240) : null; }
function eventThreadId(event: unknown): string | null { const message = eventRecord(event); const params = eventRecord(message?.params); const thread = eventRecord(params?.thread); const id = typeof params?.threadId === "string" ? params.threadId : typeof thread?.id === "string" ? thread.id : null; return id && id.trim() && id.length <= 240 && !/[\r\n]/.test(id) ? id : null; }
function eventThreadStatus(event: unknown): string | null { const message = eventRecord(event); const params = eventRecord(message?.params); const status = eventRecord(params?.status); const type = eventText(status?.type); const flags = Array.isArray(status?.activeFlags) ? status.activeFlags : []; if (flags.some((flag) => flag === "waitingOnApproval" || flag === "waitingOnUserInput")) return "waiting"; if (type) return type; if (message?.method === "turn/started") return "active"; if (message?.method === "turn/completed") { const turn = eventRecord(params?.turn); return turn?.status === "failed" ? "systemError" : turn?.status === "interrupted" ? "interrupted" : "idle"; } return null; }
function threadSummaryFromEvent(event: unknown): ThreadSummary | null { const message = eventRecord(event); const params = eventRecord(message?.params); const thread = eventRecord(params?.thread); const id = eventThreadId(event); if (!id || !thread) return null; return { id, name: eventText(thread.name), preview: eventText(thread.preview), modelProvider: eventText(thread.modelProvider), status: eventThreadStatus(event) ?? (message?.method === "thread/started" ? "active" : null), createdAt: typeof thread.createdAt === "number" ? thread.createdAt : null, updatedAt: typeof thread.updatedAt === "number" ? thread.updatedAt : null, isPinned: thread.isPinned === true, ephemeral: thread.ephemeral === true }; }
function agentFailureEvent(message: string): unknown { return message === "生成已停止" ? { method: "turn/completed", params: { turn: { status: "interrupted" } } } : { method: "agent/error", params: { message } }; }
function applyThreadEvent(threads: ThreadSummary[], event: unknown, archived: boolean): ThreadSummary[] { const message = eventRecord(event); const method = typeof message?.method === "string" ? message.method : ""; const threadId = eventThreadId(event); if (method === "thread/started") { const incoming = threadSummaryFromEvent(event); if (!incoming || archived) return threads; const existing = threads.find((thread) => thread.id === incoming.id); return existing ? threads.map((thread) => thread.id === incoming.id ? { ...thread, ...incoming, name: incoming.name ?? thread.name, preview: incoming.preview ?? thread.preview, modelProvider: incoming.modelProvider ?? thread.modelProvider, createdAt: incoming.createdAt ?? thread.createdAt, updatedAt: incoming.updatedAt ?? thread.updatedAt } : thread) : [incoming, ...threads]; } if (!threadId) return threads; if (method === "thread/archived" && !archived) return threads.filter((thread) => thread.id !== threadId); if (method === "thread/unarchived" && archived) return threads.filter((thread) => thread.id !== threadId); const status = eventThreadStatus(event); if (["thread/status/changed", "turn/started", "turn/completed"].includes(method) && status) return threads.map((thread) => thread.id === threadId ? { ...thread, status } : thread); if (method === "thread/name/updated") { const params = eventRecord(message?.params); const name = eventText(params?.name); return name ? threads.map((thread) => thread.id === threadId ? { ...thread, name } : thread) : threads; } return threads; }

function App() {
  const api = window.novelCodex;
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    try { return localStorage.getItem("novel-codex-theme") === "dark" ? "dark" : "light"; } catch { return "light"; }
  });
  const [activeView, setActiveView] = useState<ViewId>("overview");
  const [state, setState] = useState<RuntimeState>(emptyState);
  const [activeProjectId, setActiveProjectId] = useState(() => {
    try { return localStorage.getItem("novel-codex-active-project") ?? DEFAULT_PROJECT_ID; } catch { return DEFAULT_PROJECT_ID; }
  });
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [exportOpen, setExportOpen] = useState(false);
  const [projectCreateOpen, setProjectCreateOpen] = useState(false);
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatus>({ state: "offline", mode: "local", error: "正在连接 Creative Runtime…" });
  const [agentStatus, setAgentStatus] = useState<AgentStatus>({ configured: false, state: "idle", mode: "codex-app-server", error: null });
  const [approvalQueue, setApprovalQueue] = useState<ApprovalRequest[]>([]);
  const [selectedChapterId, setSelectedChapterId] = useState("chapter-01");
  const [composer, setComposer] = useState("");
  const [activity, setActivity] = useState<string[]>([]);
  const [agentRun, setAgentRun] = useState<AgentRun>(createAgentRun());
  const [threads, setThreads] = useState<ThreadSummary[]>([]);
  const [threadLoading, setThreadLoading] = useState(false);
  const [threadAction, setThreadAction] = useState<string | null>(null);
  const [threadArchived, setThreadArchived] = useState(false);
  const [threadCursor, setThreadCursor] = useState<string | null>(null);
  const [threadDetail, setThreadDetail] = useState<ThreadDetail | null>(null);
  const [threadDetailLoading, setThreadDetailLoading] = useState(false);
  const [contextTab, setContextTab] = useState<"story" | "task" | "changes" | "usage" | "diagnostics" | "settings">("story");
  const [agentConfigSaving, setAgentConfigSaving] = useState(false);
  const [backgroundTerminals, setBackgroundTerminals] = useState<BackgroundTerminal[]>([]);
  const [backgroundTerminalLoading, setBackgroundTerminalLoading] = useState(false);
  const [creativeRefreshToken, setCreativeRefreshToken] = useState(0);
  const [creativeDirty, setCreativeDirty] = useState(false);
  const [pendingNavigation, setPendingNavigation] = useState<PendingNavigation | null>(null);
  const [savingBeforeLeave, setSavingBeforeLeave] = useState(false);
  const [planningIntentSeed, setPlanningIntentSeed] = useState("");
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
  const creativeSaveHandlerRef = useRef<(() => Promise<boolean>) | null>(null);
  const creativeDiscardHandlerRef = useRef<(() => void) | null>(null);
  const [diffs, setDiffs] = useState<ChapterDiff[]>([]);
  const [testResult, setTestResult] = useState<StoryTestResult | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const activeLabel = useMemo(() => navGroups.flatMap((group) => group.items).find((item) => item.id === activeView)?.label ?? "版本与变更", [activeView]);
  const projectId = state.project.id;
  const selectedChapter = state.novelEdition.chapters.find((chapter) => chapter.id === selectedChapterId) ?? state.novelEdition.chapters[0];
  const agentAvailable = Boolean(api && agentStatus.configured && agentRun.connectionState !== "error" && agentStatus.state !== "error");

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try { localStorage.setItem("novel-codex-theme", theme); } catch { /* Storage can be unavailable in hardened sessions. */ }
  }, [theme]);

  useEffect(() => {
    try { localStorage.setItem("novel-codex-active-project", activeProjectId); } catch { /* Storage can be unavailable in hardened sessions. */ }
  }, [activeProjectId]);

  useEffect(() => {
    if (!api) return;
    return api.app.onCloseRequested(() => {
      if (creativeDirty) setPendingNavigation({ kind: "close" });
      else void api.app.confirmClose();
    });
  }, [api, creativeDirty]);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      if (!api) return;
      try {
        const status = await api.runtime.status();
        if (!cancelled) setRuntimeStatus(status);
        const agent = await api.agent.status();
        if (!cancelled) {
          setAgentStatus(agent);
          setAgentRun((run) => ({ ...run, model: agent.model ?? run.model, provider: agent.provider ?? run.provider, connectionState: agent.configured ? run.connectionState === "error" ? "error" : "ready" : "offline" }));
        }
        try {
          const available = await api.runtime.listProjects();
          if (!cancelled) setProjects(available.projects);
        } catch { /* The active project can still load when list metadata is unavailable. */ }
        const next = await api.runtime.getState(activeProjectId);
        if (!cancelled) { setState(next); setSelectedChapterId(next.novelEdition.chapters[0]?.id ?? ""); }
      } catch (error) {
        if (!cancelled) setRuntimeStatus({ state: "offline", mode: "local", error: error instanceof Error ? error.message : "Creative Runtime 不可用" });
      }
    }
    void load();
    const timer = window.setTimeout(() => void load(), 900);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [api, activeProjectId]);

  useEffect(() => {
    if (!api) return;
    let cancelled = false;
    const refreshStatus = async () => {
      try {
        const status = await api.runtime.status();
        if (!cancelled) setRuntimeStatus(status);
      } catch (error) {
        if (!cancelled) setRuntimeStatus({ state: "offline", mode: "local", error: error instanceof Error ? error.message : "Creative Runtime 不可用" });
      }
    };
    const timer = window.setInterval(() => void refreshStatus(), 2_000);
    void refreshStatus();
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [api]);

  useEffect(() => { if (!api) return; return api.agent.onEvent((event) => { setThreads((current) => applyThreadEvent(current, event, threadArchived)); setAgentRun((run) => reduceAgentEvent(run, event)); if (isResolvedApprovalEvent(event)) setApprovalQueue((current) => current.filter((request) => String(request.requestId) !== String(event.params.requestId))); const message = eventRecord(event); if (message?.method === "agent/error") setApprovalQueue([]); }); }, [api, threadArchived]);
  useEffect(() => { if (!api) return; return api.agent.onApprovalRequest((request) => { setApprovalQueue((current) => current.some((item) => String(item.requestId) === String(request.requestId)) || (request.threadId && agentRun.threadId && request.threadId !== agentRun.threadId) || current.length >= MAX_PENDING_INTERACTIONS ? current : [...current, request]); setActivity((items) => [isUserInputRequest(request) ? "Codex 正在等待你的回答。" : "Codex 正在等待权限确认。", ...items].slice(0, 4)); }); }, [api, agentRun.threadId]);
  useEffect(() => { if (approvalQueue.length === 0) return; setAgentRun((run) => run.state === "running" ? { ...run, state: "waiting" } : run); }, [approvalQueue.length]);
  useEffect(() => {
    if (!api || !agentStatus.configured) return;
    let cancelled = false;
    setThreadLoading(true);
    setThreadCursor(null);
    setThreadDetail(null);
    void api.agent.listThreads(projectId, threadArchived).then((result) => { if (!cancelled) { setThreads(result.threads); setThreadCursor(result.nextCursor); } }).catch(() => { if (!cancelled) setThreadAction("任务历史暂不可用"); }).finally(() => { if (!cancelled) setThreadLoading(false); });
    return () => { cancelled = true; };
  }, [api, agentStatus.configured, projectId, threadArchived]);

  const registerCreativeSaveHandler = useCallback((handler: (() => Promise<boolean>) | null) => { creativeSaveHandlerRef.current = handler; }, []);
  const registerCreativeDiscardHandler = useCallback((handler: (() => void) | null) => { creativeDiscardHandlerRef.current = handler; }, []);
  function applyNavigation(action: PendingNavigation) {
    if (action.kind === "close") { void api?.app.confirmClose(); return; }
    if (action.kind === "view") { setActiveView(action.view); return; }
    if (action.kind === "chapter") { setSelectedChapterId(action.chapterId); setActiveView("chapters"); return; }
    if (action.kind === "project") {
      resetProjectSession();
      setActiveProjectId(action.projectId);
      setActivity((items) => ["正在打开作品…", ...items].slice(0, 4));
      return;
    }
    if (action.kind === "new-project") { setProjectCreateOpen(true); return; }
    setActiveView("diffs");
    if (!api || !selectedChapter) return;
    void api.runtime.getDiffs(projectId, selectedChapter.id).then((result) => setDiffs(result.diffs)).catch((error) => setActivity((items) => [`读取变更失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4)));
  }
  function requestNavigation(action: PendingNavigation) {
    if (creativeDirty) { setPendingNavigation(action); return; }
    applyNavigation(action);
  }
  function navigateView(nextView: ViewId) {
    if (nextView === activeView) return;
    requestNavigation({ kind: "view", view: nextView });
  }
  function selectChapter(chapter: Chapter) {
    if (chapter.id === selectedChapterId && activeView === "chapters") return;
    requestNavigation({ kind: "chapter", chapterId: chapter.id });
  }
  function resetProjectSession() {
    setAgentRun(createAgentRun());
    setApprovalQueue([]);
    setThreads([]);
    setThreadDetail(null);
    setThreadAction(null);
    setDiffs([]);
    setTestResult(null);
    setSearchResults([]);
    setComposer("");
    setPlanningIntentSeed("");
    setActiveView("overview");
  }
  function selectProject(nextProjectId: string) {
    if (nextProjectId === activeProjectId || agentRun.state === "running" || agentRun.state === "waiting") return;
    requestNavigation({ kind: "project", projectId: nextProjectId });
  }
  async function createProject(title: string, logline: string) {
    if (!api) throw new Error("Novel Codex 桌面桥接未加载");
    const next = await api.runtime.createProject(title, logline);
    setProjects((items) => [next.project, ...items.filter((project) => project.id !== next.project.id)]);
    setState(next);
    setActiveProjectId(next.project.id);
    setSelectedChapterId(next.novelEdition.chapters[0]?.id ?? "");
    resetProjectSession();
    setActivity((items) => [`作品“${next.project.title}”已创建。`, ...items].slice(0, 4));
  }
  async function refreshFormalState() {
    if (!api) return;
    const next = await api.runtime.getState(projectId);
    setState(next);
    const chapter = next.novelEdition.chapters.find((item) => item.id === selectedChapterId) ?? next.novelEdition.chapters[0];
    setSelectedChapterId(chapter?.id ?? "");
    setCreativeRefreshToken((value) => value + 1);
  }
  async function loadDiffs() { requestNavigation({ kind: "diffs" }); }

  async function saveAndContinueNavigation() {
    const handler = creativeSaveHandlerRef.current;
    if (!pendingNavigation || !handler || savingBeforeLeave) return;
    setSavingBeforeLeave(true);
    try {
      if (!(await handler())) return;
      const action = pendingNavigation;
      setCreativeDirty(false);
      setPendingNavigation(null);
      applyNavigation(action);
    } finally { setSavingBeforeLeave(false); }
  }

  function discardAndContinueNavigation() {
    if (!pendingNavigation) return;
    const action = pendingNavigation;
    creativeDiscardHandlerRef.current?.();
    setCreativeDirty(false);
    setPendingNavigation(null);
    applyNavigation(action);
  }
  async function runTests() { try { if (!api) throw new Error("Novel Codex 桌面桥接未加载"); const result = await api.runtime.runStoryTests(projectId); setTestResult(result); setActivity((items) => [`Story Tests：${result.passed ? "全部通过" : "发现问题"}`, ...items].slice(0, 4)); } catch (error) { setActivity((items) => [`Story Tests 失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4)); } }
  async function search() { if (!searchQuery.trim()) return setSearchResults([]); try { if (!api) throw new Error("Novel Codex 桌面桥接未加载"); const results = (await api.runtime.search(projectId, searchQuery, 20)).results; setSearchResults(results); if (results[0]) setActivity((items) => [`检索到${searchKindLabel(results[0].source)}：${results[0].snippet}`, ...items].slice(0, 4)); } catch (error) { setActivity((items) => [`搜索失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4)); } }
  async function submitPrompt(requestedPrompt?: string) {
    const prompt = (requestedPrompt ?? composer).trim();
    if (!prompt) return;
    const inputError = authorIntentError(prompt);
    if (inputError) { setActivity((items) => [inputError, ...items].slice(0, 4)); return; }
    if (creativeDirty) {
      setActivity((items) => ["请先保存当前正文、事实表单或审阅决定，再启动或追加 Agent 任务。", ...items].slice(0, 4));
      return;
    }
    if (!agentAvailable) {
      setActivity((items) => ["当前没有可用模型，请先在右侧“设置”中完成配置。", ...items].slice(0, 4));
      return;
    }
    if (agentRun.state === "running") {
      if (!api || agentRun.mode !== "codex-app-server") return;
      try {
        const result = await api.agent.steer(prompt);
        if (!result.steered) throw new Error("当前 Codex 轮次尚未准备好接收追加指令");
        setComposer("");
        setAgentRun((run) => reduceAgentEvent(run, { method: "agent/steer", params: { prompt } }));
        setActivity((items) => [`已追加指令：${prompt}`, ...items].slice(0, 4));
      } catch (error) {
        setActivity((items) => [`追加指令失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4));
      }
      return;
    }
    if (agentRun.state === "waiting") return;
    const previous = summarizeAgentRun(agentRun);
    const history = previous ? [previous, ...agentRun.history].slice(0, 6) : agentRun.history;
    setComposer("");
    setApprovalQueue([]);
    setAgentRun(createAgentRun(prompt, history));
    setActivity((items) => [`你：${prompt}`, ...items].slice(0, 4));
    try {
      if (!api) throw new Error("桌面 Agent 未连接，当前只能手动编辑");
      const result = await api.agent.sendPrompt(projectId, prompt);
      setPlanningIntentSeed(`${prompt}\n\n请结合我们刚才的讨论结论：\n${result.message}`);
      setActivity((items) => [`Codex：${result.message}`, ...items].slice(0, 4));
      if (!creativeDirty) await refreshFormalState();
      else setActivity((items) => ["Agent 已完成；中央文档有未保存修改，因此未自动刷新。", ...items].slice(0, 4));
      void refreshThreads(false, false);
    } catch (error) {
      const message = error instanceof Error ? error.message : "未知错误";
      setAgentRun((run) => reduceAgentEvent(run, agentFailureEvent(message)));
      setActivity((items) => [`Agent 请求失败：${message}`, ...items].slice(0, 4));
    }
  }
  async function generateChapter(chapter: Chapter, intent: string, options: { povCharacterId: string | null; storyTime: number | null }) {
    if (agentRun.state === "running" || agentRun.state === "waiting") throw new Error("当前已有 Agent 任务正在执行");
    const prompt = `生成第 ${chapter.number} 章：${intent}`;
    const previous = summarizeAgentRun(agentRun);
    const history = previous ? [previous, ...agentRun.history].slice(0, 6) : agentRun.history;
    setApprovalQueue([]);
    setAgentRun(createAgentRun(prompt, history));
    setActivity((items) => [`开始生成第 ${chapter.number} 章：${chapter.title}`, ...items].slice(0, 4));
    try {
      if (!api) throw new Error("桌面 Agent 未连接，当前只能手动编辑");
      const result = await api.agent.generateChapter(projectId, {
        chapterId: chapter.id,
        chapterNumber: chapter.number,
        povCharacterId: options.povCharacterId,
        storyTime: options.storyTime,
        intent,
      });
      setActivity((items) => [`第 ${chapter.number} 章候选已保存，等待审阅。`, `Codex：${result.message}`, ...items].slice(0, 4));
      if (!creativeDirty) await refreshFormalState();
      void refreshThreads(false, false);
    } catch (error) {
      const message = error instanceof Error ? error.message : "未知错误";
      setAgentRun((run) => reduceAgentEvent(run, agentFailureEvent(message)));
      setActivity((items) => [`章节生成未完成：${message}`, ...items].slice(0, 4));
      if (!creativeDirty) await refreshFormalState();
      throw error;
    }
  }
  async function generatePlanning(operation: "generateBrief" | "generateOutline" | "generatePlan", prompt: string) {
    if (creativeDirty || agentRun.state === "running" || agentRun.state === "waiting") return;
    const previous = summarizeAgentRun(agentRun);
    const history = previous ? [previous, ...agentRun.history].slice(0, 6) : agentRun.history;
    setApprovalQueue([]);
    setAgentRun(createAgentRun(prompt, history));
    setActivity((items) => [`开始生成可审阅的规划候选`, ...items].slice(0, 4));
    try {
      if (!api) throw new Error("桌面 Agent 未连接，当前只能手动编辑");
      const result = await api.agent.generatePlanning(projectId, operation, prompt);
      setActivity((items) => [`规划候选已保存，等待审阅。`, `Codex：${result.message}`, ...items].slice(0, 4));
      if (!creativeDirty) await refreshFormalState();
      void refreshThreads(false, false);
    } catch (error) {
      const message = error instanceof Error ? error.message : "未知错误";
      setAgentRun((run) => reduceAgentEvent(run, agentFailureEvent(message)));
      setActivity((items) => [`规划生成未完成：${message}`, ...items].slice(0, 4));
      if (!creativeDirty) await refreshFormalState();
    }
  }
  async function refreshThreads(archived = threadArchived, clearAction = true) {
    if (!api || !agentStatus.configured) return;
    setThreadLoading(true);
    if (clearAction) setThreadAction(null);
    setThreadCursor(null);
    setThreadDetail(null);
    try { const result = await api.agent.listThreads(projectId, archived); setThreads(result.threads); setThreadCursor(result.nextCursor); } catch { setThreadAction("任务历史暂不可用"); } finally { setThreadLoading(false); }
  }
  async function loadMoreThreads() {
    if (!api || !agentStatus.configured || !threadCursor || threadLoading) return;
    setThreadLoading(true);
    try {
      const result = await api.agent.listThreads(projectId, threadArchived, threadCursor);
      setThreads((current) => { const known = new Set(current.map((thread) => thread.id)); return [...current, ...result.threads.filter((thread) => !known.has(thread.id))]; });
      setThreadCursor(result.nextCursor);
    } catch { setThreadAction("更多任务暂不可用"); } finally { setThreadLoading(false); }
  }
  async function resumeThread(thread: ThreadSummary) {
    if (!api || agentRun.state === "running" || agentRun.state === "waiting") return;
    setThreadAction("正在恢复任务…");
    try {
      if (threadArchived) {
        await api.agent.unarchiveThread(projectId, thread.id);
        setThreadArchived(false);
      }
      const result = await api.agent.resumeThread(projectId, thread.id);
      setAgentRun({ ...createAgentRun("", agentRun.history), threadId: result.thread.id });
      setApprovalQueue([]);
      setThreadAction(`已恢复：${result.thread.name ?? result.thread.preview ?? "未命名任务"}`);
      setActivity((items) => [`已恢复 Codex 任务：${result.thread.name ?? result.thread.preview ?? "未命名任务"}`, ...items].slice(0, 4));
      if (threadArchived) void refreshThreads(false, false);
    } catch (error) {
      setThreadAction(`恢复失败：${error instanceof Error ? error.message : "未知错误"}`);
    }
  }
  async function toggleThreadPin(thread: ThreadSummary) {
    if (!api || !agentStatus.configured) return;
    const nextPinned = !thread.isPinned;
    setThreadAction(nextPinned ? "正在置顶任务…" : "正在取消置顶…");
    try {
      await api.agent.setThreadPinned(projectId, thread.id, nextPinned);
      await refreshThreads(threadArchived, false);
      setThreadAction(nextPinned ? "任务已置顶" : "任务已取消置顶");
    } catch (error) {
      setThreadAction(`更新置顶状态失败：${error instanceof Error ? error.message : "未知错误"}`);
    }
  }
  async function readThread(thread: ThreadSummary) {
    if (!api || !agentStatus.configured) return;
    setThreadDetail(null);
    setThreadDetailLoading(true);
    setThreadAction("正在读取任务详情…");
    try {
      setThreadDetail(await api.agent.readThread(projectId, thread.id));
      setThreadAction(null);
    } catch (error) {
      setThreadAction(`读取详情失败：${error instanceof Error ? error.message : "未知错误"}`);
    } finally {
      setThreadDetailLoading(false);
    }
  }
  async function toggleThreadArchive(thread: ThreadSummary) {
    if (!api || !agentStatus.configured) return;
    if (agentRun.state === "running" || agentRun.state === "waiting") {
      setThreadAction("当前任务正在执行，完成或停止后才能归档。");
      return;
    }
    const restoring = threadArchived;
    setThreadAction(restoring ? "正在恢复归档任务…" : "正在归档任务…");
    try {
      if (restoring) {
        await api.agent.unarchiveThread(projectId, thread.id);
      } else {
        await api.agent.archiveThread(projectId, thread.id);
        if (thread.id === agentRun.threadId) setAgentRun((run) => ({ ...run, threadId: null, turnId: null }));
      }
      await refreshThreads(threadArchived, false);
      setThreadAction(restoring ? "任务已恢复到活动列表" : "任务已归档，可在已归档列表中恢复");
    } catch (error) {
      setThreadAction(`${restoring ? "恢复" : "归档"}失败：${error instanceof Error ? error.message : "未知错误"}`);
    }
  }
  function toggleThreadArchiveView() { setThreadAction(null); setThreadArchived((archived) => !archived); }
  async function interruptAgent() { if (!api || agentRun.state !== "running") return; try { const result = await api.agent.interrupt(); if (result.interrupted) { setAgentRun((run) => reduceAgentEvent(run, { method: "turn/completed", params: { turn: { id: run.turnId, status: "interrupted" } } })); setActivity((items) => ["已请求停止 Codex 当前执行。", ...items].slice(0, 4)); } } catch (error) { const message = error instanceof Error ? error.message : "未知错误"; setAgentRun((run) => reduceAgentEvent(run, { method: "agent/error", params: { message } })); setActivity((items) => [`停止 Agent 失败：${message}`, ...items].slice(0, 4)); } }
  async function reconnectAgent() {
    if (!api || !agentRun.threadId) return;
    setAgentRun((run) => reduceAgentEvent(run, { method: "agent/connection", params: { state: "reconnecting" } }));
    setActivity((items) => ["正在恢复 Codex 任务连接…", ...items].slice(0, 4));
    try {
      const result = await api.agent.reconnectThread(projectId, agentRun.threadId);
      setAgentRun((run) => reduceAgentEvent({ ...run, error: null }, { method: "thread/started", params: { thread: result.thread } }));
      setThreadAction("Codex 任务连接已恢复");
      setActivity((items) => ["Codex 任务连接已恢复，可以继续发送指令。", ...items].slice(0, 4));
      void refreshThreads(false, false);
    } catch (error) {
      const message = error instanceof Error ? error.message : "未知错误";
      setAgentRun((run) => reduceAgentEvent(run, { method: "agent/error", params: { message } }));
      setActivity((items) => [`恢复 Codex 连接失败：${message}`, ...items].slice(0, 4));
    }
  }
  async function reviewAgent() {
    if (!api || !agentRun.threadId || agentRun.review === "running") return;
    setAgentRun((run) => reduceAgentEvent(run, { method: "agent/review", params: { threadId: run.threadId } }));
    setContextTab("changes");
    try {
      await api.agent.reviewThread(projectId, agentRun.threadId);
      setActivity((items) => ["已启动当前工作区变更审查。", ...items].slice(0, 4));
    } catch (error) {
      const message = error instanceof Error ? error.message : "未知错误";
      setAgentRun((run) => reduceAgentEvent(run, { method: "agent/error", params: { message } }));
      setActivity((items) => [`变更审查失败：${message}`, ...items].slice(0, 4));
    }
  }
  async function compactContext() {
    if (!api || !agentRun.threadId) return;
    setAgentRun((run) => reduceAgentEvent(run, { method: "agent/compactionStarted", params: { threadId: run.threadId } }));
    try {
      await api.agent.compactThread(projectId, agentRun.threadId);
      setAgentRun((run) => reduceAgentEvent(run, { method: "agent/compactionCompleted", params: { threadId: run.threadId } }));
      setActivity((items) => ["上下文压缩完成，已保留当前故事约束。", ...items].slice(0, 4));
    } catch (error) {
      const message = error instanceof Error ? error.message : "未知错误";
      setAgentRun((run) => reduceAgentEvent(run, { method: "agent/error", params: { message } }));
      setActivity((items) => [`上下文压缩失败：${message}`, ...items].slice(0, 4));
    }
  }
  async function configureAgent(provider: string, model: string) {
    if (!api || agentConfigSaving) return;
    setAgentConfigSaving(true);
    try {
      const next = await api.agent.configure(provider, model);
      setAgentStatus(next);
      setAgentRun((run) => ({ ...run, provider: next.provider ?? provider, model: next.model ?? model, connectionState: next.configured ? "ready" : "offline", error: null }));
      setActivity((items) => [`Agent 配置已切换为 ${next.provider ?? provider} · ${next.model ?? model}。`, ...items].slice(0, 4));
    } catch (error) {
      setActivity((items) => [`Agent 配置失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4));
    } finally { setAgentConfigSaving(false); }
  }
  async function loadBackgroundTerminals() {
    if (!api || !agentRun.threadId) return;
    setBackgroundTerminalLoading(true);
    try { setBackgroundTerminals((await api.agent.listBackgroundTerminals(projectId, agentRun.threadId)).terminals); } catch (error) { setActivity((items) => [`后台终端读取失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4)); } finally { setBackgroundTerminalLoading(false); }
  }
  async function terminateBackgroundTerminal(processId: string) {
    if (!api || !agentRun.threadId) return;
    try { await api.agent.terminateBackgroundTerminal(projectId, agentRun.threadId, processId); setBackgroundTerminals((terminals) => terminals.filter((terminal) => terminal.processId !== processId)); setActivity((items) => [`后台终端 ${processId} 已终止。`, ...items].slice(0, 4)); } catch (error) { setActivity((items) => [`终止后台终端失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4)); }
  }
  async function resolveApproval(response: ApprovalResponse): Promise<boolean> { const current = approvalQueue[0]; if (!current || !api) return false; try { await api.agent.resolveApproval(current.requestId, response); setApprovalQueue((queue) => queue.filter((request) => String(request.requestId) !== String(current.requestId))); const label = "answers" in response ? "提交回答" : response.decision === "acceptForSession" ? "本次会话允许" : response.decision === "accept" ? "批准" : response.decision === "cancel" ? "取消" : "拒绝"; setActivity((items) => [`已${label} Codex 请求。`, ...items].slice(0, 4)); return true; } catch (error) { setActivity((items) => [`交互请求处理失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4)); return false; } }
  function openFactReview() { navigateView("chapters"); setActivity((items) => ["故事事实需随草稿进入联合审阅后确认。", ...items].slice(0, 4)); }

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "p") { event.preventDefault(); setCommandPaletteOpen((open) => !open); return; }
      if (event.key === "Escape") { setCommandPaletteOpen(false); setSearchOpen(false); }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const connectionState = agentRun.connectionState;
  const visibleAgentStatus = { ...agentStatus, state: connectionState === "reconnecting" ? "reconnecting" : agentRun.state === "running" ? "running" : agentRun.state === "waiting" ? "waiting" : agentRun.state === "error" || connectionState === "error" ? "error" : agentStatus.state };
  return <div className="app-shell"><header className="topbar"><div className="brand-lockup"><div className="brand-mark">✦</div><span>Novel Codex</span><span className="beta-pill">LOCAL</span></div><div className="topbar-center"><ProjectSwitcher projects={projects} activeProjectId={activeProjectId} busy={agentRun.state === "running" || agentRun.state === "waiting"} onSelect={selectProject} onCreate={() => requestNavigation({ kind: "new-project" })} /><span className="save-state"><span className={`save-dot ${creativeDirty ? "pending" : ""}`} />{creativeDirty ? "创作修改未保存" : "工作区已同步"}</span></div><div className="topbar-actions">{searchOpen && <input autoFocus className="top-search" value={searchQuery} onChange={(event) => setSearchQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void search(); }} placeholder="搜索角色、场景、章节" />}<button className="icon-button" title="搜索" onClick={() => setSearchOpen((open) => !open)}>⌕</button><button className="icon-button" title="命令面板 ⌘⇧P" onClick={() => setCommandPaletteOpen(true)}>⌘</button><button className="icon-button" title="运行 Story Tests" onClick={() => void runTests()}>✓</button><button className="icon-button theme-toggle" title={theme === "light" ? "切换深色主题" : "切换浅色主题"} aria-label={theme === "light" ? "切换深色主题" : "切换浅色主题"} onClick={() => setTheme(theme === "light" ? "dark" : "light")}>{theme === "light" ? "☾" : "☼"}</button><div className="avatar">N</div></div></header><div className="workspace"><Sidebar activeView={activeView} state={state} onNavigate={navigateView} onDiffs={() => void loadDiffs()} onCreateProject={() => requestNavigation({ kind: "new-project" })} runtimeStatus={runtimeStatus} threads={threads} currentThreadId={agentRun.threadId} onResume={(thread) => void resumeThread(thread)} busy={agentRun.state === "running" || agentRun.state === "waiting"} /><main className="main-panel"><div className="content-header"><div className="breadcrumbs"><span>小说项目</span><span>/</span><strong>{activeLabel}</strong></div><div className="header-actions"><button className="secondary-button" onClick={() => setExportOpen(true)}>导出</button><button className="primary-button" onClick={() => composerRef.current?.focus()}>✦ 与 Codex 协作</button></div></div><section className="hero-block"><div className="eyebrow">CREATIVE PROJECT <span>·</span> NOVEL EDITION</div><h1>{activeView === "overview" ? state.project.title : activeLabel}</h1><p className="logline">{state.project.logline}</p></section><AgentStatusBar status={visibleAgentStatus} connectionState={connectionState} threadId={agentRun.threadId} onReconnect={() => void reconnectAgent()} /><div className="sr-only" role="status" aria-live="polite">{connectionState === "reconnecting" ? "正在恢复 Codex 任务连接" : visibleAgentStatus.state === "running" ? "Agent 正在执行" : visibleAgentStatus.state === "waiting" ? "等待你的输入或授权" : ""}</div>{agentStatus.configured && <ThreadHistory threads={threads} loading={threadLoading} action={threadAction} currentThreadId={agentRun.threadId} archived={threadArchived} busy={agentRun.state === "running" || agentRun.state === "waiting"} nextCursor={threadCursor} detail={threadDetail} detailLoading={threadDetailLoading} onRefresh={() => void refreshThreads()} onLoadMore={() => void loadMoreThreads()} onToggleArchived={toggleThreadArchiveView} onResume={(thread) => void resumeThread(thread)} onRead={(thread) => void readThread(thread)} onCloseDetail={() => setThreadDetail(null)} onPin={(thread) => void toggleThreadPin(thread)} onArchive={(thread) => void toggleThreadArchive(thread)} />}{searchResults.length > 0 && <div className="search-results">{searchResults.map((result) => <button key={`${result.kind}-${result.id}`} onClick={() => { setSearchResults([]); setSearchOpen(false); setActivity((items) => [`找到${result.kind}：${result.title}`, ...items].slice(0, 4)); }}>{result.kind} · {result.title}</button>)}</div>}
{activeView === "overview" && <Overview state={state} onSelectChapter={selectChapter} onRunTests={() => void runTests()} onFocusComposer={() => composerRef.current?.focus()} onOpenBrief={() => navigateView("brief")} onOpenChapters={() => navigateView("chapters")} testResult={testResult} />}
{(["brief", "outline", "plans", "chapters"] as ViewId[]).includes(activeView) && <CreativeWorkspaceView api={api} projectId={projectId} view={activeView as "brief" | "outline" | "plans" | "chapters"} state={state} selectedChapterId={selectedChapterId} onSelectChapter={selectChapter} onGeneratePlanning={(operation, prompt) => void generatePlanning(operation, prompt)} onGenerateChapter={generateChapter} onOpenPlanningTarget={(kind) => navigateView(kind === "plan" ? "plans" : "outline")} onReturnToFeedback={() => navigateView("chapters")} onFormalChange={refreshFormalState} onDirtyChange={setCreativeDirty} onSaveHandlerChange={registerCreativeSaveHandler} onDiscardHandlerChange={registerCreativeDiscardHandler} agentAvailable={agentAvailable} agentRunning={agentRun.state === "running" || agentRun.state === "waiting"} refreshToken={creativeRefreshToken} planningIntentSeed={planningIntentSeed} />}
{activeView === "diffs" && <DiffView diffs={diffs} />}
{activeView !== "overview" && activeView !== "brief" && activeView !== "outline" && activeView !== "plans" && activeView !== "chapters" && activeView !== "diffs" && <ViewPanel view={activeView} state={state} onAddCanonNote={openFactReview} />}
</main><aside className="conversation-rail" aria-label="Codex 协作栏"><div className="conversation-rail-scroll"><EnhancedAgentTranscript run={agentRun} approvalRequest={approvalQueue[0] ?? null} queuedApprovalCount={Math.max(0, approvalQueue.length - 1)} onResolveApproval={(response) => void resolveApproval(response)} onClear={() => setAgentRun(createAgentRun())} onRetry={() => void submitPrompt(agentRun.prompt)} onInterrupt={() => void interruptAgent()} onReview={() => void reviewAgent()} onCompact={() => void compactContext()} /></div><Composer composer={composer} setComposer={setComposer} onSubmit={() => void submitPrompt()} onInterrupt={() => void interruptAgent()} composerRef={composerRef} activity={activity} agentStatus={visibleAgentStatus} agentAvailable={agentAvailable} blockedByUnsavedChanges={creativeDirty} /><div className="conversation-context"><AgentInspector state={state} runtimeStatus={runtimeStatus} agentRun={agentRun} contextTab={contextTab} onContextTabChange={setContextTab} onAddCanonNote={openFactReview} onReview={() => void reviewAgent()} onCompact={() => void compactContext()} backgroundTerminals={backgroundTerminals} backgroundTerminalLoading={backgroundTerminalLoading} onLoadBackgroundTerminals={() => void loadBackgroundTerminals()} onTerminateBackgroundTerminal={(processId) => void terminateBackgroundTerminal(processId)} onConfigureAgent={(provider, model) => void configureAgent(provider, model)} agentConfigSaving={agentConfigSaving} /></div></aside></div>{pendingNavigation && <UnsavedChangesDialog saving={savingBeforeLeave} onSave={() => void saveAndContinueNavigation()} onDiscard={discardAndContinueNavigation} onStay={() => { if (pendingNavigation.kind === "close") void api?.app.cancelClose(); setPendingNavigation(null); }} />}{commandPaletteOpen && <CommandPalette onClose={() => setCommandPaletteOpen(false)} onFocusComposer={() => { setCommandPaletteOpen(false); composerRef.current?.focus(); }} onReview={() => { setCommandPaletteOpen(false); void reviewAgent(); }} onCompact={() => { setCommandPaletteOpen(false); void compactContext(); }} onReconnect={() => { setCommandPaletteOpen(false); void reconnectAgent(); }} onTheme={() => { setCommandPaletteOpen(false); setTheme((current) => current === "light" ? "dark" : "light"); }} />}{exportOpen && <ExportDialog onClose={() => setExportOpen(false)} onExport={async (options) => { if (!api) throw new Error("Runtime 尚未连接，请稍后重试。"); return api.runtime.exportProject(activeProjectId, options); }} />}{projectCreateOpen && <ProjectCreateDialog onClose={() => setProjectCreateOpen(false)} onSubmit={createProject} />}</div>;
}

function Sidebar({ activeView, state, onNavigate, onDiffs, onCreateProject, runtimeStatus, threads, currentThreadId, onResume, busy }: { activeView: ViewId; state: RuntimeState; onNavigate: (view: ViewId) => void; onDiffs: () => void; onCreateProject: () => void; runtimeStatus: RuntimeStatus; threads: ThreadSummary[]; currentThreadId: string | null; onResume: (thread: ThreadSummary) => void; busy: boolean }) {
  return <aside className="sidebar"><div className="sidebar-heading"><span>PROJECT</span><button className="tiny-button" title="新建作品" onClick={onCreateProject} disabled={busy}>＋</button></div><div className="project-card"><div className="book-icon">▧</div><div><strong>{state.project.title}</strong><span>Novel Edition · v0.1</span></div><span className="more">•••</span></div><div className="sidebar-task-section"><div className="nav-label">TASKS <small>{threads.length}</small></div>{threads.slice(0, 5).map((thread) => { const label = thread.name ?? thread.preview ?? "未命名任务"; const current = thread.id === currentThreadId; return <button className={`sidebar-task ${current ? "current" : ""}`} key={thread.id} onClick={() => onResume(thread)} disabled={current || busy} title={label}><span className="sidebar-task-marker">{current ? "✓" : "·"}</span><span>{label}</span></button>; })}{threads.length === 0 && <span className="sidebar-task-empty">发送指令后，任务会显示在这里</span>}</div><nav className="nav-groups">{navGroups.map((group) => <div className="nav-group" key={group.label}><div className="nav-label">{group.label}</div>{group.items.map((item) => <button className={`nav-item ${activeView === item.id ? "active" : ""}`} key={item.id} onClick={() => onNavigate(item.id)}><span className="nav-icon">{item.icon}</span><span>{item.label}</span>{item.id === "chapters" && <span className="nav-count">{state.novelEdition.chapters.length}</span>}{item.id === "scenes" && <span className="nav-count">{state.narrativeIr.scenes.length}</span>}</button>)}</div>)}</nav><div className="sidebar-footer"><button className={`nav-item ${activeView === "diffs" ? "active" : ""}`} onClick={onDiffs}><span className="nav-icon">⌁</span><span>版本与变更</span></button><button className="nav-item" onClick={() => onNavigate("overview")}><span className="nav-icon">?</span><span>帮助</span></button><div className="runtime-card"><span className={`online-dot ${runtimeStatus.state === "online" ? "" : "offline"}`} /> Creative Runtime <span>{runtimeStatus.state === "online" ? "本地在线" : "不可用"}</span></div></div></aside>;
}

function ProjectSwitcher({ projects, activeProjectId, busy, onSelect, onCreate }: { projects: ProjectSummary[]; activeProjectId: string; busy: boolean; onSelect: (projectId: string) => void; onCreate: () => void }) {
  const [open, setOpen] = useState(false);
  const active = projects.find((project) => project.id === activeProjectId);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") setOpen(false); };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
  useEffect(() => { if (busy) setOpen(false); }, [busy]);
  return <div className="project-switcher-wrap"><button className="project-switcher" aria-expanded={open} disabled={busy} onClick={() => setOpen((value) => !value)}><span className="project-dot" /><span>{active?.title ?? "选择作品"}</span><span className="muted">⌄</span></button>{open && <div className="project-menu" role="menu" aria-label="选择作品"><div className="project-menu-heading"><span>我的作品</span><button className="project-create-menu" onClick={() => { setOpen(false); onCreate(); }}>＋ 新建</button></div>{projects.map((project) => <button className={`project-menu-item ${project.id === activeProjectId ? "selected" : ""}`} role="menuitem" key={project.id} onClick={() => { setOpen(false); onSelect(project.id); }}><span className="project-menu-mark">{project.id === activeProjectId ? "✓" : "▧"}</span><span><strong>{project.title}</strong><small>{project.logline || "暂无简介"}</small></span></button>)}</div>}</div>;
}

function ProjectCreateDialog({ onClose, onSubmit }: { onClose: () => void; onSubmit: (title: string, logline: string) => Promise<void> }) {
  const [title, setTitle] = useState("");
  const [logline, setLogline] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  async function submit() {
    const normalizedTitle = title.trim();
    if (!normalizedTitle) { setError("请填写作品名称。"); return; }
    if (normalizedTitle.length > 120) { setError("作品名称不能超过 120 个字符。"); return; }
    if (logline.trim().length > 500) { setError("作品简介不能超过 500 个字符。"); return; }
    setSaving(true);
    setError(null);
    try {
      await onSubmit(normalizedTitle, logline);
      onClose();
    } catch (reason) {
      const message = reason instanceof Error
        ? reason.message.replace(/^Error invoking remote method '[^']+':\s*(?:Error:\s*)?/, "")
        : "作品创建失败，请稍后重试。";
      setError(message || "作品创建失败，请稍后重试。");
    } finally { setSaving(false); }
  }
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape" && !saving) onClose(); };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose, saving]);
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !saving) onClose(); }}><form className="project-dialog" role="dialog" aria-modal="true" aria-labelledby="project-dialog-title" aria-describedby="project-dialog-description" onSubmit={(event) => { event.preventDefault(); void submit(); }}><div className="project-dialog-header"><div><span className="eyebrow">NEW CREATIVE PROJECT</span><h2 id="project-dialog-title">新建作品</h2></div><button className="dialog-close" type="button" aria-label="关闭" onClick={onClose} disabled={saving}>×</button></div><p id="project-dialog-description" className="project-dialog-description">创建一个独立的小说工作区，之后可以在右侧与 Codex 协作。</p><label className="dialog-field"><span>作品名称 <em>必填</em></span><input autoFocus value={title} maxLength={120} placeholder="例如：雾中的回声" onChange={(event) => { setTitle(event.target.value); setError(null); }} /></label><label className="dialog-field"><span>一句话简介 <small>可选</small></span><textarea value={logline} maxLength={500} placeholder="描述这部作品的核心冲突或气质" onChange={(event) => { setLogline(event.target.value); setError(null); }} onKeyDown={(event) => { if ((event.metaKey || event.ctrlKey) && event.key === "Enter") { event.preventDefault(); void submit(); } }} /></label>{error && <div className="dialog-error" role="alert">{error}</div>}<div className="project-dialog-actions"><button className="secondary-button" type="button" onClick={onClose} disabled={saving}>取消</button><button className="primary-button" type="submit" disabled={saving}>{saving ? "创建中…" : "创建作品"}</button></div></form></div>;
}

function UnsavedChangesDialog({ saving, onSave, onDiscard, onStay }: { saving: boolean; onSave: () => void; onDiscard: () => void; onStay: () => void }) {
  const stayRef = useRef<HTMLButtonElement | null>(null);
  useEffect(() => { stayRef.current?.focus(); }, []);
  return <div className="modal-backdrop" role="presentation"><section className="project-dialog unsaved-dialog" role="dialog" aria-modal="true" aria-labelledby="unsaved-dialog-title" aria-describedby="unsaved-dialog-description"><div className="project-dialog-header"><div><span className="eyebrow">UNSAVED CREATIVE WORK</span><h2 id="unsaved-dialog-title">当前创作修改还没有确认保存</h2></div></div><p id="unsaved-dialog-description" className="project-dialog-description">保存会保留正文候选、事实表单和审阅决定；也可以明确放弃，或留在当前页面继续处理。</p><div className="project-dialog-actions three-way-actions"><button className="text-button danger" type="button" disabled={saving} onClick={onDiscard}>放弃修改并离开</button><button ref={stayRef} className="secondary-button" type="button" disabled={saving} onClick={onStay}>留在当前页面</button><button className="primary-button" type="button" disabled={saving} onClick={onSave}>{saving ? "正在保存…" : "保存并继续"}</button></div></section></div>;
}

function Overview({ state, onSelectChapter, onRunTests, onFocusComposer, onOpenBrief, onOpenChapters, testResult }: { state: RuntimeState; onSelectChapter: (chapter: Chapter) => void; onRunTests: () => void; onFocusComposer: () => void; onOpenBrief: () => void; onOpenChapters: () => void; testResult: StoryTestResult | null }) {
  const chapter = state.novelEdition.chapters[0];
  if (!chapter) return <div className="overview-content"><div className="empty-state project-empty-state"><span>✦</span><strong>从构想、规划或第一章开始</strong><p>可以先与 Codex 讨论题材和冲突，整理正式创作简报；有明确开场时，也可以直接创建独立章节开始写作。</p><div><button className="primary-button" onClick={onFocusComposer}>讨论小说构想</button><button className="secondary-button" onClick={onOpenBrief}>整理创作简报</button><button className="secondary-button" onClick={onOpenChapters}>直接写第一章</button></div></div></div>;
  const passage = chapter.passages[0];
  const scene = state.narrativeIr.scenes.find((item) => item.id === passage?.scene_id);
  return <div className="overview-content"><div className="metric-grid"><Metric label="章节" value={String(state.novelEdition.chapters.length)} detail="真实 SQLite 数据" icon="▤" /><Metric label="场景" value={String(state.narrativeIr.scenes.length)} detail="含 Beat 结构" icon="◈" /><Metric label="角色" value={String(state.canon.characters.length).padStart(2, "0")} detail="来自 Story Canon" icon="♙" /><Metric label="故事线" value={String(state.canon.plotThreads.length).padStart(2, "0")} detail="持续追踪中" icon="⌁" /></div><div className="editor-card"><div className="editor-toolbar"><div><span className="editor-file-icon">▤</span><strong>CHAPTER {String(chapter.number).padStart(2, "0")}</strong><span className="muted">·</span><span>{chapter.title}</span></div><div className="toolbar-actions"><button onClick={() => onSelectChapter(chapter)}>编辑</button><button onClick={onRunTests}>Story Tests</button></div></div><div className="chapter-editor"><div className="chapter-meta"><span>第 {chapter.number} 章</span><span>{passage?.body.length ?? 0} 字</span><span>最近编辑：{formatDate(chapter.updated_at)}</span></div><h2>{chapter.title}</h2>{passage?.body.split("\n\n").map((paragraph) => <p key={paragraph}>{paragraph}</p>)}{scene && <button className="scene-anchor" onClick={() => onSelectChapter(chapter)}><span className="anchor-dot" /><span><strong>Scene {scene.id.replace("scene-", "")} · {scene.title}</strong><small>{scene.beats.length} beats · {scene.participants.map((participant) => participant.name).join(" / ")} · {scene.status}</small></span><b>编辑章节</b></button>}</div></div>{testResult && <TestPanel result={testResult} />}</div>;
}

function ViewPanel({ view, state, onAddCanonNote }: { view: Exclude<ViewId, "overview" | "chapters" | "diffs">; state: RuntimeState; onAddCanonNote: () => void }) {
  const copy: Record<typeof view, { title: string; description: string }> = { brief: { title: "创作简报", description: "固定作品承诺与边界。" }, outline: { title: "长篇大纲", description: "管理故事阶段与转折。" }, plans: { title: "近章规划", description: "滚动规划接下来的章节。" }, characters: { title: "角色", description: "跨媒介共享的 Canon 实体。" }, scenes: { title: "场景", description: "Scene / Beat 是小说走向漫画和动漫的叙事中间层。" }, plot: { title: "情节线", description: "追踪冲突、伏笔和未解决的问题。" }, timeline: { title: "时间线", description: "用事件与状态变化组织故事时间。" }, world: { title: "世界与规则", description: "供 Agent 写作前查询、写作后校验的 Canon 约束。" } };
  const content = copy[view]; return <div className="view-panel"><div className="view-intro"><div className="view-symbol">{view === "characters" ? "♙" : view === "scenes" ? "◈" : "⌁"}</div><div><h2>{content.title}</h2><p>{content.description}</p></div>{view === "world" && <button className="primary-button" onClick={onAddCanonNote}>＋ 新建</button>}</div>{view === "characters" && <div className="character-grid">{state.canon.characters.map((character) => <div className="big-character-card" key={character.id}><div className="character-avatar large" style={{ background: character.color }}>{character.name.slice(0, 1)}</div><h3>{character.name}</h3><p>{character.role}</p><span>{character.status}</span></div>)}</div>}{view === "scenes" && <div className="list-card">{state.narrativeIr.scenes.map((scene) => <SceneRow scene={scene} key={scene.id} />)}</div>}{view === "plot" && <div className="list-card">{state.canon.plotThreads.map((thread) => <div className="list-row" key={thread.id}><span className={`thread-line ${thread.tone}`} /><span><strong>{thread.title}</strong><small className="list-sub">{thread.state}</small></span></div>)}</div>}{view === "timeline" && <div className="list-card">{state.narrativeIr.scenes.map((scene) => <div className="list-row" key={scene.id}><span className="list-index">{scene.time}</span><span><strong>{scene.title}</strong><small className="list-sub">{scene.location} · {scene.summary}</small></span></div>)}</div>}{view === "world" && <div className="list-card">{state.canon.worldRules.map((rule) => <div className="list-row" key={rule.id}><span className="status-dot" /><span><strong>{rule.statement}</strong><small className="list-sub">{rule.severity}</small></span></div>)}</div>}</div>; }
function SceneRow({ scene }: { scene: Scene }) { return <div className="scene-row"><div className="list-row"><span className="status-dot" data-status={scene.status} /><span><strong>{scene.title}</strong><small className="list-sub">{scene.location} · {scene.time} · {scene.beats.length} beats</small></span><span className="list-sub">{scene.status}</span></div><div className="beat-list">{scene.beats.map((beat) => <span key={beat.id}>{beat.position}. {beat.description}</span>)}</div></div>; }
function DiffView({ diffs }: { diffs: ChapterDiff[] }) { return <div className="view-panel"><div className="view-intro"><div className="view-symbol">⌁</div><div><h2>版本与变更</h2><p>每次章节正文保存前后的内容都保存在 Chapter Diff 中。</p></div></div>{diffs.length === 0 ? <div className="empty-state"><span>✓</span><strong>还没有章节变更</strong><p>编辑并保存章节后，变更记录会出现在这里。</p></div> : <div className="diff-list">{diffs.map((diff) => <details className="diff-card" key={diff.id}><summary>{diff.summary} · {formatDate(diff.createdAt)}</summary><div><del>{diff.fromText}</del><ins>{diff.toText}</ins></div></details>)}</div>}</div>; }
function ContextPanel({ state, runtimeStatus, onAddCanonNote }: { state: RuntimeState; runtimeStatus: RuntimeStatus; onAddCanonNote: () => void }) { return <aside className="context-panel"><div className="panel-tabs"><button className="selected">Story Context</button><button>Activity</button></div><div className="context-section"><div className="section-title"><span>CANON NOTES</span><button onClick={onAddCanonNote}>＋</button></div><div className="canon-list">{state.canon.notes.map((note) => <div className="canon-note" key={note}><span className="note-pin">•</span>{note}</div>)}</div></div><div className="context-section"><div className="section-title"><span>ACTIVE CHARACTERS</span><span className="section-count">{state.canon.characters.length}</span></div><div className="character-list">{state.canon.characters.map((character) => <div className="character-row" key={character.id}><div className="character-avatar" style={{ background: character.color }}>{character.name.slice(0, 1)}</div><div><strong>{character.name}</strong><span>{character.role}</span></div><span className="character-status">{character.status}</span></div>)}</div></div><div className="context-section"><div className="section-title"><span>OPEN THREADS</span><span className="section-count">{state.canon.plotThreads.length}</span></div>{state.canon.plotThreads.map((thread) => <div className="thread-row" key={thread.id}><span className={`thread-line ${thread.tone}`} /><span>{thread.title}</span><small>{thread.state}</small></div>)}</div><div className="context-footer"><span>{runtimeStatus.state === "online" ? "Context synchronized" : "Local fallback"}</span><span className={`online-dot ${runtimeStatus.state === "online" ? "" : "offline"}`} /></div></aside>; }
function phaseStatusLabel(status: AgentPhaseStatus): string { return status === "running" ? "进行中" : status === "waiting" ? "等待确认" : status === "failed" ? "失败" : status === "completed" ? "完成" : status === "skipped" ? "未使用" : "待处理"; }
function phaseIcon(status: AgentPhaseStatus): string { return status === "completed" ? "✓" : status === "failed" ? "×" : status === "running" ? "•" : status === "waiting" ? "!" : status === "skipped" ? "–" : "·"; }
function agentStateLabel(state: AgentRun["state"]): string { return state === "running" ? "执行中" : state === "waiting" ? "等待确认" : state === "error" ? "执行失败" : state === "cancelled" ? "已停止" : state === "completed" ? "已完成" : "暂无执行"; }
function AgentPhaseRail({ run }: { run: AgentRun }) { return <div className="agent-phase-rail" aria-label="Agent 执行阶段">{run.phases.map((phase) => <div className={`agent-phase ${phase.status}`} key={phase.id}><span className="phase-marker" aria-hidden="true">{phaseIcon(phase.status)}</span><div><strong>{phase.label}</strong><small>{phaseStatusLabel(phase.status)}</small></div></div>)}</div>; }
function planStatusLabel(status: AgentRun["plan"][number]["status"]): string { return status === "inProgress" ? "进行中" : status === "completed" ? "已完成" : "待处理"; }
function AgentPlan({ run }: { run: AgentRun }) { if (run.plan.length === 0) return null; const completed = run.plan.filter((step) => step.status === "completed").length; return <section className="agent-plan" aria-label="Agent 执行计划"><div className="agent-plan-heading"><strong>执行计划</strong><span>{completed}/{run.plan.length} 已完成</span></div><ol>{run.plan.map((step) => <li className={step.status} key={step.id}><span className="plan-marker" aria-hidden="true">{step.status === "completed" ? "✓" : step.status === "inProgress" ? "•" : "·"}</span><span>{step.label}</span><small>{planStatusLabel(step.status)}</small></li>)}</ol></section>; }
function AgentDiff({ run }: { run: AgentRun }) { if (!run.diff) return null; return <details className="agent-diff"><summary>统一变更摘要 · 1</summary><pre>{run.diff}</pre></details>; }
function ContextPhaseList({ run }: { run: AgentRun }) { const currentPhase = run.phases.find((phase) => phase.status === "running" || phase.status === "waiting"); const completedSteps = run.steps.filter((step) => step.status === "completed").length; const completedPlan = run.plan.filter((step) => step.status === "completed").length; return <><div className="context-phase-list" aria-label="Agent 阶段摘要">{run.phases.map((phase) => <div className={`context-phase ${phase.status}`} key={phase.id}><span aria-hidden="true">{phaseIcon(phase.status)}</span><strong>{phase.label}</strong><small>{phaseStatusLabel(phase.status)}</small></div>)}</div><div className="context-metrics"><span>{currentPhase ? `当前：${currentPhase.label}` : `已完成：${completedSteps} 步`}</span><span>{run.plan.length > 0 ? `计划：${completedPlan}/${run.plan.length}` : "暂无计划"}</span><span>{run.diff ? "统一 diff：1" : run.artifacts.length > 0 ? `产物：${run.artifacts.length}` : "暂无变更"}</span></div></>; }
function OptimizedContextPanel({ state, runtimeStatus, agentRun, contextTab, onContextTabChange, onAddCanonNote }: { state: RuntimeState; runtimeStatus: RuntimeStatus; agentRun: AgentRun; contextTab: "story" | "activity"; onContextTabChange: (tab: "story" | "activity") => void; onAddCanonNote: () => void }) { const runState = agentRun.state === "running" ? "执行中" : agentRun.state === "waiting" ? "等待确认" : agentRun.state === "error" ? "执行失败" : agentRun.state === "cancelled" ? "已停止" : agentRun.state === "completed" ? "已完成" : "暂无执行"; return <aside className="context-panel"><div className="panel-tabs" role="tablist" aria-label="项目侧栏"><button className={contextTab === "story" ? "selected" : ""} role="tab" aria-selected={contextTab === "story"} onClick={() => onContextTabChange("story")}>Story Context</button><button className={contextTab === "activity" ? "selected" : ""} role="tab" aria-selected={contextTab === "activity"} onClick={() => onContextTabChange("activity")}>Activity{agentRun.state === "running" && <span className="activity-badge">●</span>}</button></div>{contextTab === "story" ? <><div className="context-section"><div className="section-title"><span>CANON NOTES</span><button onClick={onAddCanonNote}>＋</button></div><div className="canon-list">{state.canon.notes.map((note) => <div className="canon-note" key={note}><span className="note-pin">•</span>{note}</div>)}</div></div><div className="context-section"><div className="section-title"><span>ACTIVE CHARACTERS</span><span className="section-count">{state.canon.characters.length}</span></div><div className="character-list">{state.canon.characters.map((character) => <div className="character-row" key={character.id}><div className="character-avatar" style={{ background: character.color }}>{character.name.slice(0, 1)}</div><div><strong>{character.name}</strong><span>{character.role}</span></div><span className="character-status">{character.status}</span></div>)}</div></div><div className="context-section"><div className="section-title"><span>OPEN THREADS</span><span className="section-count">{state.canon.plotThreads.length}</span></div>{state.canon.plotThreads.map((thread) => <div className="thread-row" key={thread.id}><span className={`thread-line ${thread.tone}`} /><span>{thread.title}</span><small>{thread.state}</small></div>)}</div></> : <div className="context-activity"><div className="activity-summary"><span className={`online-dot ${agentRun.state === "running" ? "activity-live" : ""}`} /><div><strong>Agent {runState}</strong><small>{agentRun.prompt || "还没有执行记录"}</small></div></div>{agentRun.steps.length === 0 ? <div className="context-empty">发送一条创作指令后，执行过程会显示在这里。</div> : <><ContextPhaseList run={agentRun} /><div className="context-steps">{agentRun.steps.slice(-8).map((step) => <div className={`context-step ${step.status}`} key={step.id}><span>{step.kind === "tool" ? "›" : step.kind === "assistant" ? "✦" : step.kind === "error" ? "×" : "·"}</span><div><strong>{step.title}</strong><small>{step.status === "running" ? "进行中" : step.status === "waiting" ? "等待中" : step.status === "failed" ? "失败" : "完成"}</small></div></div>)}</div></>}<button className="context-focus" onClick={() => document.getElementById("agent-transcript")?.scrollIntoView({ behavior: "smooth", block: "nearest" })}>查看完整执行轨迹 →</button></div>}<div className="context-footer"><span>{runtimeStatus.state === "online" ? "Context synchronized" : "Local fallback"}</span><span className={`online-dot ${runtimeStatus.state === "online" ? "" : "offline"}`} /></div></aside>; }
function AgentTranscript({ run, onClear, onRetry, onInterrupt }: { run: AgentRun; onClear: () => void; onRetry: () => void; onInterrupt: () => void }) { if (run.state === "idle") return null; const stateLabel = run.state === "running" ? "执行中" : run.state === "waiting" ? "等待确认" : run.state === "error" ? "执行失败" : run.state === "cancelled" ? "已停止" : "已完成"; return <section className="agent-transcript" id="agent-transcript" aria-live="polite"><div className="transcript-header"><div><span className="section-title">AGENT ACTIVITY</span><span className={`run-mode ${run.mode}`}>Codex App Server</span><span className={`run-state ${run.state}`}>{stateLabel}</span></div><div className="transcript-actions">{run.state === "running" && <button className="transcript-stop" onClick={onInterrupt}>停止</button>}{(run.state === "error" || run.state === "completed" || run.state === "cancelled") && <button className="transcript-retry" onClick={onRetry}>重试</button>}<button className="transcript-clear" onClick={onClear}>清除</button></div></div><div className="transcript-prompt"><span className="transcript-avatar user">你</span><div><small>你的创作指令</small><p>{run.prompt}</p></div></div><AgentPhaseRail run={run} /><AgentPlan run={run} /><div className="transcript-steps" role="log" aria-label="Codex Agent 执行过程">{run.steps.map((step) => <div className={`transcript-step ${step.kind} ${step.status}`} key={step.id}><span className="step-marker" aria-hidden="true">{step.kind === "assistant" ? "✦" : step.kind === "tool" ? "›" : step.kind === "approval" ? "!" : step.kind === "error" ? "×" : "·"}</span><div className="step-content"><div className="step-heading"><strong>{step.title}</strong><span>{step.status === "running" ? "进行中" : step.status === "waiting" ? "等待中" : step.status === "failed" ? "失败" : "完成"}</span></div>{step.text && <p className="step-text">{step.text}{step.status === "running" && <span className="stream-cursor" aria-label="正在接收内容">▋</span>}</p>}{step.detail && <details className="step-detail-disclosure"><summary>查看执行详情</summary><pre className="step-detail">{step.detail}</pre></details>}</div></div>)}</div>{run.diff && <AgentDiff run={run} />}{run.artifacts.length > 0 && <details className="agent-artifacts"><summary>产物变更 · {run.artifacts.length}</summary>{run.artifacts.map((artifact) => <div className="artifact-row" key={artifact.id}><span className="artifact-icon" aria-hidden="true">{artifact.kind === "chapter-change" ? "▤" : "◇"}</span><div><strong>{artifact.title}</strong>{artifact.detail && <pre>{artifact.detail}</pre>}</div><small>{artifact.status === "applied" ? "已应用" : "待确认"}</small></div>)}</details>}{run.summary && <div className="agent-summary"><strong>完成总结</strong><p>{run.summary}</p></div>}{run.history.length > 0 && <details className="run-history"><summary>本次会话的其他执行 · {run.history.length}</summary>{run.history.map((entry) => <div className={`history-row ${entry.state}`} key={entry.id}><div className="history-row-icon" aria-hidden="true">{entry.state === "completed" ? "✓" : entry.state === "cancelled" ? "–" : "×"}</div><div className="history-row-content"><div className="history-row-heading"><strong>{entry.prompt}</strong><span className={`history-status ${entry.state}`}>{agentStateLabel(entry.state)}</span></div><small>Codex App Server · {formatDate(new Date(entry.createdAt).toISOString())}{entry.artifactCount > 0 ? ` · 产物 ${entry.artifactCount}` : ""}</small><p>{entry.summary}</p></div></div>)}</details>}{run.error && <div className="transcript-error">{run.error}</div>}</section>; }
function threadStatusLabel(status: string | null): string { return status === "waiting" ? "等待授权" : status === "active" ? "执行中" : status === "systemError" ? "异常" : status === "notLoaded" ? "可继续" : "空闲"; }
function formatThreadDate(timestamp: number | null): string { return timestamp ? formatDate(new Date(timestamp * 1000).toISOString()) : "—"; }
function ThreadHistory({ threads, loading, action, currentThreadId, archived, busy, nextCursor, detail, detailLoading, onRefresh, onLoadMore, onToggleArchived, onResume, onRead, onCloseDetail, onPin, onArchive }: { threads: ThreadSummary[]; loading: boolean; action: string | null; currentThreadId: string | null; archived: boolean; busy: boolean; nextCursor: string | null; detail: ThreadDetail | null; detailLoading: boolean; onRefresh: () => void; onLoadMore: () => void; onToggleArchived: () => void; onResume: (thread: ThreadSummary) => void; onRead: (thread: ThreadSummary) => void; onCloseDetail: () => void; onPin: (thread: ThreadSummary) => void; onArchive: (thread: ThreadSummary) => void }) {
  return <section className="thread-history-panel" aria-label="Codex 任务历史"><div className="thread-history-toolbar"><span className="thread-history-title">CODEX TASKS <small>{loading ? "加载中…" : `${threads.length} 个${archived ? "已归档任务" : "活动任务"}`}</small></span><div className="thread-history-toolbar-actions"><button className="thread-history-view-toggle" onClick={onToggleArchived}>{archived ? "活动" : "已归档"}</button><button className="thread-history-refresh" aria-label="刷新任务历史" onClick={onRefresh} disabled={loading}>↻</button></div></div><details className="thread-history-disclosure"><summary>{archived ? "查看已归档任务" : "查看任务历史"}</summary><div className="thread-history-list">{loading && <div className="thread-history-empty">正在读取隔离 App Server 任务…</div>}{!loading && threads.length === 0 && <div className="thread-history-empty">{archived ? "暂无已归档任务。归档后的创作任务会出现在这里。" : "暂无活动任务。发送第一条指令后，这里会出现可恢复的任务。"}</div>}{threads.map((thread) => { const label = thread.name ?? thread.preview ?? "未命名任务"; const current = thread.id === currentThreadId; return <div className={`thread-history-row ${current ? "current" : ""}`} key={thread.id}><button className="thread-history-open" onClick={() => onResume(thread)} disabled={current || busy} aria-label={`${archived ? "恢复并打开" : "打开并恢复"}任务：${label}`}><span className="thread-history-marker" aria-hidden="true">{current ? "✓" : "·"}</span><span className="thread-history-copy"><strong>{label}</strong><small>{thread.preview && thread.name ? thread.preview : "继续这个故事"} · {formatThreadDate(thread.updatedAt ?? thread.createdAt)}</small></span><span className="thread-history-status">{current ? "当前" : threadStatusLabel(thread.status)}</span></button><div className="thread-history-row-actions"><button className="thread-history-action-button" onClick={() => onRead(thread)} disabled={detailLoading} aria-label={`查看任务详情：${label}`} title="查看详情">▤</button><button className={`thread-history-action-button ${thread.isPinned ? "pinned" : ""}`} onClick={() => onPin(thread)} disabled={busy} aria-label={thread.isPinned ? `取消置顶任务：${label}` : `置顶任务：${label}`} aria-pressed={thread.isPinned} title={thread.isPinned ? "取消置顶" : "置顶"}>{thread.isPinned ? "★" : "☆"}</button><button className="thread-history-action-button" onClick={() => onArchive(thread)} disabled={busy} aria-label={`${archived ? "恢复" : "归档"}任务：${label}`} title={archived ? "恢复到活动任务" : "归档任务"}>{archived ? "↩" : "▱"}</button></div></div>; })}</div>{nextCursor && <button className="thread-history-load-more" onClick={onLoadMore} disabled={loading}>{loading ? "加载中…" : "加载更多任务"}</button>}</details>{detailLoading && <div className="thread-detail-loading" role="status">正在读取任务详情…</div>}{detail && <ThreadDetailView detail={detail} onClose={onCloseDetail} />}{action && <div className="thread-history-action" role="status">{action}</div>}</section>;
}
function threadTurnStatusLabel(status: string | null): string { return status === "completed" ? "已完成" : status === "inProgress" || status === "active" ? "进行中" : status === "failed" ? "失败" : status === "interrupted" ? "已停止" : "历史记录"; }
function threadItemLabel(type: string | null): string { return type === "userMessage" ? "你的指令" : type === "agentMessage" ? "Codex 回复" : type === "commandExecution" ? "工具执行" : type === "fileChange" ? "文件变更" : type === "reasoning" ? "思考摘要" : "执行项"; }
function ThreadDetailView({ detail, onClose }: { detail: ThreadDetail; onClose: () => void }) { const label = detail.thread.name ?? detail.thread.preview ?? "未命名任务"; return <section className="thread-detail" aria-label={`任务详情：${label}`}><div className="thread-detail-header"><div><strong>{label}</strong><small>只读任务详情 · {detail.turns.length} 个轮次</small></div><button className="thread-detail-close" onClick={onClose} aria-label="收起任务详情">收起</button></div>{detail.turns.length === 0 ? <div className="thread-detail-empty">该任务暂无可展示的历史内容。</div> : <div className="thread-detail-turns">{detail.turns.map((turn, index) => <article className="thread-detail-turn" key={turn.id}><div className="thread-detail-turn-heading"><strong>第 {index + 1} 轮</strong><span>{threadTurnStatusLabel(turn.status)}</span></div>{turn.items.length === 0 ? <p>本轮暂无可展示内容。</p> : <div className="thread-detail-items">{turn.items.map((item) => <div className="thread-detail-item" key={item.id}><span>{threadItemLabel(item.type)}</span><p>{item.text ?? "无文本摘要"}</p>{item.status && <small>{threadTurnStatusLabel(item.status)}</small>}</div>)}</div>}</article>)}</div>}</section>; }
function AgentStatusBar({ status, connectionState, threadId, onReconnect }: { status: { configured: boolean; state: string; error: string | null; provider?: string; model?: string }; connectionState: AgentRun["connectionState"]; threadId: string | null; onReconnect: () => void }) { const online = status.configured && status.state !== "error" && connectionState !== "error"; const providerLabel = status.provider === "deepseek" ? `DeepSeek · ${status.model ?? "deepseek-flash"}` : "Novel Codex 专用 CLI"; const statusText = connectionState === "reconnecting" ? "任务连接已断开，等待恢复…" : status.state === "running" ? "正在生成事件…" : status.state === "waiting" ? "等待输入或授权…" : status.error ?? (online ? "App Server stdio 已就绪" : "未连接 Novel Codex 专用 CLI，编辑和本地命令仍可用"); return <div className={`agent-status-bar ${online ? "agent-ready" : "agent-local"}`}><span className={`online-dot ${connectionState === "ready" ? "" : "offline"}`} /><strong>{online ? providerLabel : "本地 Runtime"}</strong><span>{statusText}</span>{threadId && (connectionState === "reconnecting" || connectionState === "error") && <button className="status-reconnect" onClick={onReconnect}>恢复连接</button>}</div>; }
function UserInputCard({ questions, queuedCount, onResolve }: { questions: UserInputQuestion[]; queuedCount: number; onResolve: ResolveApproval }) { const [answers, setAnswers] = useState<Record<string, string>>({}); const [otherQuestions, setOtherQuestions] = useState<Record<string, boolean>>({}); const [submitting, setSubmitting] = useState(false); const [error, setError] = useState<string | null>(null); const setAnswer = (id: string, value: string) => setAnswers((current) => ({ ...current, [id]: value })); const canSubmit = questions.length > 0 && questions.every((question) => Boolean(answers[question.id]?.trim())); async function submit() { if (!canSubmit || submitting) return; setSubmitting(true); setError(null); const accepted = await onResolve({ answers }); if (accepted === false) { setError("回答未能发送，请重试。"); } else { setAnswers({}); } setSubmitting(false); } return <div className="approval-card user-input-card"><div className="user-input-heading"><strong>Codex 需要你的回答</strong><span>回答后继续当前任务{queuedCount > 0 ? ` · 还有 ${queuedCount} 个交互请求` : ""}</span></div>{questions.length === 0 ? <div className="user-input-empty">当前请求没有可展示的问题。</div> : <div className="approval-questions">{questions.map((question, index) => { const other = otherQuestions[question.id] === true; const selected = answers[question.id] ?? ""; return <fieldset className="approval-question" key={question.id}><legend>{question.header ?? `问题 ${index + 1}`}</legend><p>{question.question}</p>{question.options.length > 0 && <div className="approval-question-options">{question.options.map((option) => <label className="approval-option" key={option.label}><input type="radio" name={`user-input-${question.id}`} value={option.label} checked={!other && selected === option.label} onChange={() => { setOtherQuestions((current) => ({ ...current, [question.id]: false })); setAnswer(question.id, option.label); }} /><span><strong>{option.label}</strong>{option.description && <small>{option.description}</small>}</span></label>)}{question.isOther && <label className="approval-option"><input type="radio" name={`user-input-${question.id}`} checked={other} onChange={() => { setOtherQuestions((current) => ({ ...current, [question.id]: true })); setAnswer(question.id, ""); }} /><span><strong>其他</strong><small>填写自定义回答</small></span></label>}</div>}{(question.options.length === 0 || (question.isOther && other)) && <textarea className="approval-input" value={selected} maxLength={240} rows={2} onChange={(event) => setAnswer(question.id, event.target.value)} placeholder="输入你的回答" aria-label={question.question} />}</fieldset>; })}</div>}{error && <div className="user-input-error" role="alert">{error}</div>}<div className="approval-card-actions"><button className="primary-button" disabled={!canSubmit || submitting} onClick={() => void submit()}>{submitting ? "发送中…" : "提交回答"}</button></div></div>; }
function ApprovalCard({ request, queuedCount, onResolve }: { request: { method: string; params: Record<string, unknown> }; queuedCount: number; onResolve: ResolveApproval }) { const params = request.params; if (isUserInputRequest(request)) return <UserInputCard questions={userInputQuestions(params)} queuedCount={queuedCount} onResolve={onResolve} />; const decisions = params.availableDecisions ?? params.available_decisions; const canAcceptForSession = Array.isArray(decisions) && decisions.includes("acceptForSession"); const network = params.networkApprovalContext; const networkRecord = network !== null && typeof network === "object" && !Array.isArray(network) ? network as Record<string, unknown> : null; const networkTarget = networkRecord ? [String(networkRecord.host ?? ""), String(networkRecord.protocol ?? "")].filter(Boolean).join(" / ") : ""; return <div className="approval-card"><div className="approval-heading"><strong>Codex 请求授权</strong>{queuedCount > 0 && <span>还有 {queuedCount} 个交互请求</span>}</div><span>{String(params.reason ?? "Codex 请求执行一项需要确认的操作")}</span>{networkTarget && <code>网络访问：{networkTarget}</code>}{!networkTarget && params.command !== undefined && <code>{String(params.command)}</code>}{params.cwd !== undefined && <small>工作目录：{String(params.cwd)}</small>}<div className="approval-card-actions"><button className="secondary-button" onClick={() => onResolve({ decision: "decline" })}>拒绝</button>{canAcceptForSession && <button className="secondary-button" onClick={() => onResolve({ decision: "acceptForSession" })}>本次会话允许</button>}<button className="primary-button" onClick={() => onResolve({ decision: "accept" })}>批准</button></div></div>; }
function ChevronDownIcon() { return <svg className="composer-icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="m4.5 6 3.5 3.5L11.5 6" /></svg>; }
function SendArrowIcon() { return <svg className="composer-icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M8 12V4m0 0L4.5 7.5M8 4l3.5 3.5" /></svg>; }
function StopIcon() { return <svg className="composer-icon composer-stop-icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><rect x="5" y="5" width="6" height="6" rx="1" /></svg>; }
function Composer({ composer, setComposer, onSubmit, onInterrupt, composerRef, activity, agentStatus, agentAvailable, blockedByUnsavedChanges }: { composer: string; setComposer: (value: string) => void; onSubmit: () => void; onInterrupt: () => void; composerRef: RefObject<HTMLTextAreaElement | null>; activity: string[]; agentStatus: { configured: boolean; state: string; model?: string | null; provider?: string | null }; agentAvailable: boolean; blockedByUnsavedChanges: boolean }) {
  const inputError = useMemo(() => composer.trim() ? authorIntentError(composer) : null, [composer]);
  const active = agentStatus.state === "running";
  const waiting = agentStatus.state === "waiting";
  const disabled = waiting || blockedByUnsavedChanges || (!active && !agentAvailable);
  const latestActivity = activity[0];
  const modelLabel = agentStatus.model ?? (agentStatus.configured ? "Codex" : "未配置模型");
  const placeholder = blockedByUnsavedChanges ? "请先保存中央文档的候选修改……" : !agentAvailable ? "请在设置中配置可用模型……" : active ? "追加指令，Codex 将在当前轮次中处理……" : waiting ? "等待交互请求完成后继续……" : "告诉 Codex 你想如何继续这个故事……";
  return <><div className="composer-card composer-global"><div className="composer-body"><textarea ref={composerRef} aria-label="创作指令" disabled={disabled} value={composer} onChange={(event) => setComposer(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); onSubmit(); } }} placeholder={placeholder} /><div className="composer-bottom"><div><span className="composer-option">{modelLabel}<small><ChevronDownIcon /></small></span>{active ? <><button className="composer-steer" disabled={blockedByUnsavedChanges || Boolean(inputError) || !composer.trim()} aria-label="追加创作指令" onClick={onSubmit}><SendArrowIcon /></button><button className="composer-stop" aria-label="停止 Codex 执行" onClick={onInterrupt}><StopIcon /></button></> : waiting ? <span className="composer-waiting">等待交互</span> : <button className="composer-send" disabled={disabled || !composer.trim() || Boolean(inputError)} aria-label="发送创作指令" onClick={onSubmit}><SendArrowIcon /></button>}</div></div></div></div>{inputError && <p className="dialog-error" role="alert">{inputError}</p>}{latestActivity && <div className="activity-strip" role="status" aria-live="polite"><div><span className="activity-spark">✦</span>{latestActivity}</div></div>}</>;
}
function Metric({ label, value, detail, icon }: { label: string; value: string; detail: string; icon: string }) { return <div className="metric-card"><div className="metric-icon">{icon}</div><div><span>{label}</span><strong>{value}</strong><small>{detail}</small></div></div>; }
function searchKindLabel(kind: SearchResult["kind"]): string { return { project: "作品", character: "角色", relationship: "关系", event: "事件", location: "地点", worldRule: "世界规则", plotThread: "情节线", scene: "场景", chapter: "章节" }[kind]; }
function TestPanel({ result }: { result: StoryTestResult }) { return <div className={`test-panel ${result.passed ? "passed" : "failed"}`}><strong>Story Tests {result.passed ? "全部通过" : "发现问题"}</strong>{result.checks.map((check) => <span key={check.id} title={check.details}>{check.passed ? "✓" : "!"} {check.label}{!check.passed && ` · ${check.details}`}</span>)}</div>; }
function updateLocalChapter(current: RuntimeState, chapterId: string, body: string): RuntimeState { const chapters = current.novelEdition.chapters.map((chapter) => chapter.id === chapterId ? { ...chapter, updated_at: new Date().toISOString(), passages: chapter.passages.map((item, index) => index === 0 ? { ...item, body } : item) } : chapter); return { ...current, novelEdition: { ...current.novelEdition, chapters } }; }
function formatDate(value?: string) { return value ? new Date(value).toLocaleString("zh-CN", { hour: "2-digit", minute: "2-digit" }) : "—"; }

function AgentInspector({ state, runtimeStatus, agentRun, contextTab, onContextTabChange, onAddCanonNote, onReview, onCompact, backgroundTerminals, backgroundTerminalLoading, onLoadBackgroundTerminals, onTerminateBackgroundTerminal, onConfigureAgent, agentConfigSaving }: { state: RuntimeState; runtimeStatus: RuntimeStatus; agentRun: AgentRun; contextTab: "story" | "task" | "changes" | "usage" | "diagnostics" | "settings"; onContextTabChange: (tab: "story" | "task" | "changes" | "usage" | "diagnostics" | "settings") => void; onAddCanonNote: () => void; onReview: () => void; onCompact: () => void; backgroundTerminals: BackgroundTerminal[]; backgroundTerminalLoading: boolean; onLoadBackgroundTerminals: () => void; onTerminateBackgroundTerminal: (processId: string) => void; onConfigureAgent: (provider: string, model: string) => void; agentConfigSaving: boolean }) {
  const tabs: { id: typeof contextTab; label: string }[] = [{ id: "story", label: "Story" }, { id: "task", label: "Task" }, { id: "changes", label: "Changes" }, { id: "usage", label: "Usage" }, { id: "diagnostics", label: "Debug" }, { id: "settings", label: "设置" }];
  const status = agentRun.state === "running" ? "执行中" : agentRun.state === "waiting" ? "等待确认" : agentRun.state === "error" ? "执行失败" : agentRun.state === "completed" ? "已完成" : "暂无执行";
  const model = agentRun.model ?? "未指定";
  const provider = agentRun.provider ?? "默认 Provider";
  const [providerDraft, setProviderDraft] = useState(provider === "deepseek" ? "deepseek" : "openai");
  const [modelDraft, setModelDraft] = useState(agentRun.model ?? "gpt-5.6-terra");
  useEffect(() => { setProviderDraft(agentRun.provider === "deepseek" ? "deepseek" : "openai"); setModelDraft(agentRun.model ?? (agentRun.provider === "deepseek" ? "deepseek-flash" : "gpt-5.6-terra")); }, [agentRun.model, agentRun.provider]);
  const renderSettings = <div className="inspector-stack"><div className="inspector-summary-line"><strong>Agent 配置</strong><span>仅影响 Novel Codex</span></div><label className="settings-field"><span>Provider</span><select value={providerDraft} onChange={(event) => { setProviderDraft(event.target.value); setModelDraft(event.target.value === "deepseek" ? "deepseek-flash" : "gpt-5.6-terra"); }}><option value="openai">OpenAI</option><option value="deepseek">DeepSeek（仅讨论）</option></select></label><label className="settings-field"><span>Model</span><select value={modelDraft} onChange={(event) => setModelDraft(event.target.value)}>{providerDraft === "deepseek" ? <option value="deepseek-flash">deepseek-flash</option> : <option value="gpt-5.6-terra">gpt-5.6-terra</option>}</select></label><button className="primary-button" onClick={() => onConfigureAgent(providerDraft, modelDraft.trim())} disabled={agentConfigSaving || !modelDraft.trim()}>{agentConfigSaving ? "保存中…" : "应用配置"}</button><small className="inspector-note">模型由审核目录固定；DeepSeek 仅用于自由讨论。配置写入隔离的 Codex Home，API Key 只从主进程环境读取。</small></div>;
  const renderStory = <><div className="context-section"><div className="section-title"><span>CANON NOTES</span><button onClick={onAddCanonNote}>＋</button></div><div className="canon-list">{state.canon.notes.map((note) => <div className="canon-note" key={note}><span className="note-pin">•</span>{note}</div>)}</div></div><div className="context-section"><div className="section-title"><span>ACTIVE CHARACTERS</span><span className="section-count">{state.canon.characters.length}</span></div><div className="character-list">{state.canon.characters.map((character) => <div className="character-row" key={character.id}><div className="character-avatar" style={{ background: character.color }}>{character.name.slice(0, 1)}</div><div><strong>{character.name}</strong><span>{character.role}</span></div><span className="character-status">{character.status}</span></div>)}</div></div><div className="context-section"><div className="section-title"><span>OPEN THREADS</span><span className="section-count">{state.canon.plotThreads.length}</span></div>{state.canon.plotThreads.map((thread) => <div className="thread-row" key={thread.id}><span className={`thread-line ${thread.tone}`} /><span>{thread.title}</span><small>{thread.state}</small></div>)}</div></>;
  const renderTask = <div className="inspector-stack"><div className="inspector-status-card"><span className={`online-dot ${agentRun.connectionState === "ready" ? "" : "offline"}`} /><div><strong>{status}</strong><small>Codex App Server</small></div></div><div className="inspector-facts"><span><small>Provider</small><strong>{provider}</strong></span><span><small>Model</small><strong>{model}</strong></span><span><small>Connection</small><strong>{connectionLabel(agentRun.connectionState)}</strong></span><span><small>Thread</small><strong>{agentRun.threadId ? "已连接" : "未创建"}</strong></span></div><div className="inspector-actions"><button className="secondary-button" onClick={onReview} disabled={!agentRun.threadId || agentRun.review === "running"}>{agentRun.review === "running" ? "审查中…" : "审查变更"}</button><button className="secondary-button" onClick={onCompact} disabled={!agentRun.threadId}>压缩上下文</button></div><div className="inspector-subsection"><div className="section-title"><span>BACKGROUND TERMINALS</span><button onClick={onLoadBackgroundTerminals} disabled={!agentRun.threadId || backgroundTerminalLoading}>{backgroundTerminalLoading ? "…" : "↻"}</button></div>{backgroundTerminals.length === 0 ? <small className="inspector-empty">没有已登记的后台终端</small> : backgroundTerminals.map((terminal) => <div className="terminal-row" key={terminal.processId}><div><strong>{terminal.command ?? `进程 ${terminal.processId}`}</strong><small>{terminal.cwd ?? "工作目录未知"} · {terminal.status ?? "运行中"}</small></div><button onClick={() => onTerminateBackgroundTerminal(terminal.processId)}>终止</button></div>)}</div></div>;
  const renderChanges = <div className="inspector-stack"><div className="inspector-summary-line"><strong>文件与内容变更</strong><span>{agentRun.artifacts.length} 项</span></div>{agentRun.artifacts.length === 0 && !agentRun.diff ? <div className="context-empty">当前任务还没有报告变更。</div> : <>{agentRun.artifacts.map((artifact) => <div className="change-row" key={artifact.id}><span className={`change-status ${artifact.status}`}>{artifact.status === "applied" ? "✓" : artifact.status === "declined" ? "–" : artifact.status === "failed" ? "×" : "·"}</span><div><strong>{artifact.title}</strong><small>{artifactStatusLabel(artifact.status)}</small>{artifact.detail && <pre>{artifact.detail}</pre>}</div></div>)}{agentRun.diff && <details className="agent-diff"><summary>统一 diff</summary><pre>{agentRun.diff}</pre></details>}</>}</div>;
  const renderUsage = <div className="inspector-stack"><div className="usage-card"><span>本轮 Token</span><strong>{formatTokenCount(agentRun.tokenUsage?.totalTokens)}</strong><small>输入 {formatTokenCount(agentRun.tokenUsage?.inputTokens)} · 输出 {formatTokenCount(agentRun.tokenUsage?.outputTokens)}</small></div><div className="inspector-facts"><span><small>Context Window</small><strong>{formatTokenCount(agentRun.tokenUsage?.contextWindow)}</strong></span><span><small>Compaction</small><strong>{agentRun.contextCompactions} 次</strong></span><span><small>Run Started</small><strong>{agentRun.startedAt ? formatDate(new Date(agentRun.startedAt).toISOString()) : "—"}</strong></span></div><small className="inspector-note">Token 与上下文事件来自隔离的 App Server 会话。</small></div>;
  const renderDiagnostics = <div className="inspector-stack"><div className="inspector-summary-line"><strong>运行诊断</strong><span>{agentRun.diagnostics.length} 条</span></div><div className="diagnostic-row"><span className={`online-dot ${runtimeStatus.state === "online" ? "" : "offline"}`} /><span>Creative Runtime：{runtimeStatus.state === "online" ? "在线" : "离线"}</span></div><div className="diagnostic-row"><span className={`online-dot ${agentRun.connectionState === "ready" ? "" : "offline"}`} /><span>Codex 连接：{connectionLabel(agentRun.connectionState)}</span></div>{agentRun.diagnostics.length === 0 ? <div className="context-empty">暂无诊断信息。</div> : <div className="diagnostic-list">{agentRun.diagnostics.map((message, index) => <p key={`${message}-${index}`}>{message}</p>)}</div>}</div>;
  const content = contextTab === "story" ? renderStory : contextTab === "task" ? renderTask : contextTab === "changes" ? renderChanges : contextTab === "usage" ? renderUsage : contextTab === "diagnostics" ? renderDiagnostics : <>{renderSettings}{window.novelCodex && <MetricsPanel api={window.novelCodex} />}</>;
  return <aside className="context-panel"><div className="panel-tabs inspector-tabs" role="tablist" aria-label="任务检查器">{tabs.map((tab) => <button key={tab.id} className={contextTab === tab.id ? "selected" : ""} role="tab" aria-selected={contextTab === tab.id} onClick={() => onContextTabChange(tab.id)}>{tab.label}</button>)}</div><div className="inspector-content">{content}</div><div className="context-footer"><span>{runtimeStatus.state === "online" ? "Context synchronized" : "Local fallback"}</span><span className={`online-dot ${runtimeStatus.state === "online" ? "" : "offline"}`} /></div></aside>;
}

function connectionLabel(state: AgentRun["connectionState"]): string { return state === "ready" ? "已连接" : state === "connecting" ? "连接中" : state === "reconnecting" ? "恢复中" : state === "error" ? "异常" : "离线"; }
function artifactStatusLabel(status: AgentRun["artifacts"][number]["status"]): string { return status === "applied" ? "已应用" : status === "declined" ? "已拒绝" : status === "failed" ? "失败" : "待确认"; }
function formatTokenCount(value: number | null | undefined): string { return value === null || value === undefined ? "—" : value.toLocaleString("zh-CN"); }

function stepStatusLabel(status: AgentRun["steps"][number]["status"]): string { return status === "running" ? "进行中" : status === "waiting" ? "等待中" : status === "failed" ? "失败" : "完成"; }
function stepMarker(kind: AgentRun["steps"][number]["kind"]): string { return kind === "tool" ? ">" : kind === "approval" ? "!" : kind === "error" ? "×" : kind === "assistant" ? "✦" : "·"; }
function workSteps(run: AgentRun): AgentRun["steps"] { return run.steps.filter((step) => step.kind !== "assistant" && step.id !== "agent-message"); }

function runDuration(run: AgentRun): string { if (!run.startedAt) return "—"; const seconds = Math.max(0, Math.round(((run.completedAt ?? Date.now()) - run.startedAt) / 1000)); return seconds < 1 ? "<1s" : seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`; }
function runDurationLabel(run: AgentRun): string { if (!run.startedAt) return "—"; const seconds = Math.max(0, Math.round(((run.completedAt ?? Date.now()) - run.startedAt) / 1000)); return seconds < 60 ? `${seconds}秒` : `${Math.floor(seconds / 60)}分钟 ${seconds % 60}秒`; }
function messageClock(run: AgentRun): string { return new Date(run.startedAt ?? Date.now()).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" }); }
function splitConversationBlocks(text: string): string[] {
  const blocks: string[] = [];
  let current: string[] = [];
  let mode: "paragraph" | "heading" | "list" | "code" = "paragraph";
  let inCode = false;
  const flush = () => { if (current.length > 0) { blocks.push(current.join("\n").trim()); current = []; } };
  for (const line of text.replace(/\r\n/g, "\n").split("\n")) {
    const trimmed = line.trim();
    if (inCode) {
      current.push(line);
      if (trimmed === "```") { inCode = false; flush(); mode = "paragraph"; }
      continue;
    }
    if (trimmed.startsWith("```")) { flush(); inCode = true; mode = "code"; current = [line]; continue; }
    if (!trimmed) { flush(); mode = "paragraph"; continue; }
    const nextMode = /^#{1,3}\s+/.test(trimmed) ? "heading" : /^[-*]\s+|^\d+\.\s+/.test(trimmed) ? "list" : "paragraph";
    if (current.length > 0 && nextMode !== mode) flush();
    mode = nextMode;
    current.push(line);
    if (nextMode === "heading") flush();
  }
  if (inCode || current.length > 0) flush();
  return blocks;
}

function renderConversationAnswer(text: string) { return splitConversationBlocks(text).map((block, index) => { const lines = block.split("\n"); const first = lines[0].trim(); if (first.startsWith("```")) { const language = first.slice(3).trim(); const codeLines = lines.slice(1); if (codeLines.at(-1)?.trim() === "```") codeLines.pop(); const code = codeLines.join("\n"); return <div className="conversation-code-card" key={`code-${index}`}><div className="conversation-code-toolbar"><span><span className="conversation-code-icon" aria-hidden="true">〈/〉</span>纯文本{language ? ` · ${language}` : ""}</span><button className="conversation-copy" type="button" title="复制代码" aria-label="复制代码" onClick={() => { if (navigator.clipboard) void navigator.clipboard.writeText(code).catch(() => undefined); }}>▢</button></div><pre className="conversation-code"><code>{code}</code></pre></div>; } if (lines.length === 1 && /^#{1,3}\s+/.test(first)) return <h3 className="conversation-heading" key={`heading-${index}`}>{first.replace(/^#{1,3}\s+/, "")}</h3>; if (lines.every((line) => /^[-*]\s+/.test(line.trim()))) return <ul className="conversation-list" key={`list-${index}`}>{lines.map((line, itemIndex) => <li key={`${index}-${itemIndex}`}>{line.trim().replace(/^[-*]\s+/, "")}</li>)}</ul>; if (lines.every((line) => /^\d+\.\s+/.test(line.trim()))) return <ol className="conversation-list" key={`ordered-${index}`}>{lines.map((line, itemIndex) => <li key={`${index}-${itemIndex}`}>{line.trim().replace(/^\d+\.\s+/, "")}</li>)}</ol>; return <p key={`paragraph-${index}`}>{block}</p>; }); }

function ConversationWorkItem({ step }: { step: AgentRun["steps"][number] }) {
  const content = step.text || step.detail;
  const expandable = Boolean(content);
  const expanded = step.status === "running" || step.status === "waiting" || step.status === "failed";
  const heading = <><span className={`conversation-work-marker ${step.kind}`} aria-hidden="true">{stepMarker(step.kind)}</span><span className="conversation-work-copy"><strong>{step.title}</strong><small>{stepStatusLabel(step.status)}</small></span>{expandable && <span className="conversation-work-toggle">{expanded ? "展开" : "查看详情"}</span>}</>;
  if (!expandable) return <div className={`conversation-work-item ${step.kind} ${step.status}`}>{heading}</div>;
  return <details className={`conversation-work-item ${step.kind} ${step.status}`} open={expanded}><summary>{heading}</summary><div className="conversation-work-detail">{step.text && <p>{step.text}{step.status === "running" && <span className="stream-cursor" aria-label="正在接收内容">▋</span>}</p>}{step.detail && <pre>{step.detail}</pre>}</div></details>;
}

function ConversationWorkDetails({ run, steps }: { run: AgentRun; steps: AgentRun["steps"] }) {
  const [open, setOpen] = useState(run.state === "error");
  const currentStep = steps.find((step) => step.status === "running" || step.status === "waiting");
  return <details className="conversation-work-details" open={open} onToggle={(event) => setOpen(event.currentTarget.open)}><summary>查看完整执行过程 · {steps.length} 步</summary><div className="conversation-work-details-body"><div className="conversation-work-overview"><span className={`conversation-work-live ${run.state}`} aria-hidden="true">{run.state === "completed" ? "✓" : run.state === "error" ? "×" : run.state === "waiting" ? "!" : "•"}</span><div><strong>{currentStep?.title ?? run.phases.find((phase) => phase.status === "running" || phase.status === "waiting")?.label ?? "任务处理"}</strong><small>{currentStep ? stepStatusLabel(currentStep.status) : run.state === "completed" ? "已完成" : "等待下一步"} · 已处理 {runDurationLabel(run)}</small></div></div><AgentPhaseRail run={run}/><AgentPlan run={run}/><div className="transcript-steps" role="log" aria-label="Codex Agent 执行过程">{steps.length === 0 ? <div className="conversation-work-empty">当前没有可展开的执行详情。</div> : steps.map((step) => <ConversationWorkItem step={step} key={step.id}/>)}</div>{run.diagnostics.length > 0 && <details className="run-diagnostics"><summary>诊断信息 · {run.diagnostics.length}</summary>{run.diagnostics.map((message, index) => <p key={`${message}-${index}`}>{message}</p>)}</details>}</div></details>;
}

function ConversationWorkLog({ run }: { run: AgentRun }) {
  const steps = workSteps(run);
  const hasWork = steps.length > 0 || run.plan.length > 0 || run.phases.some((phase) => phase.status !== "pending");
  const live = run.state === "running" || run.state === "waiting";
  const [, setClock] = useState(0);
  useEffect(() => { if (!live) return; const timer = window.setInterval(() => setClock((value) => value + 1), 1000); return () => window.clearInterval(timer); }, [live]);
  if (!hasWork) return null;
  const expanded = run.state === "running" || run.state === "waiting" || run.state === "error";
  const summary = live ? `已处理 ${runDurationLabel(run)}` : run.state === "error" ? `已处理 ${runDurationLabel(run)}` : `用时 ${runDurationLabel(run)}`;
  const currentStep = steps.find((step) => step.status === "running" || step.status === "waiting");
  const currentPhase = run.phases.find((phase) => phase.status === "running" || phase.status === "waiting");
  const thinkingText = currentStep?.detail ?? currentStep?.title ?? currentPhase?.detail ?? currentPhase?.label ?? (live ? "正在整理上下文并规划下一步" : "执行过程已完成");
  return <details className="conversation-work" open={expanded}><summary><span className="conversation-work-title">{summary}</span><span className="conversation-work-meta" aria-hidden="true">›</span></summary><div className="conversation-work-body"><div className="conversation-thinking"><span>{live ? "正在思考" : run.state === "error" ? "执行失败" : "执行记录"}</span><p>{thinkingText}</p></div><ConversationWorkDetails run={run} steps={steps}/></div></details>;
}

type ChangeStats = { added: number; removed: number; known: boolean };

function changeStats(text: string): ChangeStats {
  let added = 0;
  let removed = 0;
  for (const line of text.split("\n")) {
    if (line.startsWith("+++") || line.startsWith("---")) continue;
    if (line.startsWith("+")) added += 1;
    if (line.startsWith("-")) removed += 1;
  }
  return { added, removed, known: added > 0 || removed > 0 };
}

function totalChangeStats(run: AgentRun): ChangeStats {
  const source = run.diff || run.artifacts.map((artifact) => artifact.detail ?? "").filter(Boolean).join("\n");
  return changeStats(source);
}

function ChangeStatsLabel({ stats }: { stats: ChangeStats }) {
  if (!stats.known) return null;
  return <span className="change-stats"><span className="change-stat-add">+{stats.added}</span><span className="change-stat-remove">−{stats.removed}</span></span>;
}

function ConversationChanges({ run, onReview }: { run: AgentRun; onReview: () => void }) {
  const [open, setOpen] = useState(true);
  if (run.artifacts.length === 0 && !run.diff) return null;
  const pending = run.artifacts.some((artifact) => artifact.status === "proposed");
  const stats = totalChangeStats(run);
  const fileLabel = run.artifacts.length > 0 ? `${run.artifacts.length} 个文件已更改` : "内容已更改";
  return <section className={`conversation-changes ${pending ? "pending" : ""}`} aria-label="文件变更"><div className="conversation-change-header"><div className="conversation-change-title"><span className="conversation-change-icon" aria-hidden="true">▧</span><div><strong>{fileLabel}</strong><ChangeStatsLabel stats={stats} /></div></div><div className="conversation-change-actions"><button className="conversation-change-undo" disabled title="当前仅展示变更，回滚能力尚未接入">撤销</button><button className="conversation-change-review" onClick={onReview}>{run.review === "running" ? "审查中" : "审查"}</button></div></div>{open && <div className="conversation-change-list">{run.artifacts.map((artifact) => { const artifactStats = changeStats(artifact.detail ?? ""); return <article className="artifact-row" key={artifact.id}><span className="artifact-icon" aria-hidden="true">{artifact.kind === "chapter-change" ? "▤" : "◇"}</span><div className="artifact-copy"><div className="artifact-heading"><strong>{artifact.title}</strong><ChangeStatsLabel stats={artifactStats} /></div>{artifact.detail && <details className="change-detail"><summary>{artifactStatusLabel(artifact.status)} · 查看详情</summary><pre>{artifact.detail}</pre></details>}</div></article>; })}{run.diff && <details className="agent-diff"><summary>查看统一 diff</summary><pre>{run.diff}</pre></details>}</div>}<button className="conversation-change-toggle" aria-expanded={open} onClick={() => setOpen((value) => !value)}>{open ? "⌃" : "⌄"}</button></section>;
}

function ConversationHistory({ history }: { history: AgentRun["history"] }) {
  if (history.length === 0) return null;
  return <details className="conversation-history"><summary>此前回合 · {history.length}</summary><div className="conversation-history-list">{history.map((entry) => <article className="conversation-history-item" key={entry.id}><div className="conversation-history-prompt"><span className="transcript-avatar user">你</span><p>{entry.prompt}</p><span className={`history-status ${entry.state}`}>{agentStateLabel(entry.state)}</span></div><div className="conversation-history-answer"><span className="conversation-avatar assistant" aria-hidden="true">✦</span><p>{entry.summary}</p></div><small>Codex App Server · {formatDate(new Date(entry.createdAt).toISOString())}{entry.artifactCount > 0 ? ` · 变更 ${entry.artifactCount} 项` : ""}</small></article>)}</div></details>;
}

function EnhancedAgentTranscript({ run, approvalRequest, queuedApprovalCount, onResolveApproval, onClear, onRetry, onInterrupt, onReview, onCompact }: { run: AgentRun; approvalRequest: ApprovalRequest | null; queuedApprovalCount: number; onResolveApproval: ResolveApproval; onClear: () => void; onRetry: () => void; onInterrupt: () => void; onReview: () => void; onCompact: () => void }) {
  if (run.state === "idle") return null;
  const stateLabel = run.connectionState === "reconnecting" ? "连接恢复中" : run.state === "running" ? "执行中" : run.state === "waiting" ? "等待交互" : run.state === "error" ? "执行失败" : run.state === "cancelled" ? "已停止" : "已完成";
  const modeLabel = "Codex App Server";
  const answer = run.answer.trim();
  return <section className="agent-transcript conversation-panel" id="agent-transcript" aria-label="当前 Codex 对话回合"><div className="conversation-header"><div><span className="conversation-kicker">当前回合</span><span className={`run-mode ${run.mode}`}>{modeLabel}</span><span className={`run-state ${run.state}`}>{stateLabel}</span></div><div className="transcript-actions">{run.state === "running" && <button className="transcript-stop" onClick={onInterrupt}>停止</button>}{run.threadId && <><button className="transcript-action" onClick={onReview} disabled={run.review === "running"}>{run.review === "running" ? "审查中" : "审查"}</button><button className="transcript-action" onClick={onCompact}>压缩</button></>}{(run.state === "error" || run.state === "completed" || run.state === "cancelled") && <button className="transcript-retry" onClick={onRetry}>重试</button>}<button className="transcript-clear" onClick={onClear}>清除</button></div></div><div className="conversation-turn"><article className="conversation-message user-message"><span className="transcript-avatar user">你</span><div className="conversation-message-body"><div className="conversation-message-meta"><strong>你的创作指令</strong><span>本轮输入</span></div><p>{run.prompt}</p><div className="conversation-user-meta"><span>{messageClock(run)}</span><button className="conversation-copy" title="复制我的创作指令" aria-label="复制我的创作指令" onClick={() => { if (navigator.clipboard) void navigator.clipboard.writeText(run.prompt).catch(() => undefined); }}>▢</button></div></div></article><article className="conversation-message assistant-message"><span className="conversation-avatar assistant" aria-hidden="true">✦</span><div className="conversation-message-body"><div className="conversation-message-meta"><strong>Codex</strong><span>{answer ? "回复" : run.state === "running" ? "正在处理" : "暂无文本回复"}</span>{answer && <button className="conversation-copy" title="复制 Codex 回复" aria-label="复制 Codex 回复" onClick={() => { if (navigator.clipboard) void navigator.clipboard.writeText(answer).catch(() => undefined); }}>复制</button>}</div>{answer ? <div className="conversation-answer" aria-live="polite">{renderConversationAnswer(answer)}{run.state === "running" && <span className="stream-cursor" aria-label="正在接收内容">▋</span>}</div> : <div className="conversation-answer-empty" aria-live="polite"><span className="conversation-answer-pulse" aria-hidden="true">•</span>{run.state === "waiting" ? "等待你的回答后继续。" : run.state === "running" ? "正在读取故事上下文并生成回复……" : "本轮没有文本回复。"}</div>}</div></article>{approvalRequest && <div className="conversation-interaction"><div className="conversation-interaction-heading"><span>需要你的决定</span><small>处理后 Codex 会继续当前回合{queuedApprovalCount > 0 ? ` · 之后还有 ${queuedApprovalCount} 个请求` : ""}</small></div><EnhancedApprovalCard request={approvalRequest} queuedCount={queuedApprovalCount} onResolve={onResolveApproval}/></div>}{run.error && <div className="transcript-error" role="alert"><strong>本轮未完成</strong><span>{run.error}</span></div>}<ConversationChanges run={run} onReview={onReview}/><ConversationWorkLog run={run}/>{run.summary && <div className="conversation-result"><strong>{run.state === "completed" ? "本轮完成" : "本轮状态"}</strong><p>{run.summary}</p></div>}<ConversationHistory history={run.history}/></div></section>;
}

function CommandPalette({ onClose, onFocusComposer, onReview, onCompact, onReconnect, onTheme }: { onClose: () => void; onFocusComposer: () => void; onReview: () => void; onCompact: () => void; onReconnect: () => void; onTheme: () => void }) {
  const commands = [{ label: "聚焦创作输入", hint: "⌘↵", action: onFocusComposer }, { label: "审查当前变更", hint: "", action: onReview }, { label: "压缩上下文", hint: "", action: onCompact }, { label: "恢复 Codex 连接", hint: "", action: onReconnect }, { label: "切换主题", hint: "", action: onTheme }];
  return <div className="command-palette-backdrop" role="presentation" onMouseDown={(event) => { if (event.currentTarget === event.target) onClose(); }}><section className="command-palette" role="dialog" aria-modal="true" aria-label="命令面板"><div className="command-palette-heading"><strong>命令面板</strong><button onClick={onClose} aria-label="关闭命令面板">Esc</button></div><div className="command-palette-list">{commands.map((command) => <button key={command.label} onClick={command.action}><span>✦</span><strong>{command.label}</strong><small>{command.hint}</small></button>)}</div></section></div>;
}

export default App;

function EnhancedApprovalCard({ request, queuedCount, onResolve }: { request: ApprovalRequest; queuedCount: number; onResolve: ResolveApproval }) {
  const params = request.params;
  if (isUserInputRequest(request)) return <UserInputCard questions={userInputQuestions(params)} queuedCount={queuedCount} onResolve={onResolve} />;
  const method = request.method.toLowerCase();
  const isFileChange = method.includes("filechange") || method.includes("applypatch") || method.includes("file_change");
  const decisions = params.availableDecisions ?? params.available_decisions;
  const canAcceptForSession = Array.isArray(decisions) && decisions.includes("acceptForSession");
  const network = eventRecord(params.networkApprovalContext);
  const networkTarget = network ? [String(network.host ?? ""), String(network.protocol ?? "")].filter(Boolean).join(" / ") : "";
  const changes = Array.isArray(params.changes) ? params.changes.slice(0, 8).flatMap((entry) => { const change = eventRecord(entry); if (!change) return []; const path = eventText(change.path) ?? "未命名文件"; const summary = eventText(change.diff); return [{ path, summary }]; }) : [];
  const permissions = eventRecord(params.additionalPermissions);
  return <div className={`approval-card ${isFileChange ? "file-approval-card" : ""}`}><div className="approval-heading"><strong>{isFileChange ? "Codex 请求修改文件" : "Codex 请求授权"}</strong>{queuedCount > 0 && <span>还有 {queuedCount} 个交互请求</span>}</div><span>{String(params.reason ?? (isFileChange ? "请确认是否应用以下文件变更" : "Codex 请求执行一项需要确认的操作"))}</span>{changes.length > 0 && <div className="approval-change-list">{changes.map((change) => <div className="approval-change" key={`${change.path}-${change.summary ?? ""}`}><strong>{change.path}</strong>{change.summary && <pre>{change.summary}</pre>}</div>)}</div>}{permissions && <small>附加权限：{Object.keys(permissions).slice(0, 6).join("、")}</small>}{networkTarget && <code>网络访问：{networkTarget}</code>}{!networkTarget && params.command !== undefined && <code>{String(params.command)}</code>}{params.cwd !== undefined && <small>工作目录：{String(params.cwd)}</small>}<div className="approval-card-actions"><button className="secondary-button" onClick={() => onResolve({ decision: "decline" })}>拒绝</button>{canAcceptForSession && <button className="secondary-button" onClick={() => onResolve({ decision: "acceptForSession" })}>本次会话允许</button>}<button className="primary-button" onClick={() => onResolve({ decision: "accept" })}>批准</button></div></div>;
}

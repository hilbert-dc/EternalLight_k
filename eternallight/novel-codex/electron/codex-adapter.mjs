import { spawn } from "node:child_process";
import readline from "node:readline";

const MAX_EVENT_BYTES = 64 * 1024;
const MAX_AGENT_MESSAGE_TEXT = 64 * 1024;
const MAX_THREAD_TEXT = 240;
const MAX_THREAD_LIST = 20;
const MAX_THREAD_DETAIL_TURNS = 12;
const MAX_THREAD_DETAIL_ITEMS = 8;
const MAX_USER_INPUT_QUESTIONS = 3;
const MAX_USER_INPUT_OPTIONS = 6;
const MAX_USER_INPUT_TEXT = 240;
const PINNED_THREAD_SECTION_ID = "01984de2-8f74-7c91-a3b2-5c5e937cf318";
const MODEL_PROTOCOL_ERROR = "模型未按结构化工具协议返回，已阻止内部协议文本进入对话。请重试；若持续发生，请检查模型工具模式。";

function waitForChildClose(child, timeoutMs) {
  if (child.exitCode !== null) return Promise.resolve(true);
  return new Promise((resolve) => {
    const onClose = () => { clearTimeout(timer); resolve(true); };
    const timer = setTimeout(() => { child.removeListener("close", onClose); resolve(false); }, timeoutMs);
    child.once("close", onClose);
  });
}

function splitCommand(command) {
  return command.match(/(?:[^\s"]+|"[^"]*")+/g)?.map((part) => part.replace(/^"|"$/g, "")) ?? [];
}

function safeMessage(message) {
  const encoded = JSON.stringify(message);
  return encoded.length > MAX_EVENT_BYTES ? { method: "agent/error", params: { message: "Novel Agent Engine 事件超过显示上限" } } : message;
}

function bounded(value, limit) {
  return typeof value === "string" && value.length > limit ? `${value.slice(0, limit)}…` : value;
}

function agentMessageText(item) {
  if (typeof item?.text === "string") return item.text;
  if (!Array.isArray(item?.content)) return null;
  const text = item.content
    .map((part) => typeof part?.text === "string" ? part.text : "")
    .filter(Boolean)
    .join("");
  return text || null;
}

function isModelProtocolLeak(value) {
  return typeof value === "string" && /<(?:\|\||｜｜)\s*DSML\s*(?:\|\||｜｜)/i.test(value);
}

function messageContainsModelProtocolLeak(message) {
  if (message?.method === "item/agentMessage/delta" && isModelProtocolLeak(message.params?.delta)) return true;
  if (message?.method === "item/completed" && message.params?.item?.type === "agentMessage") {
    return isModelProtocolLeak(agentMessageText(message.params.item));
  }
  if (message?.method !== "turn/completed" || !Array.isArray(message.params?.turn?.items)) return false;
  return message.params.turn.items.some((item) => item?.type === "agentMessage" && isModelProtocolLeak(agentMessageText(item)));
}

function isUserInputMethod(method) {
  return method === "item/tool/requestUserInput";
}

function validRequestId(value) {
  return (typeof value === "string" && value.trim()) || (typeof value === "number" && Number.isSafeInteger(value));
}

function validUserInputId(value) {
  return typeof value === "string" && value.trim() && value.length <= MAX_USER_INPUT_TEXT && !/[\r\n]/.test(value);
}

function userInputQuestions(params) {
  if (!Array.isArray(params?.questions)) return [];
  return params.questions.slice(0, MAX_USER_INPUT_QUESTIONS).filter((question) => validUserInputId(question?.id) && typeof question?.question === "string").map((question) => ({
    id: question.id,
    question: question.question,
    options: Array.isArray(question.options) ? question.options.slice(0, MAX_USER_INPUT_OPTIONS).filter((option) => typeof option?.label === "string" && option.label.trim()) : [],
    isOther: question.isOther === true,
  }));
}

function boundedUserInputText(value) {
  return typeof value === "string" && value.trim() && value.length <= MAX_USER_INPUT_TEXT && !/[\r\n]/.test(value);
}

function validateUserInputResponse(response, params) {
  if (response === null || typeof response !== "object" || Array.isArray(response) || response.answers === null || typeof response.answers !== "object" || Array.isArray(response.answers)) throw new Error("用户回答格式无效");
  const questions = userInputQuestions(params);
  if (questions.length === 0) throw new Error("用户问题格式无效");
  const answers = response.answers;
  const answerIds = Object.keys(answers);
  if (answerIds.length !== questions.length || questions.some((question) => !Object.hasOwn(answers, question.id))) throw new Error("必须回答所有问题");
  const normalized = {};
  for (const question of questions) {
    const answer = answers[question.id];
    const values = Array.isArray(answer) ? answer : [answer];
    if (values.length === 0 || values.length > MAX_USER_INPUT_OPTIONS || values.some((value) => !boundedUserInputText(value))) throw new Error("用户回答内容无效或过长");
    if (question.options.length > 0 && !question.isOther && values.some((value) => !question.options.some((option) => option.label === value))) throw new Error("用户回答不在可选项中");
    normalized[question.id] = { answers: values };
  }
  return { answers: normalized };
}

function validateProcessId(processId) {
  if (typeof processId !== "string" || !processId.trim() || processId.length > MAX_THREAD_TEXT || /[\r\n]/.test(processId)) throw new Error("后台终端 processId 无效");
  return processId;
}

function validateThreadId(threadId) {
  if (typeof threadId !== "string" || !threadId.trim() || threadId.length > MAX_THREAD_TEXT || /[\r\n]/.test(threadId)) throw new Error("线程 ID 无效");
  return threadId;
}

function threadSummary(thread) {
  const status = thread?.status;
  return {
    id: bounded(thread?.id, MAX_THREAD_TEXT),
    name: bounded(thread?.name ?? null, MAX_THREAD_TEXT),
    preview: bounded(thread?.preview ?? null, MAX_THREAD_TEXT),
    modelProvider: bounded(thread?.modelProvider ?? null, 80),
    status: bounded(status?.type ?? null, 40),
    createdAt: typeof thread?.createdAt === "number" ? thread.createdAt : null,
    updatedAt: typeof thread?.updatedAt === "number" ? thread.updatedAt : null,
    isPinned: thread?.section?.id === PINNED_THREAD_SECTION_ID,
    ephemeral: thread?.ephemeral === true,
  };
}

function threadItemSummary(item, index) {
  const type = bounded(item?.type ?? "unknown", 40);
  const contentText = Array.isArray(item?.content)
    ? item.content.map((part) => typeof part?.text === "string" ? part.text : "").filter(Boolean).join(" ")
    : "";
  const text = [item?.text, item?.summary, item?.command, item?.aggregatedOutput, item?.output, item?.diff, item?.path, contentText]
    .find((value) => typeof value === "string" && value.trim()) ?? null;
  const status = typeof item?.status === "string" ? item.status : item?.status?.type;
  return {
    id: bounded(item?.id ?? `${type}-${index}`, MAX_THREAD_TEXT),
    type,
    status: bounded(status ?? null, 40),
    text: bounded(text, MAX_THREAD_TEXT),
  };
}

function threadTurnSummary(turn, index) {
  const status = typeof turn?.status === "string" ? turn.status : turn?.status?.type;
  const items = Array.isArray(turn?.items) ? turn.items.slice(0, MAX_THREAD_DETAIL_ITEMS).map(threadItemSummary) : [];
  return {
    id: bounded(turn?.id ?? `turn-${index}`, MAX_THREAD_TEXT),
    status: bounded(status ?? null, 40),
    items,
  };
}

export class CodexAppServerAdapter {
  constructor({ apiKey = null, command = null, commandSource = null, configurationError = null, cwd = process.cwd(), env = process.env, model = "deepseek-flash", provider = "deepseek" }) {
    this.apiKey = apiKey;
    this.command = command;
    this.commandSource = commandSource;
    this.configurationError = configurationError;
    this.cwd = cwd;
    this.env = { ...env };
    this.model = model;
    this.provider = provider;
    this.process = null;
    this.reader = null;
    this.nextId = 1;
    this.pending = new Map();
    this.userInputs = new Map();
    this.threadId = null;
    this.turnId = null;
    this.executable = null;
    this.initializing = null;
    this.threadStarting = null;
    this.initialized = false;
    this.activeOperation = this.env.NOVEL_CODEX_OPERATION ?? null;
    this.turnWaiter = null;
    this.protocolFailure = false;
    this.callbacks = { onEvent: () => {}, onRawEvent: () => {}, onUserInputRequest: () => {} };
    this.eventSubscribers = new Set();
    this.executionLease = null;
    this.lastError = null;
  }

  get enabled() {
    return Boolean(this.command) && !this.configurationError;
  }

  get localEnabled() {
    return Boolean(this.command);
  }

  status() {
    return {
      configured: this.enabled,
      state: this.lastError || this.configurationError ? "error" : this.turnWaiter ? "running" : this.process ? "ready" : "idle",
      mode: this.enabled ? "novel-agent-engine" : this.localEnabled ? "local-history" : "local",
      error: this.lastError ?? this.configurationError ?? (this.enabled ? null : "未找到 Novel Agent Engine 内核"),
      provider: this.provider,
      model: this.model,
      commandSource: this.commandSource,
      cliPath: this.executable ?? null,
      isolatedHome: this.env.CODEX_HOME ?? null,
    };
  }

  configure({ apiKey = null, command, commandSource = null, configurationError = null, cwd = this.cwd, env = process.env, model = this.model, provider = this.provider }) {
    this.assertConfigurable();
    if (this.process) {
      this.reader?.close();
      this.process.kill();
    }
    this.apiKey = apiKey;
    this.command = command;
    this.commandSource = commandSource;
    this.configurationError = configurationError;
    this.cwd = cwd;
    this.env = { ...env };
    this.model = model;
    this.provider = provider;
    this.process = null;
    this.reader = null;
    this.pending.clear();
    this.userInputs.clear();
    this.executable = null;
    this.lastError = null;
    this.threadId = null;
    this.turnId = null;
    this.threadStarting = null;
    this.initialized = false;
    this.activeOperation = this.env.NOVEL_CODEX_OPERATION ?? null;
    this.protocolFailure = false;
  }

  assertConfigurable() {
    if (this.turnWaiter || this.turnId) throw new Error("Novel Agent Engine 正在处理上一轮请求，不能重新配置");
    if (this.initializing || this.threadStarting || this.pending.size > 0) throw new Error("Novel Agent Engine 正在处理请求，不能重新配置");
  }

  setCallbacks(callbacks) {
    this.callbacks = { ...this.callbacks, ...callbacks };
  }

  async ensureInitialized({ localOnly = false } = {}) {
    if (!this.localEnabled || (!localOnly && !this.enabled)) throw new Error(localOnly ? "Novel Agent Engine 本地历史不可用" : "Novel Agent Engine 未配置");
    if (this.initialized) return true;
    if (this.initializing) return this.initializing;
    this.initializing = this.initialize().finally(() => { this.initializing = null; });
    return this.initializing;
  }

  async initialize() {
    this.startProcess();
    await this.request("initialize", {
      clientInfo: { name: "novel_codex", title: "Novel Codex", version: "0.1.0" },
      capabilities: { experimentalApi: true },
    });
    this.notify("initialized", {});
    this.initialized = true;
    this.lastError = null;
    this.protocolFailure = false;
    return true;
  }

  async ensureSession() {
    await this.ensureInitialized();
    if (this.threadId) return this.threadId;
    if (this.threadStarting) return this.threadStarting;
    this.threadStarting = this.startThread().finally(() => { this.threadStarting = null; });
    return this.threadStarting;
  }

  async startThread() {
    const result = await this.request("thread/start", { model: this.model, cwd: this.cwd, serviceName: "novel_codex" });
    this.threadId = result?.thread?.id;
    if (!this.threadId) throw new Error("Novel Agent Engine 未返回 thread id");
    this.turnId = null;
    this.emit({ method: "agent/ready", params: { threadId: this.threadId } });
    return this.threadId;
  }

  detachThread() {
    if (this.turnWaiter) throw new Error("Novel Agent Engine 正在处理上一轮请求");
    this.threadId = null;
    this.turnId = null;
  }

  prepareTaskOperation(operation, taskCapability) {
    if (!new Set(["generateBrief", "generateOutline", "generatePlan", "generateChapter", "reviewPlanningSeat"]).has(operation)) throw new Error("生成任务 operation 无效");
    this.prepareOperationEnvironment(operation, taskCapability);
  }

  prepareAdvisorySession(taskCapability) {
    this.prepareOperationEnvironment("advisory", taskCapability);
  }

  prepareOperationEnvironment(operation, taskCapability) {
    if (typeof taskCapability !== "string" || taskCapability.length < 24 || /[\r\n]/.test(taskCapability)) throw new Error("任务能力凭证无效");
    if (this.turnWaiter) throw new Error("Novel Agent Engine 正在处理上一轮请求");
    if (this.process && operation === this.activeOperation && this.env.NOVEL_CODEX_TASK_CAPABILITY === taskCapability) return;
    if (this.process) {
      this.reader?.close();
      this.process.kill();
    }
    this.process = null;
    this.reader = null;
    this.pending.clear();
    this.userInputs.clear();
    this.threadId = null;
    this.turnId = null;
    this.threadStarting = null;
    this.initializing = null;
    this.initialized = false;
    this.lastError = null;
    this.protocolFailure = false;
    const environment = { ...this.env };
    if (operation) environment.NOVEL_CODEX_OPERATION = operation;
    else delete environment.NOVEL_CODEX_OPERATION;
    environment.NOVEL_CODEX_TASK_CAPABILITY = taskCapability;
    this.env = environment;
    this.activeOperation = operation;
  }

  async finishTaskOperation() {
    const error = new Error("创作引擎任务会话已结束");
    if (this.turnWaiter) {
      clearTimeout(this.turnWaiter.timer);
      this.turnWaiter.reject(error);
      this.turnWaiter = null;
    }
    const child = this.process;
    this.reader?.close();
    this.process = null;
    this.reader = null;
    for (const request of this.pending.values()) {
      clearTimeout(request.timer);
      request.reject(error);
    }
    this.pending.clear();
    this.userInputs.clear();
    this.threadId = null;
    this.turnId = null;
    this.threadStarting = null;
    this.initializing = null;
    this.initialized = false;
    this.lastError = null;
    this.activeOperation = null;
    this.protocolFailure = false;
    this.executionLease = null;
    const environment = { ...this.env };
    delete environment.NOVEL_CODEX_OPERATION;
    delete environment.NOVEL_CODEX_TASK_CAPABILITY;
    this.env = environment;
    if (child && child.exitCode === null) {
      if (!child.killed) child.kill();
      const stopped = await waitForChildClose(child, 1_500);
      if (!stopped && child.exitCode === null) {
        child.kill("SIGKILL");
        await waitForChildClose(child, 500);
      }
    }
  }

  async listThreads({ archived = false, cursor = null } = {}) {
    if (typeof archived !== "boolean") throw new Error("archived 必须是布尔值");
    if (cursor !== null && (typeof cursor !== "string" || cursor.length > MAX_THREAD_TEXT || /[\r\n]/.test(cursor))) throw new Error("任务列表 cursor 无效");
    await this.ensureInitialized({ localOnly: true });
    const result = await this.request("thread/list", {
      cwd: this.cwd,
      limit: MAX_THREAD_LIST,
      sortKey: "updated_at",
      sortDirection: "desc",
      sourceKinds: ["appServer"],
      archived,
      ...(cursor ? { cursor } : {}),
    });
    const data = Array.isArray(result?.data) ? result.data : [];
    const threads = data.slice(0, MAX_THREAD_LIST).map(threadSummary).filter((thread) => typeof thread.id === "string" && thread.id.length > 0);
    return { threads, nextCursor: typeof result?.nextCursor === "string" && result.nextCursor ? bounded(result.nextCursor, MAX_THREAD_TEXT) : null };
  }

  async readThread(threadId) {
    validateThreadId(threadId);
    await this.ensureInitialized({ localOnly: true });
    const result = await this.request("thread/read", { threadId, includeTurns: true });
    const thread = result?.thread;
    if (thread?.id !== threadId) throw new Error("Novel Agent Engine 未返回匹配的线程");
    const turns = Array.isArray(thread.turns) ? thread.turns.slice(0, MAX_THREAD_DETAIL_TURNS).map(threadTurnSummary) : [];
    return { thread: threadSummary(thread), turns };
  }

  async resumeThread(threadId) {
    validateThreadId(threadId);
    if (this.turnWaiter) throw new Error("Novel Agent Engine 正在处理上一轮请求");
    await this.ensureInitialized({ localOnly: true });
    const result = await this.request("thread/resume", { threadId });
    const thread = result?.thread;
    if (thread?.id !== threadId) throw new Error("Novel Agent Engine 未返回匹配的线程");
    this.threadId = thread.id;
    this.turnId = null;
    this.emit({ method: "thread/started", params: { thread } });
    return { thread: threadSummary(thread) };
  }

  async reconnectThread(threadId) {
    validateThreadId(threadId);
    if (this.turnWaiter) throw new Error("Novel Agent Engine 正在处理上一轮请求");
    this.lastError = null;
    const result = await this.resumeThread(threadId);
    this.emit({ method: "agent/connection", params: { state: "ready", threadId } });
    return result;
  }

  async reviewThread(threadId) {
    validateThreadId(threadId);
    if (this.turnWaiter) throw new Error("Novel Agent Engine 正在处理上一轮请求");
    await this.resumeThread(threadId);
    const result = await this.runManagedTurn(threadId, () => this.request("review/start", { threadId, delivery: "inline", target: { type: "uncommittedChanges" } }));
    this.emit({ method: "agent/review", params: { threadId, delivery: "inline", target: "uncommittedChanges" } });
    return { started: true, turn: result?.turn ?? null };
  }

  async compactThread(threadId) {
    validateThreadId(threadId);
    if (this.turnWaiter) throw new Error("Novel Agent Engine 正在处理上一轮请求");
    await this.resumeThread(threadId);
    const result = await this.runManagedTurn(threadId, () => this.request("thread/compact/start", { threadId }));
    return { started: true, turn: result?.turn ?? null };
  }

  async runManagedTurn(threadId, start) {
    const completion = new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.turnWaiter = null; reject(new Error("Novel Agent Engine 回合超时")); }, 10 * 60 * 1000);
      this.turnWaiter = { resolve, reject, timer, threadId, turnId: null, agentMessageItemId: null, agentMessage: "" };
    });
    void completion.catch(() => {});
    try {
      const started = await start();
      this.turnId = started?.turn?.id ?? this.turnId;
      if (this.turnWaiter) this.turnWaiter.turnId = this.turnId;
      return await completion;
    } catch (error) {
      if (this.turnWaiter) clearTimeout(this.turnWaiter.timer);
      this.turnWaiter = null;
      throw error;
    }
  }

  async listBackgroundTerminals(threadId) {
    validateThreadId(threadId);
    await this.ensureInitialized({ localOnly: true });
    const result = await this.request("thread/backgroundTerminals/list", { threadId });
    const terminals = Array.isArray(result?.data) ? result.data.slice(0, 12).map((terminal) => ({
      processId: typeof terminal?.processId === "string" && terminal.processId.trim() ? bounded(terminal.processId, MAX_THREAD_TEXT) : null,
      command: bounded(terminal?.command ?? null, MAX_THREAD_TEXT),
      cwd: bounded(terminal?.cwd ?? null, MAX_THREAD_TEXT),
      status: bounded(terminal?.status ?? null, 40),
    })).filter((terminal) => terminal.processId !== null) : [];
    return { terminals };
  }

  async terminateBackgroundTerminal(threadId, processId) {
    validateThreadId(threadId);
    validateProcessId(processId);
    await this.ensureInitialized({ localOnly: true });
    await this.request("thread/backgroundTerminals/terminate", { threadId, processId });
    return { terminated: true };
  }

  async setThreadPinned(threadId, isPinned) {
    validateThreadId(threadId);
    if (typeof isPinned !== "boolean") throw new Error("isPinned 必须是布尔值");
    await this.ensureInitialized({ localOnly: true });
    await this.request("thread/section/move", { threadId, sectionId: isPinned ? PINNED_THREAD_SECTION_ID : null });
    const detail = await this.readThread(threadId);
    return { thread: { ...detail.thread, isPinned } };
  }

  async archiveThread(threadId) {
    validateThreadId(threadId);
    if (this.turnWaiter) throw new Error("Novel Agent Engine 正在处理上一轮请求");
    await this.ensureInitialized({ localOnly: true });
    await this.request("thread/archive", { threadId });
    if (this.threadId === threadId) {
      this.threadId = null;
      this.turnId = null;
    }
    return { archived: true };
  }

  async unarchiveThread(threadId) {
    validateThreadId(threadId);
    await this.ensureInitialized({ localOnly: true });
    const result = await this.request("thread/unarchive", { threadId });
    if (result?.thread?.id !== threadId) throw new Error("Novel Agent Engine 未返回匹配的线程");
    return { thread: threadSummary(result.thread) };
  }

  startProcess() {
    if (this.process) return;
    const [executable, ...args] = Array.isArray(this.command) ? this.command : splitCommand(this.command ?? "");
    if (!executable) throw new Error("CODEX_APP_SERVER_COMMAND 为空");
    this.executable = executable;
    const child = spawn(executable, args, { cwd: this.cwd, env: this.env, stdio: ["pipe", "pipe", "pipe"] });
    this.process = child;
    this.reader = readline.createInterface({ input: child.stdout });
    this.reader.on("line", (line) => this.handleLine(line));
    child.stderr.on("data", (chunk) => this.emit({ method: "agent/stderr", params: { text: chunk.toString().slice(0, 4096) } }));
    child.on("error", (error) => { if (this.process === child) this.fail(error); });
    child.on("exit", (code, signal) => {
      if (this.process !== child) return;
      this.process = null;
      this.reader = null;
      this.threadId = null;
      this.turnId = null;
      this.initialized = false;
      delete this.env.NOVEL_CODEX_OPERATION;
      delete this.env.NOVEL_CODEX_TASK_CAPABILITY;
      this.activeOperation = null;
      this.fail(new Error(`Novel Agent Engine 内核已退出 (${code ?? signal ?? "unknown"})`));
    });
  }

  handleLine(line) {
    if (!line.trim()) return;
    let message;
    try { message = JSON.parse(line); } catch { this.emit({ method: "agent/text", params: { text: line.slice(0, MAX_EVENT_BYTES) } }); return; }
    if (this.protocolFailure) return;
    if (messageContainsModelProtocolLeak(message)) {
      this.protocolFailure = true;
      const error = new Error(MODEL_PROTOCOL_ERROR);
      this.fail(error);
      return;
    }
    this.callbacks.onRawEvent(message);
    const safe = safeMessage(message);
    if (message.method?.includes("requestApproval")) {
      if (!validRequestId(message.id)) {
        this.emit({ method: "agent/error", params: { message: "软件权限请求缺少有效 ID" } });
        return;
      }
      this.write({ id: message.id, result: { decision: "decline" } });
      this.emit({
        method: "agent/softwarePermissionDenied",
        params: {
          requestId: message.id,
          requestMethod: message.method,
          reason: "Novel Agent Engine 不允许模型请求命令、文件或网络权限",
        },
      });
      return;
    }
    this.callbacks.onEvent(safe);
    if (message.method === "thread/started" && message.params?.thread?.id) this.threadId = message.params.thread.id;
    if (message.method === "thread/closed" && message.params?.threadId === this.threadId) {
      this.threadId = null;
      this.turnId = null;
    }
    if (message.method === "turn/started" && message.params?.threadId === this.threadId && message.params?.turn?.id) {
      this.turnId = message.params.turn.id;
      if (this.turnWaiter && message.params?.threadId === this.turnWaiter.threadId && this.turnWaiter.turnId === null) {
        this.turnWaiter.turnId = message.params.turn.id;
      }
    }
    if (isUserInputMethod(message.method)) {
      if (!validRequestId(message.id)) {
        this.emit({ method: "agent/error", params: { message: "作者问题缺少有效 ID" } });
        return;
      }
      this.userInputs.set(message.id, message);
      this.callbacks.onUserInputRequest({
        requestId: message.id,
        method: message.method,
        params: message.params ?? {},
        threadId: typeof message.params?.threadId === "string" ? message.params.threadId : this.threadId,
        turnId: typeof message.params?.turnId === "string" ? message.params.turnId : this.turnId,
        itemId: typeof message.params?.itemId === "string" ? message.params.itemId : null,
      });
      return;
    }
    if (message.id !== undefined && this.pending.has(message.id)) {
      const request = this.pending.get(message.id);
      this.pending.delete(message.id);
      clearTimeout(request.timer);
      if (message.error) {
        request.reject(new Error(message.error.message ?? "Novel Agent Engine 请求失败"));
      } else {
        if (request.method === "turn/start" && this.turnWaiter && message.result?.turn?.id) {
          this.turnWaiter.turnId = message.result.turn.id;
        }
        request.resolve(message.result);
      }
    }
    const waiter = this.turnWaiter;
    const eventTurnId = message.params?.turnId ?? null;
    if (
      waiter
      && waiter.turnId !== null
      && message.params?.threadId === waiter.threadId
      && eventTurnId === waiter.turnId
      && message.method === "item/agentMessage/delta"
      && typeof message.params?.delta === "string"
    ) {
      const itemId = typeof message.params?.itemId === "string" ? message.params.itemId : null;
      if (waiter.agentMessageItemId !== itemId) {
        waiter.agentMessageItemId = itemId;
        waiter.agentMessage = "";
      }
      const remaining = MAX_AGENT_MESSAGE_TEXT - waiter.agentMessage.length;
      if (remaining > 0) waiter.agentMessage += message.params.delta.slice(0, remaining);
    }
    if (
      waiter
      && waiter.turnId !== null
      && message.params?.threadId === waiter.threadId
      && eventTurnId === waiter.turnId
      && message.method === "item/completed"
      && message.params?.item?.type === "agentMessage"
    ) {
      const text = agentMessageText(message.params.item);
      if (text !== null) {
        waiter.agentMessageItemId = typeof message.params.item.id === "string" ? message.params.item.id : null;
        waiter.agentMessage = text.slice(0, MAX_AGENT_MESSAGE_TEXT);
      }
    }
    if (message.method === "turn/completed") {
      if (message.params?.threadId === this.threadId && message.params?.turn?.id === this.turnId) this.turnId = null;
      const completedWaiter = this.turnWaiter;
      if (!completedWaiter) return;
      if (completedWaiter.turnId === null || message.params?.threadId !== completedWaiter.threadId || message.params?.turn?.id !== completedWaiter.turnId) return;
      this.turnWaiter = null;
      clearTimeout(completedWaiter.timer);
      if (message.params?.turn?.status === "failed") completedWaiter.reject(new Error(message.params?.turn?.error?.message ?? "Novel Agent Engine 回合失败"));
      else completedWaiter.resolve({ mode: "novel-agent-engine", message: completedWaiter.agentMessage, turn: message.params?.turn ?? null });
    }
  }

  request(method, params) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`Novel Agent Engine 请求超时：${method}`));
      }, 30_000);
      this.pending.set(id, { method, resolve, reject, timer });
      try {
        this.write({ id, method, params });
      } catch (error) {
        this.pending.delete(id);
        clearTimeout(timer);
        reject(error);
      }
    });
  }

  notify(method, params) { this.write({ method, params }); }

  write(message) {
    if (!this.process?.stdin.writable) throw new Error("Novel Agent Engine stdin 不可写");
    this.process.stdin.write(`${JSON.stringify(message)}\n`);
  }

  async sendTurn(input, { disableEnvironment = false, threadName = null } = {}) {
    if (this.turnWaiter) throw new Error("Novel Agent Engine 正在处理上一轮请求");
    const createdThread = !this.threadId;
    const threadId = await this.ensureSession();
    const result = new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.turnWaiter = null; reject(new Error("Novel Agent Engine 回合超时")); }, 10 * 60 * 1000);
      this.turnWaiter = { resolve, reject, timer, threadId, turnId: null, agentMessageItemId: null, agentMessage: "" };
    });
    void result.catch(() => {});
    try {
      const started = await this.request("turn/start", {
        threadId,
        input,
        ...(disableEnvironment ? { environments: [], approvalPolicy: "never" } : {}),
      });
      this.turnId = started?.turn?.id ?? this.turnId;
      if (this.turnWaiter) this.turnWaiter.turnId = this.turnId;
      if (createdThread) {
        const name = (threadName ?? input[0]?.text ?? "").replace(/\s+/g, " ").trim().slice(0, MAX_THREAD_TEXT);
        if (name) {
          await this.request("thread/name/set", { threadId, name });
        }
      }
      return await result;
    } catch (error) {
      if (this.turnWaiter) clearTimeout(this.turnWaiter.timer);
      this.turnWaiter = null;
      throw error;
    }
  }

  resumeSession(threadId) {
    return this.resumeThread(threadId);
  }

  async interruptTurn() {
    if (!this.turnWaiter || !this.threadId) return { interrupted: false };
    await this.request("turn/interrupt", { threadId: this.threadId, ...(this.turnId ? { turnId: this.turnId } : {}) });
    return { interrupted: true };
  }

  async steerTurn(input) {
    if (!this.turnWaiter || !this.threadId || !this.turnId) return { steered: false };
    await this.request("turn/steer", {
      threadId: this.threadId,
      input,
      expectedTurnId: this.turnId,
    });
    return { steered: true };
  }

  resolveUserInput(requestId, response) {
    const request = this.userInputs.get(requestId);
    if (!request || !isUserInputMethod(request.method)) throw new Error("作者问题已过期");
    const result = validateUserInputResponse(response, request.params);
    this.userInputs.delete(requestId);
    this.write({ id: requestId, result });
    return { resolved: true };
  }

  emit(message) {
    this.callbacks.onRawEvent(message);
    this.callbacks.onEvent(safeMessage(message));
    for (const subscriber of this.eventSubscribers) {
      try { subscriber(message); } catch { /* Isolate engine event consumers. */ }
    }
  }

  subscribeEvents(subscriber) {
    if (typeof subscriber !== "function") throw new Error("Agent event subscriber is invalid");
    this.eventSubscribers.add(subscriber);
    return () => this.eventSubscribers.delete(subscriber);
  }

  async bindExecution({ lease }) {
    if (this.executionLease) throw new Error("Novel Agent Engine already has an execution lease");
    this.executionLease = { ...lease };
  }

  async revokeExecution({ runId, leaseId, expectedEpoch, revokedEpoch }) {
    const lease = this.executionLease;
    if (!lease || lease.runId !== runId || lease.leaseId !== leaseId || lease.epoch !== expectedEpoch) throw new Error("Novel Agent Engine execution lease mismatch");
    this.executionLease = { ...lease, epoch: revokedEpoch, revoked: true };
    return { revokedEpoch };
  }

  async endExecution(lease) {
    if (this.executionLease?.runId === lease.runId && this.executionLease?.leaseId === lease.leaseId) this.executionLease = null;
  }

  fail(error) {
    this.lastError = error.message;
    for (const request of this.pending.values()) {
      clearTimeout(request.timer);
      request.reject(error);
    }
    this.pending.clear();
    this.userInputs.clear();
    if (this.turnWaiter) { clearTimeout(this.turnWaiter.timer); this.turnWaiter.reject(error); this.turnWaiter = null; }
    this.turnId = null;
    this.initialized = false;
    this.emit({ method: "agent/error", params: { message: error.message } });
  }

  close() {
    this.reader?.close();
    this.process?.kill();
    this.process = null;
    this.threadId = null;
    this.turnId = null;
    delete this.env.NOVEL_CODEX_OPERATION;
    delete this.env.NOVEL_CODEX_TASK_CAPABILITY;
    this.activeOperation = null;
    this.protocolFailure = false;
    this.executionLease = null;
    this.fail(new Error("Novel Agent Engine 已关闭"));
  }
}

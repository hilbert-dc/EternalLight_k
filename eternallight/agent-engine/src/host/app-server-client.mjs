import { nativeBinding } from "../native.mjs";

function cancelledError() {
  return Object.assign(new Error("Codex execution has been revoked"), { code: "TASK_CANCELLED" });
}

/** Minimal product-neutral JSON-RPC client for a Codex App Server process. */
export class CodexAppServerClient {
  constructor({ command, cwd = process.cwd(), env = process.env, model, requestTimeoutMs = 30_000, turnTimeoutMs = 10 * 60_000, serverRequestHandler = null, clientInfo = { name: "eternallight_agent_engine", title: "Eternal Light Agent Engine", version: "0.1.0" } }) {
    if (!Array.isArray(command) || command.length === 0 || command.some((part) => typeof part !== "string" || !part)) throw new Error("App Server command is required");
    if (typeof model !== "string" || !model) throw new Error("App Server model is required");
    this.command = [...command];
    this.cwd = cwd;
    this.env = { ...env };
    this.model = model;
    this.requestTimeoutMs = requestTimeoutMs;
    this.turnTimeoutMs = turnTimeoutMs;
    this.clientInfo = Object.freeze({ ...clientInfo });
    if (serverRequestHandler !== null && typeof serverRequestHandler !== "function") throw new Error("serverRequestHandler must be a function");
    this.serverRequestHandler = serverRequestHandler;
    this.process = null;
    this.processGeneration = 0;
    this.rpc = new (nativeBinding().NativeRpcPending)();
    this.pending = new Map();
    this.threadId = null;
    this.turnId = null;
    this.initializedGeneration = null;
    this.initializing = null;
    this.sessionCreation = null;
    this.operationReserved = false;
    this.executionRevoked = false;
    this.activeLease = null;
    this.activeTurn = null;
    this.callbacks = { onEvent: () => {}, onRawEvent: () => {} };
    this.eventSubscribers = new Set();
    this.closing = null;
  }

  setCallbacks(callbacks) {
    this.callbacks = { ...this.callbacks, ...callbacks };
  }

  subscribeEvents(subscriber) {
    if (typeof subscriber !== "function") throw new Error("event subscriber must be a function");
    this.eventSubscribers.add(subscriber);
    return () => this.eventSubscribers.delete(subscriber);
  }

  async ensureSession() {
    await this.#ensureInitialized();
    if (this.threadId) return this.threadId;
    if (!this.sessionCreation) {
      const generation = this.processGeneration;
      this.sessionCreation = (async () => {
        const result = await this.#request("thread/start", { model: this.model, cwd: this.cwd, serviceName: "eternallight_agent_engine" });
        if (generation !== this.processGeneration || !this.process) throw new Error("Codex App Server process changed while creating a session");
        const threadId = result?.thread?.id;
        if (typeof threadId !== "string" || !threadId) throw new Error("Codex App Server did not return a thread id");
        this.threadId = threadId;
        return threadId;
      })().finally(() => { this.sessionCreation = null; });
    }
    return this.sessionCreation;
  }

  async resumeSession(threadId) {
    if (this.operationReserved) throw new Error("cannot resume a thread during an active operation");
    if (typeof threadId !== "string" || !threadId) throw new Error("thread id is required");
    this.operationReserved = true;
    try {
      await this.#ensureInitialized();
      const generation = this.processGeneration;
      const result = await this.#request("thread/resume", { threadId });
      if (generation !== this.processGeneration || !this.process) throw new Error("Codex App Server process changed while resuming a session");
      this.threadId = result?.thread?.id ?? threadId;
      return this.threadId;
    } finally {
      this.operationReserved = false;
    }
  }

  async sendTurn(input, { disableEnvironment = false, threadName = null } = {}) {
    if (this.operationReserved) throw new Error("Codex App Server already has an active operation");
    if (!Array.isArray(input) || input.length === 0) throw new Error("Codex turn input is required");
    if (this.executionRevoked) throw cancelledError();

    this.operationReserved = true;
    const createdThread = !this.threadId;
    const context = this.#createTurnContext();
    this.activeTurn = context;
    try {
      const threadId = await this.ensureSession();
      context.native.bindThread(this.processGeneration, threadId);
      if (context.cancellationRequested || this.executionRevoked) {
        this.#finishTurn(context, { error: cancelledError() });
        return await context.completion;
      }
      const startPromise = this.#request("turn/start", {
        threadId: context.threadId,
        input,
        ...(disableEnvironment ? { environments: [], approvalPolicy: "never" } : {}),
      });
      void startPromise.then(async (started) => {
        await this.#bindStartedTurn(context, started?.turn?.id ?? null);
        if (createdThread && typeof threadName === "string" && threadName.trim() && !context.cancellationRequested) {
          await this.#request("thread/name/set", { threadId: context.threadId, name: threadName.replace(/\s+/g, " ").trim().slice(0, 240) });
        }
      }).catch((error) => this.#finishTurn(context, { error }));
      return await context.completion;
    } catch (error) {
      this.#finishTurn(context, { error });
      return await context.completion;
    }
  }

  async bindExecution({ lease }) {
    if (!lease?.runId || !lease?.leaseId || !Number.isInteger(lease.epoch)) throw new Error("App Server execution lease is invalid");
    if (this.activeTurn && !this.activeTurn.terminal) throw new Error("cannot replace execution during an active turn");
    this.activeLease = Object.freeze({ runId: lease.runId, leaseId: lease.leaseId, epoch: lease.epoch });
    this.executionRevoked = false;
  }

  async interruptTurn() {
    const context = this.activeTurn;
    if (!context || context.terminal) return false;
    context.native.cancel();
    if (!context.turnId) return true;
    await this.#interruptContext(context);
    return true;
  }

  async revokeExecution({ runId = this.activeLease?.runId, leaseId = this.activeLease?.leaseId, expectedEpoch = this.activeLease?.epoch, revokedEpoch = expectedEpoch + 1 } = {}) {
    if (!this.activeLease || this.activeLease.runId !== runId || this.activeLease.leaseId !== leaseId || this.activeLease.epoch !== expectedEpoch) throw new Error("App Server execution lease mismatch");
    this.executionRevoked = true;
    await this.interruptTurn();
    this.activeLease = Object.freeze({ ...this.activeLease, epoch: revokedEpoch });
    return Object.freeze({ revokedEpoch });
  }

  async close({ timeoutMs = 2_000 } = {}) {
    if (this.closing) return this.closing;
    const child = this.process;
    const error = new Error("Codex App Server client closed");
    this.closing = (async () => {
      this.executionRevoked = true;
      this.activeLease = null;
      this.sessionCreation = null;
      this.threadId = null;
      this.turnId = null;
      this.eventSubscribers.clear();
      this.#fail(error, false);
      if (child) await this.#retireProcess(child, { timeoutMs, error: null, emit: false });
      this.operationReserved = false;
    })().finally(() => { this.closing = null; });
    return this.closing;
  }

  #createTurnContext() {
    const context = {
      native: new (nativeBinding().NativeAppServerTurn)(),
      interruptPromise: null,
      deliverySettled: false,
      resolve: null,
      reject: null,
      timer: null,
      completion: null,
    };
    for (const name of ["generation", "threadId", "turnId", "cancellationRequested", "terminal"]) {
      Object.defineProperty(context, name, { get: () => JSON.parse(context.native.inspect())[name] });
    }
    context.completion = new Promise((resolve, reject) => {
      context.resolve = resolve;
      context.reject = reject;
    });
    void context.completion.catch(() => undefined);
    context.timer = setTimeout(() => {
      if (context.terminal) return;
      context.native.cancel();
      const error = Object.assign(new Error("Codex App Server turn timed out"), { code: "TURN_TIMEOUT" });
      this.#settleTurnDelivery(context, { error });
      if (context.turnId) void this.#interruptContext(context).catch(() => undefined);
    }, this.turnTimeoutMs);
    return context;
  }

  async #bindStartedTurn(context, turnId) {
    if (context.terminal || this.activeTurn !== context || context.generation !== this.processGeneration) return;
    const interrupt = context.native.bindStarted(typeof turnId === "string" ? turnId : null);
    this.turnId = context.turnId;
    if (interrupt) await this.#interruptContext(context);
  }

  #settleTurnDelivery(context, { error = null, result = null }) {
    if (context.deliverySettled) return;
    context.deliverySettled = true;
    if (error) context.reject(error);
    else context.resolve(result);
  }

  #finishTurn(context, { error = null, result = null } = {}) {
    if (context.terminal) return;
    context.native.finish();
    clearTimeout(context.timer);
    this.#settleTurnDelivery(context, { error, result });
    if (this.activeTurn === context) {
      this.activeTurn = null;
      this.operationReserved = false;
      this.turnId = null;
    }
  }

  #interruptContext(context) {
    if (context.interruptPromise) return context.interruptPromise;
    if (!context.turnId || !context.threadId || context.terminal) return Promise.resolve(false);
    context.interruptPromise = this.#request("turn/interrupt", { threadId: context.threadId, turnId: context.turnId })
      .then(() => true)
      .catch((error) => {
        context.interruptPromise = null;
        throw error;
      });
    return context.interruptPromise;
  }

  #start() {
    if (this.closing) throw new Error("Codex App Server client is closing");
    if (this.process) return this.process;
    const child = {
      native: new (nativeBinding().NativeProcessTransport)(JSON.stringify({ command: this.command, cwd: this.cwd, env: this.env })),
      timer: null,
      retirement: null,
    };
    const generation = ++this.processGeneration;
    this.process = child;
    child.timer = setInterval(() => {
      if (!this.#isCurrentProcess(child, generation)) return;
      try {
        for (const expired of JSON.parse(this.rpc.apply("expire", JSON.stringify({ now: Math.floor(performance.now()) })))) {
          this.pending.get(expired.id)?.reject(new Error(expired.error));
          this.pending.delete(expired.id);
        }
        for (const event of JSON.parse(child.native.drain())) {
          if (!this.#isCurrentProcess(child, generation)) break;
          if (event.kind === "message") this.#handleMessage(child, generation, event.value);
          else if (event.kind === "stderr") this.#emit({ method: "engine/stderr", params: { text: event.text } });
          else if (event.kind === "error") this.#protocolFailure(child, generation, new Error(event.message));
          else if (event.kind === "exit") {
            clearInterval(child.timer);
            this.process = null;
            this.initializedGeneration = null;
            this.threadId = null;
            this.turnId = null;
            this.#fail(new Error(`Codex App Server exited (${event.code ?? "signal"})`));
          }
        }
      } catch (error) { this.#protocolFailure(child, generation, error); }
    }, 4);
    return child;
  }

  async #ensureInitialized() {
    if (this.closing) throw new Error("Codex App Server client is closing");
    if (this.initializing) return this.initializing;
    if (this.process && this.initializedGeneration === this.processGeneration) return true;
    this.initializing = (async () => {
      const child = this.#start();
      const generation = this.processGeneration;
      try {
        await this.#request("initialize", { clientInfo: this.clientInfo, capabilities: { experimentalApi: true } });
        if (!this.#isCurrentProcess(child, generation)) throw new Error("Codex App Server process changed during initialization");
        this.#write({ method: "initialized", params: {} }, child, generation);
        this.initializedGeneration = generation;
        return true;
      } catch (error) {
        await this.#retireProcess(child, { timeoutMs: 1_000, error, emit: false });
        throw error;
      }
    })().finally(() => { this.initializing = null; });
    return this.initializing;
  }

  #request(method, params) {
    const child = this.process;
    const generation = this.processGeneration;
    if (!child || this.initializedGeneration !== generation && method !== "initialize") return Promise.reject(new Error("Codex App Server is not initialized"));
    const id = JSON.parse(this.rpc.apply("register", JSON.stringify({ method, generation, now: Math.floor(performance.now()), timeoutMs: this.requestTimeoutMs })));
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      try { this.#write({ id, method, params }, child, generation); }
      catch (error) {
        this.rpc.apply("cancel", JSON.stringify({ id }));
        this.pending.delete(id);
        reject(error);
      }
    });
  }

  #write(message, child = this.process, generation = this.processGeneration) {
    if (!this.#isCurrentProcess(child, generation) || child.retirement || this.closing) throw new Error("Codex App Server stdin is not writable");
    child.native.send(JSON.stringify(message));
  }

  #safeWrite(message, child, generation) {
    try {
      this.#write(message, child, generation);
      return true;
    } catch {
      return false;
    }
  }

  #handleMessage(child, generation, message) {
    if (!this.#isCurrentProcess(child, generation)) return;
    if (message.id !== undefined && typeof message.method === "string") {
      this.#notifyObservers(message);
      void this.#handleServerRequest(message, child, generation).catch(() => undefined);
      return;
    }
    const response = JSON.parse(this.rpc.apply("response", JSON.stringify({ generation, message })));
    if (response) {
      const pending = this.pending.get(response.id);
      this.pending.delete(response.id);
      if (response.error) pending?.reject(new Error(response.error));
      else pending?.resolve(response.result);
    }
    const context = this.activeTurn;
    if (context) {
      const observation = JSON.parse(context.native.observe(generation, JSON.stringify(message)));
      this.turnId = context.turnId;
      if (observation.interrupt) void this.#interruptContext(context).catch(() => undefined);
      if (observation.completion) {
        const completion = observation.completion;
        this.#finishTurn(context, completion.error ? { error: new Error(completion.error) } : { result: completion.result });
      }
    }
    this.#notifyObservers(message);
  }

  async #handleServerRequest(message, child, generation) {
    if (!this.serverRequestHandler) {
      this.#safeWrite({ id: message.id, error: { code: -32601, message: "Host request is not supported" } }, child, generation);
      return;
    }
    try {
      const result = await this.serverRequestHandler({ method: message.method, params: message.params ?? {}, requestId: message.id });
      this.#safeWrite({ id: message.id, result: result ?? {} }, child, generation);
    } catch (error) {
      this.#safeWrite({ id: message.id, error: { code: Number.isInteger(error?.code) ? error.code : -32000, message: String(error?.message ?? error).slice(0, 500) } }, child, generation);
    }
  }

  #notifyObservers(message) {
    try { this.callbacks.onRawEvent(message); } catch { /* Observers cannot break the protocol loop. */ }
    this.#emit(message);
  }

  #emit(event) {
    try { this.callbacks.onEvent(event); } catch { /* Observers cannot break the protocol loop. */ }
    for (const subscriber of this.eventSubscribers) {
      try { subscriber(event); } catch { /* Isolate subscribers from one another. */ }
    }
  }

  #protocolFailure(child, generation, error) {
    if (!this.#isCurrentProcess(child, generation)) return;
    this.#fail(error);
    void this.#retireProcess(child, { timeoutMs: 1_000, error: null, emit: false });
  }

  #fail(error, emit = true) {
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
    this.rpc.apply("clear", "{}");
    if (this.activeTurn) this.#finishTurn(this.activeTurn, { error });
    this.turnId = null;
    if (emit) this.#emit({ method: "engine/error", params: { message: error.message } });
  }

  async #retireProcess(child, { timeoutMs, error = null, emit = false }) {
    if (!child) return;
    if (child.retirement) return child.retirement;
    clearInterval(child.timer);
    if (this.process === child) {
      this.process = null;
      this.initializedGeneration = null;
      this.threadId = null;
      this.turnId = null;
    }
    if (error) this.#fail(error, emit);
    child.retirement = child.native.close(timeoutMs);
    await child.retirement;
  }

  #isCurrentProcess(child, generation) {
    return this.process === child && this.processGeneration === generation;
  }
}

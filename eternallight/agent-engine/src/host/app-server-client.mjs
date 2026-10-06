import { nativeBinding } from "../native.mjs";

/** Promise/callback binding for the native App Server lifecycle owner. */
export class CodexAppServerClient {
  constructor({ command, cwd = process.cwd(), env = process.env, model, requestTimeoutMs = 30_000, turnTimeoutMs = null, serverRequestHandler = null, clientInfo = { name: "eternallight_agent_engine", title: "Eternal Light Agent Engine", version: "0.1.0" } }) {
    if (!Array.isArray(command) || !command.length || command.some(part => typeof part !== "string" || !part)) throw new Error("App Server command is required");
    if (serverRequestHandler !== null && typeof serverRequestHandler !== "function") throw new Error("serverRequestHandler must be a function");
    this.transportOptions = { command: [...command], cwd, env: { ...env } };
    this.native = new (nativeBinding().NativeAppClient)(JSON.stringify({ model, cwd, requestTimeoutMs, turnTimeoutMs, clientInfo }));
    this.serverRequestHandler = serverRequestHandler;
    this.callbacks = { onEvent() {}, onRawEvent() {} };
    this.subscribers = new Set();
    this.pending = new Map();
    this.transports = new Map();
    this.nextId = 0;
    this.timer = null;
  }

  get threadId() { return this.#inspect().threadId; }
  get turnId() { return this.#inspect().turnId; }
  setCallbacks(callbacks) { this.callbacks = { ...this.callbacks, ...callbacks }; }
  subscribeEvents(subscriber) {
    if (typeof subscriber !== "function") throw new Error("event subscriber must be a function");
    this.subscribers.add(subscriber);
    return () => this.subscribers.delete(subscriber);
  }
  ensureSession() { return this.#command("ensureSession"); }
  resumeSession(threadId) { return this.#command("resumeSession", { threadId }); }
  sendTurn(input, options = {}) { return this.#command("sendTurn", { input, options }); }
  bindExecution(params) { return this.#command("bindExecution", params); }
  interruptTurn() { return this.#command("interruptTurn"); }
  revokeExecution(params = {}) { return this.#command("revokeExecution", params); }
  close(params = {}) {
    this.subscribers.clear();
    return this.#command("close", params);
  }

  #inspect() { return JSON.parse(this.native.apply("inspect", "{}")); }
  #command(method, params = {}) {
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      try { this.#apply("command", { id, method, params }); }
      catch (error) { this.pending.delete(id); reject(error); }
    });
  }
  #apply(method, input) {
    const effects = JSON.parse(this.native.apply(method, JSON.stringify({ ...input, now: Math.floor(performance.now()) })));
    for (const effect of effects) this.#effect(effect);
    if (!this.transports.size && !this.pending.size && this.timer) { clearInterval(this.timer); this.timer = null; }
  }
  #effect(effect) {
    const generation = effect.generation;
    switch (effect.kind) {
      case "resolve": case "reject": {
        const pending = this.pending.get(effect.id);
        this.pending.delete(effect.id);
        if (effect.kind === "resolve") pending?.resolve(effect.result);
        else pending?.reject(Object.assign(new Error(effect.error), { code: effect.code }));
        break;
      }
      case "spawn": {
        try {
          const native = new (nativeBinding().NativeProcessTransport)(JSON.stringify(this.transportOptions));
          this.transports.set(generation, { native, retirement: null });
          this.timer ??= setInterval(() => {
            this.#apply("tick", {});
            for (const [generation, transport] of this.transports) {
              if (transport.retirement) continue;
              try {
                for (const event of JSON.parse(transport.native.drain())) this.#apply("transport", { generation, event });
              } catch (error) { this.#apply("transport", { generation, event: { kind: "error", message: error.message } }); }
            }
          }, 4);
        } catch (error) { this.#apply("transport", { generation, event: { kind: "error", message: error.message } }); }
        break;
      }
      case "write": {
        const transport = this.transports.get(generation);
        if (!transport || transport.retirement) break;
        try { transport.native.send(JSON.stringify(effect.message)); }
        catch (error) { this.#apply("transport", { generation, event: { kind: "error", message: error.message } }); }
        break;
      }
      case "retire": {
        const transport = this.transports.get(generation);
        if (!transport) { this.#apply("retired", { generation }); break; }
        transport.retirement ??= transport.native.close(effect.timeoutMs);
        void transport.retirement.then(() => {
          this.transports.delete(generation);
          this.#apply("retired", { generation });
        }, error => {
          // Failed reaping must not be reported as a successful close.
          for (const pending of this.pending.values()) pending.reject(error);
          this.pending.clear();
        });
        break;
      }
      case "event": {
        for (const callback of [this.callbacks.onRawEvent, this.callbacks.onEvent, ...this.subscribers]) {
          try { callback(effect.message); } catch { /* Non-authoritative observers. */ }
        }
        break;
      }
      case "hostRequest": {
        const message = effect.message;
        const reply = response => this.#apply("hostResponse", { generation, message: { id: message.id, ...response } });
        if (!this.serverRequestHandler) { reply({ error: { code: -32601, message: "Host request is not supported" } }); break; }
        void Promise.resolve().then(() => this.serverRequestHandler({ method: message.method, params: message.params ?? {}, requestId: message.id }))
          .then(result => reply({ result: result ?? {} }), error => reply({ error: { code: Number.isInteger(error?.code) ? error.code : -32000, message: String(error?.message ?? error).slice(0, 500) } }));
        break;
      }
      default: throw new Error(`Unknown native App Server effect: ${effect.kind}`);
    }
  }
}

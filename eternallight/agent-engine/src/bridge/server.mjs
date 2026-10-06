import { randomBytes } from "node:crypto";
import { nativeBinding } from "../native.mjs";
import { nativeFetch } from "../http-transport.mjs";
import { assertTrustedSessionPlan } from "../session/resolver.mjs";

/** Host callbacks and injectable transport binding; the bridge runs in Rust. */
export class ModelBridge {
  constructor({ sessionPlan, connections, credentialResolver, fetchImpl = nativeFetch, maxBodyBytes = 2 * 1024 * 1024, maxDownstreamBytes = 32 * 1024 * 1024, maxConcurrentRequests = 128, requestTimeoutMs = 120_000, token = randomBytes(32).toString("base64url"), port = 0, budget = null, requestObserver = null, responseObserver = null, qualificationObserver = null, toolPolicy = null, executorValidatedTools = null, responseBarrier = null }) {
    this.options = { sessionPlan, connections, maxBodyBytes, maxDownstreamBytes, maxConcurrentRequests, requestTimeoutMs, token, port, toolPolicy, executorValidatedTools, responseBarrier, customFetch: fetchImpl !== nativeFetch };
    this.token = token;
    this.credentialResolver = credentialResolver;
    this.fetchImpl = fetchImpl;
    this.observers = { request: requestObserver, response: responseObserver, qualification: qualificationObserver };
    this.budgets = new Map();
    this.budget = null;
    this.fetches = new Map();
    this.native = null;
    this.timer = null;
    this.baseUrl = null;
    this.server = null;
    this.closing = null;
    if (budget) this.beginTaskBudget(budget);
  }
  get controllers() { return { size: this.native ? this.#apply("inspect").active : 0 }; }
  #apply(method, input = {}) { return JSON.parse(this.native.apply(method, JSON.stringify(input))); }
  #ensureNative() {
    if (this.native) return;
    this.native = new (nativeBinding().NativeBridgeService)(JSON.stringify(this.options));
    this.baseUrl = this.#apply("inspect").baseUrl;
    this.server = { listening: true };
    this.timer = setInterval(() => {
      const native = this.native;
      const reply = payload => native.apply("reply", JSON.stringify(payload));
      for (const effect of this.#apply("poll")) {
        void this.#callback(effect.method, effect.params).then(
          result => reply({ id: effect.id, result: result ?? null }),
          error => reply({ id: effect.id, error: String(error?.message ?? error) }),
        );
      }
    }, 2);
  }
  async start() { this.#ensureNative(); return { baseUrl: this.baseUrl, token: this.token }; }
  beginTaskBudget(budget) {
    if (!budget || typeof budget.reserveRequest !== "function" || typeof budget.observeUsage !== "function") throw new Error("invalid task budget");
    this.#ensureNative();
    const remaining = budget.remainingDurationMs();
    this.#apply("budget:start", { id: budget.id, deadline: remaining === null ? null : Date.now() + remaining });
    this.budgets.set(budget.id, budget); this.budget = budget;
    return budget;
  }
  endTaskBudget(budget) {
    if (!this.native || this.budget !== budget) return false;
    const ended = this.#apply("budget:end", { id: budget.id });
    if (ended) { this.budget = null; if (!this.controllers.size) this.budgets.delete(budget.id); }
    return ended;
  }
  beginExecutionLease(lease) { this.#ensureNative(); this.#apply("lease:start", lease); return lease; }
  renewExecutionLease(params) {
    assertTrustedSessionPlan(params.sessionPlan);
    if (!this.native) throw new Error("bridge has no active execution");
    return this.#apply("lease:renew", params);
  }
  async endExecutionLease(lease) {
    if (!this.native || !this.#apply("lease:matches", lease ?? {})) return false;
    await this.cancelActiveRequests(); this.#apply("lease:clear"); return true;
  }
  async cancelActiveRequests() {
    if (!this.native) return 0;
    const count = this.#apply("cancel");
    for (const fetch of this.fetches.values()) fetch.controller.abort();
    await this.native.drain();
    for (const id of this.budgets.keys()) if (id !== this.budget?.id) this.budgets.delete(id);
    return count;
  }
  async close() {
    if (this.closing) return this.closing;
    if (!this.native) return;
    this.closing = (async () => {
      for (const fetch of this.fetches.values()) fetch.controller.abort();
      try { await this.native.close(); }
      finally {
        clearInterval(this.timer); this.timer = null; this.native = null;
        this.server = null; this.baseUrl = null; this.budget = null; this.budgets.clear();
      }
    })().finally(() => { this.closing = null; });
    return this.closing;
  }
  async #callback(method, params) {
    switch (method) {
      case "credential": return this.credentialResolver(params);
      case "reserve": return this.budgets.get(params.budgetId).reserveRequest(params);
      case "settle": return this.budgets.get(params.budgetId).settleReservation(params.id, params.usage, params.options);
      case "request": case "response": case "qualification": return this.observers[method]?.(params);
      case "fetch": {
        const controller = new AbortController();
        const handle = { controller, reader: null };
        this.fetches.set(params.requestId, handle);
        const response = await this.fetchImpl(params.url, { method: "POST", headers: params.headers, body: params.body, signal: controller.signal, maxBytes: params.maxBytes, timeoutMs: params.timeoutMs });
        handle.reader = response.body?.[Symbol.asyncIterator]();
        return { status: response.status, headers: Object.fromEntries(response.headers) };
      }
      case "fetchRead": {
        const result = await this.fetches.get(params.requestId)?.reader?.next();
        return result && !result.done ? { done: false, bytes: [...result.value] } : { done: true };
      }
      case "releaseFetch": {
        const handle = this.fetches.get(params.requestId);
        this.fetches.delete(params.requestId);
        handle?.controller.abort();
        if (handle?.reader?.return) void handle.reader.return().catch(() => {});
        return null;
      }
      default: throw new Error(`Unknown native bridge callback: ${method}`);
    }
  }
}

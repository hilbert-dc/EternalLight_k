import { nativeBinding } from "../native.mjs";
import { existsSync } from "node:fs";
import { sep } from "node:path";
import { fileURLToPath } from "node:url";
import { assertTrustedSessionPlan } from "../session/resolver.mjs";

function bridgeWorkerPath() {
  const source = fileURLToPath(new URL("./worker.mjs", import.meta.url));
  const unpacked = source.replace(`${sep}app.asar${sep}`, `${sep}app.asar.unpacked${sep}`);
  return unpacked !== source && existsSync(unpacked) ? unpacked : source;
}

/** Promise and host-object adapter for the native isolated bridge supervisor. */
export class ModelBridgeProcess {
  constructor({ sessionPlan, connections, credentialResolver, qualificationObserver = null, requestConfigurationProvider = null, requestObserver = null, responseObserver = null, token = null, toolPolicy = null, executorValidatedTools = null, responseBarrier = null, heartbeatIntervalMs = 5_000, heartbeatTimeoutMs = 2_000, lifecycleTimeoutMs = 10_000, workerPath = bridgeWorkerPath() }) {
    this.native = new (nativeBinding().NativeBridgeSupervisor)(JSON.stringify({ sessionPlan, connections, hasRequestConfigurationProvider: typeof requestConfigurationProvider === "function", token, toolPolicy, executorValidatedTools, responseBarrier, heartbeatIntervalMs, heartbeatTimeoutMs, lifecycleTimeoutMs }));
    this.credentialResolver = credentialResolver;
    this.requestConfigurationProvider = requestConfigurationProvider;
    this.observers = { qualification: qualificationObserver, request: requestObserver, response: responseObserver };
    const allowed = ["PATH", "SystemRoot", "WINDIR", "TMP", "TEMP", "TMPDIR", "SSL_CERT_FILE", "NODE_EXTRA_CA_CERTS"];
    const env = Object.fromEntries(allowed.flatMap(key => typeof process.env[key] === "string" ? [[key, process.env[key]]] : []));
    this.processOptions = { command: [process.execPath, workerPath], cwd: process.cwd(), env: { ...env, ELECTRON_RUN_AS_NODE: "1" } };
    this.transports = new Map(); this.pending = new Map(); this.budgets = new Map(); this.nextId = 0; this.timer = null;
    this.budgetRecovery = null;
  }
  #inspect() { return JSON.parse(this.native.apply("inspect", "{}")); }
  get processId() { return this.#inspect().processId; }
  get generation() { return this.#inspect().generation; }
  get baseUrl() { return this.#inspect().baseUrl; }
  get token() { return this.#inspect().token; }
  get budgetRecoveryError() { const value = this.#inspect().budgetRecoveryError; return value ? new Error(value) : null; }
  start() {
    if (!this.#inspect().acceptingStart) return this.#command("start");
    if (this.starting) return this.starting;
    const promise = this.#command("start"); this.starting = promise;
    void promise.then(() => { this.starting = null; }, () => { this.starting = null; });
    return promise;
  }
  async close() { try { return await this.#command("close"); } finally { if (!this.transports.size) this.budgets.clear(); } }
  cancelActiveRequests() { return this.#command("cancel"); }
  async beginTaskBudget(budget) {
    if (!budget || typeof budget.snapshot !== "function") throw new Error("invalid task budget");
    this.budgets.set(budget.id, budget);
    await this.#command("budget:start", { ...budget.snapshot(), startedAt: budget.startedAt }); return budget;
  }
  beginExecutionLease(lease) { return this.#command("lease:start", lease); }
  renewExecutionLease(params) { assertTrustedSessionPlan(params.sessionPlan); return this.#command("lease:renew", params); }
  endExecutionLease(lease) { return this.#command("lease:end", lease ?? {}); }
  async endTaskBudget(budget) { const result = await this.#command("budget:end", { id: budget?.id }); this.budgets.delete(budget?.id); return result; }
  #command(method, params = {}) {
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      try { this.#apply("command", { id, method, params }); }
      catch (error) { this.pending.delete(id); reject(error); }
    });
  }
  #apply(method, input) {
    for (const effect of JSON.parse(this.native.apply(method, JSON.stringify({ ...input, now: Math.floor(performance.now()) })))) this.#effect(effect);
    if (!this.transports.size && !this.pending.size && this.timer) { clearInterval(this.timer); this.timer = null; }
  }
  #effect(effect) {
    const generation = effect.generation;
    switch (effect.kind) {
      case "resolve": case "reject": {
        const pending = this.pending.get(effect.id); this.pending.delete(effect.id);
        if (effect.kind === "resolve") pending?.resolve(effect.result); else pending?.reject(new Error(effect.error));
        break;
      }
      case "spawn": {
        try {
          const native = new (nativeBinding().NativeProcessTransport)(JSON.stringify(this.processOptions));
          this.transports.set(generation, { native, retirement: null });
          this.#apply("spawned", { generation, pid: native.pid });
          this.timer ??= setInterval(() => {
            this.#apply("tick", {});
            for (const [generation, transport] of this.transports) {
              if (transport.retirement) continue;
              try { for (const event of JSON.parse(transport.native.drain())) this.#apply("transport", { generation, event }); }
              catch (error) { this.#apply("transport", { generation, event: { kind: "error", message: error.message } }); }
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
      case "callback": {
        const promise = Promise.resolve().then(() => this.#callback(effect));
        if (effect.method === "recoverBudget") this.budgetRecovery = promise.catch(() => {});
        void promise.then(result => this.#apply("callback", { id: effect.id, result: result ?? null }), error => this.#apply("callback", { id: effect.id, error: String(error?.message ?? error) }));
        break;
      }
      default: throw new Error(`Unknown native bridge supervisor effect: ${effect.kind}`);
    }
  }
  async #callback({ method, params, generation }) {
    switch (method) {
      case "requestConfiguration": return this.requestConfigurationProvider(params);
      case "credentials": return Object.fromEntries(await Promise.all(params.map(async ref => [ref, await this.credentialResolver(ref)])));
      case "retire": {
        const transport = this.transports.get(generation);
        if (transport) { transport.retirement ??= transport.native.close(0); await transport.retirement; this.transports.delete(generation); }
        return null;
      }
      case "recoverBudget": return this.budgets.get(params.budgetId).markOutstandingUnknown();
      case "completeBudget": return this.budgets.get(params.budgetId).complete?.(params.outcome);
      case "budget:reserve": return this.budgets.get(params.budgetId).reserveRequest(params);
      case "budget:settle": return this.budgets.get(params.budgetId).settleReservation(params.id, params.usage, params.options);
      case "budget:observe": return this.budgets.get(params.budgetId).observeUsage(params.usage);
      case "response": {
        if (params?.status === "failed") for (const budget of this.budgets.values()) {
          if (budget.runId === params.runId) budget.observeFailure?.(params);
        }
        return this.observers.response?.(params) ?? { recorded: false };
      }
      case "request": case "qualification": return this.observers[method]?.(params) ?? { recorded: false };
      default: throw new Error(`Unknown native bridge supervisor callback: ${method}`);
    }
  }
}

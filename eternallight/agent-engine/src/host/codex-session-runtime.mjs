import { nativeBinding } from "../native.mjs";
import { assertTrustedSessionPlan } from "../session/resolver.mjs";

function requireMethod(value, method, label) {
  if (typeof value?.[method] !== "function") throw new Error(`${label} is missing ${method}`);
}
function runtimeError(message) {
  if (message.startsWith("TASK_CANCELLED:")) return Object.assign(new Error(message.slice(15)), { code: "TASK_CANCELLED" });
  return new Error(message);
}

/** Embedding-owned handles and Promise delivery for the native runtime coordinator. */
export class CodexSessionRuntime {
  #native = new (nativeBinding().NativeSessionRuntime)();
  #pending = new Map();
  #next = 0;
  #budget = null;

  constructor({ appServer, bridge, budgetFactory, bridgeOwnership = "owned", closeAppServer = (server) => server.close(), reconcile = async () => ({ outcome: "cancelled", unresolvedOperations: [] }) }) {
    for (const method of ["ensureSession", "resumeSession", "bindExecution", "sendTurn", "revokeExecution", "interruptTurn", "close", "subscribeEvents"]) requireMethod(appServer, method, "Codex App Server");
    for (const method of ["start", "beginTaskBudget", "beginExecutionLease", "cancelActiveRequests", "endExecutionLease", "endTaskBudget", "close"]) requireMethod(bridge, method, "model bridge");
    if (typeof budgetFactory !== "function") throw new Error("Codex runtime requires a budget factory");
    if (typeof closeAppServer !== "function") throw new Error("Codex runtime App Server closer must be a function");
    if (!["owned", "borrowed"].includes(bridgeOwnership)) throw new Error("Codex runtime bridge ownership is invalid");
    if (typeof reconcile !== "function") throw new Error("Codex runtime reconcile adapter must be a function");
    Object.assign(this, { appServer, bridge, budgetFactory, bridgeOwnership, closeAppServer, reconcile });
  }
  #read(method, params = {}) {
    try { return JSON.parse(this.#native.apply(method, JSON.stringify(params))); }
    catch (error) { throw runtimeError(error.message); }
  }
  get closed() { return this.#read("snapshot").closed; }
  subscribeEvents(subscriber) { return this.appServer.subscribeEvents(subscriber); }
  ensureSession() { this.#read("open"); return this.appServer.ensureSession(); }
  resumeSession(threadId) { this.#read("open"); return this.appServer.resumeSession(threadId); }
  bindExecution(params) { return this.#command("bind", params); }
  renewExecution(params) {
    assertTrustedSessionPlan(params.sessionPlan);
    requireMethod(this.bridge, "renewExecutionLease", "model bridge");
    return this.#command("renew", params, { sessionPlan: params.sessionPlan });
  }
  sendTurn(input, options) { return this.#command("turn", { input }, options); }
  revokeExecution(request) { return this.#command("revoke", request); }
  interruptTurn(request) { return this.appServer.interruptTurn(request); }
  reconcileRun(context) { return this.reconcile(context); }
  close() { return this.closing ??= this.#command("close", { ownership: this.bridgeOwnership }); }
  #command(method, params, options) {
    const id = ++this.#next;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject, options });
      try { this.#apply("command", { id, method, params }); }
      catch (error) { this.#pending.delete(id); reject(error); }
    });
  }
  #apply(method, input) {
    for (const effect of this.#read(method, input)) {
      if (effect.kind === "reply") {
        const pending = this.#pending.get(effect.id); this.#pending.delete(effect.id);
        if (effect.error) pending?.reject(runtimeError(effect.error)); else pending?.resolve(effect.result);
      } else if (effect.kind === "releaseBudget") this.#budget = null;
      else if (effect.kind === "callback") {
        // Invoke immediately, while translating both synchronous and asynchronous errors.
        let result;
        try { result = this.#callback(effect); } catch (error) { result = Promise.reject(error); }
        void Promise.resolve(result).then(
          result => this.#apply("callback", { id: effect.id, result: result ?? null }),
          error => this.#apply("callback", { id: effect.id, error: error?.code === "TASK_CANCELLED" ? `TASK_CANCELLED:${error.message}` : String(error?.message ?? error) }),
        );
      } else throw new Error(`Unknown native runtime effect: ${effect.kind}`);
    }
  }
  #callback({ method, params }) {
    switch (method) {
      case "budget": return Promise.resolve(this.budgetFactory(params)).then(budget => {
        if (!budget) throw new Error("Codex runtime budget factory returned no budget");
        this.#budget = budget; return null;
      });
      case "bridge": return Promise.resolve(this.bridge.start()).then(() => null);
      case "budgetStarted": return Promise.resolve(this.bridge.beginTaskBudget(this.#budget)).then(() => null);
      case "leaseStarted": return Promise.resolve(this.bridge.beginExecutionLease(params.lease)).then(() => null);
      case "appBound": return Promise.resolve(this.appServer.bindExecution(params)).then(() => null);
      case "leaseRenewed": return this.bridge.renewExecutionLease({ lease: params.lease, sessionPlan: this.#pending.get(params.commandId)?.options.sessionPlan });
      case "appRenewed": return this.appServer.renewExecution?.({ lease: params.lease, sessionPlan: this.#pending.get(params.commandId)?.options.sessionPlan }) ?? params.lease;
      case "sendTurn": {
        const options = this.#pending.get(params.commandId)?.options;
        const dispatch = options?.dispatch;
        if (dispatch !== undefined && typeof dispatch !== "function") throw new Error("Codex runtime dispatch override must be a function");
        return dispatch ? dispatch(this.appServer) : this.appServer.sendTurn(params.input, options);
      }
      case "cancel": return this.bridge.cancelActiveRequests();
      case "revokeApp": return this.appServer.revokeExecution(params);
      case "interrupt": return this.appServer.interruptTurn();
      case "end:lease": return this.bridge.endExecutionLease(params);
      case "end:budget": return Promise.resolve(this.bridge.endTaskBudget(this.#budget)).then(result => {
        if (result !== true) throw new Error("model bridge did not release the task budget");
        this.#budget?.complete?.(params.budgetStatus);
        return result;
      });
      case "end:budgetRecord": return this.#budget?.complete?.(params.budgetStatus);
      case "end:app": return this.appServer.endExecution?.(params);
      case "closeApp": return this.closeAppServer(this.appServer);
      case "closeBridge": return this.bridge.close();
      default: throw new Error(`Unknown native runtime callback: ${method}`);
    }
  }
}

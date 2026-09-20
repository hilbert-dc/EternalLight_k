import { nativeBinding } from "../native.mjs";

export class RecoveryIndex {
  #native;
  #pending = Promise.resolve();
  constructor(path) { this.#native = new (nativeBinding().NativeRecoveryIndex)(path); }
  #apply(method, input = {}) {
    const result = this.#pending.then(async () => JSON.parse(await this.#native.apply(method, JSON.stringify(input))));
    this.#pending = result.catch(() => undefined);
    return result;
  }
  read() { return this.#apply("read"); }
  beginRun({ createdAt = new Date().toISOString(), ...run }) { return this.#apply("begin", { ...run, createdAt }); }
  recordRequest(runId, { state = "prepared", kind = "model", updatedAt = new Date().toISOString(), ...request }) {
    return this.#apply("record", { ...request, runId, state, kind, updatedAt });
  }
  transitionRequest(runId, requestId, state, { receiptRef = null, updatedAt = new Date().toISOString() } = {}) {
    return this.#apply("transition", { runId, requestId, state, receiptRef, updatedAt });
  }
  markRunTerminal(runId, terminalAt = new Date().toISOString()) { return this.#apply("terminal", { runId, terminalAt }); }
  resolveRun(runId, resolution, terminalAt = new Date().toISOString()) { return this.#apply("resolve", { runId, resolution, terminalAt }); }
  reconcileRun(runId, runtimeStatus, reconciledAt = new Date().toISOString(), { fence = null } = {}) {
    return this.#apply("reconcile", { runId, runtimeStatus, reconciledAt, fence });
  }
  markReconciliationRequired(runIds, reason, updatedAt = new Date().toISOString()) { return this.#apply("required", { runIds, reason: String(reason), updatedAt }); }
  confirmRuntimeReceipt(runId, requestId, receiptRef, updatedAt = new Date().toISOString()) {
    return this.settleRequest(runId, requestId, { outcome: "committed", receiptRef }, updatedAt);
  }
  settleRequest(runId, requestId, { outcome, receiptRef = null }, updatedAt = new Date().toISOString()) {
    return this.#apply("settle", { runId, requestId, outcome, receiptRef, updatedAt });
  }
  observeRuntimeOperations(runId, operations, updatedAt = new Date().toISOString()) { return this.#apply("observe", { runId, operations, updatedAt }); }
  reconciliationCandidates(runId = null) { return this.#apply("candidates", { runId }); }
  unfinishedRuns() { return this.#apply("unfinished"); }
  recoveryReport() { return this.#apply("report"); }
}

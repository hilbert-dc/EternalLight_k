import { nativeBinding } from "../native.mjs";

/**
 * Coordinates unresolved operation reconciliation without knowing domain tool
 * names, payloads, or ledger protocols. The injected operation contract owns
 * all domain queries; the generic index only persists compact references and
 * outcomes.
 */
export class RecoveryCoordinator {
  constructor({ index, operationContract }) {
    if (!index || typeof index.reconciliationCandidates !== "function" || typeof index.settleRequest !== "function") {
      throw new Error("recovery coordinator requires a recovery index");
    }
    if (!operationContract || typeof operationContract.reconcile !== "function") {
      throw new Error("recovery coordinator requires an operation contract");
    }
    this.index = index;
    this.operationContract = operationContract;
    this.pendingRuns = new Map();
  }

  reconcileRun(runId, { context = null, runtimeStatus = null, fence = null, reconciledAt = new Date().toISOString() } = {}) {
    if (this.pendingRuns.has(runId)) return this.pendingRuns.get(runId);
    const reconciliation = this.#reconcileRun(runId, { context, runtimeStatus, fence, reconciledAt });
    this.pendingRuns.set(runId, reconciliation);
    void reconciliation.then(() => this.pendingRuns.delete(runId), () => this.pendingRuns.delete(runId));
    return reconciliation;
  }

  async #reconcileRun(runId, { context, runtimeStatus, fence, reconciledAt }) {
    const machine = new (nativeBinding().NativeReconciliation)(JSON.stringify({ runId, runtimeStatus, fence, reconciledAt }));
    let effect = JSON.parse(machine.next());
    try {
      while (effect.method !== "complete") {
        const value = effect.method === "resolve"
          ? await this.operationContract.reconcile(Object.freeze(effect.args[0]), context)
          : await this.index[effect.method](...effect.args);
        effect = JSON.parse(machine.accept(JSON.stringify({ method: effect.method, value })));
      }
    } catch (error) {
      await this.index.markReconciliationRequired([runId], error instanceof Error ? error.message : String(error), reconciledAt);
      throw error;
    }
    return Object.freeze({
      ...effect.result,
      operations: Object.freeze(effect.result.operations.map(Object.freeze)),
      unresolved: Object.freeze(effect.result.unresolved.map(Object.freeze)),
    });
  }
}

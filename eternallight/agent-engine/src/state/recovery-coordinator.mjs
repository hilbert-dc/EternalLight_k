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
    const candidates = await this.index.reconciliationCandidates(runId);
    const operations = [];
    try {
      for (const candidate of candidates) {
        const resolution = await this.operationContract.reconcile(Object.freeze({ ...candidate }), context);
        await this.index.settleRequest(runId, candidate.requestId, resolution, reconciledAt);
        operations.push(Object.freeze({ requestId: candidate.requestId, ...resolution }));
      }
      if (runtimeStatus) await this.index.reconcileRun(runId, runtimeStatus, reconciledAt, { fence });
    } catch (error) {
      await this.index.markReconciliationRequired([runId], error instanceof Error ? error.message : String(error), reconciledAt);
      throw error;
    }
    const unresolved = (await this.index.reconciliationCandidates(runId)).map(({ requestId, operation }) => ({ requestId, operation }));
    return Object.freeze({
      runId,
      status: unresolved.length ? "outcome-unknown" : "reconciled",
      operations: Object.freeze(operations),
      unresolved: Object.freeze(unresolved),
    });
  }
}

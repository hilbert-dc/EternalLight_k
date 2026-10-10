import { nativeBinding } from "../native.mjs";
import { TaskBudget } from "./budget.mjs";

export class PersistentTaskBudget extends TaskBudget {
  #persistent;
  constructor({ databasePath, runId, ...options }) {
    const native = new (nativeBinding().NativePersistentBudget)(databasePath, runId, JSON.stringify({ options, now: Date.now() }));
    super(options, native);
    this.#persistent = native;
    Object.defineProperty(this, "runId", { value: runId, enumerable: true });
  }
  observeFailure(observation) {
    const fields = ["status", "runId", "requestId", "connectionId", "logicalModel", "errorCategory", "errorCode", "errorStatusCode", "retryable"];
    this.#persistent.observeFailure(JSON.stringify(Object.fromEntries(fields.map(key => [key, observation[key]]))));
  }
  complete(status = "terminal") { this.#persistent.complete(status); }
  close() { this.#persistent.close(); }
  static receipt(databasePath, runId) { return Object.freeze(JSON.parse(nativeBinding().NativePersistentBudget.receipt(databasePath, runId))); }
  // Host must verify the provider receipt and its original reservation identity.
  // This settles accounting only; it never resumes or grants execution.
  static reconcileUsage(databasePath, runId, { id, usage, receiptRef }) {
    return Object.freeze(JSON.parse(nativeBinding().NativePersistentBudget.reconcileUsage(databasePath, runId, JSON.stringify({ id, usage, receiptRef }))));
  }
  static recoverOrphans(databasePath) { return Object.freeze(JSON.parse(nativeBinding().NativePersistentBudget.recover(databasePath))); }
}

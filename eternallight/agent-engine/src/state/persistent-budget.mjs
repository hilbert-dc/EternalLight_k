import { nativeBinding } from "../native.mjs";
import { TaskBudget } from "./budget.mjs";

export class PersistentTaskBudget extends TaskBudget {
  #persistent;
  constructor({ databasePath, runId, ...options }) {
    const native = new (nativeBinding().NativePersistentBudget)(databasePath, runId, JSON.stringify({ options, now: Date.now() }));
    super(options, native);
    this.#persistent = native;
  }
  complete(status = "terminal") { this.#persistent.complete(status); }
  close() { this.#persistent.close(); }
  static receipt(databasePath, runId) { return Object.freeze(JSON.parse(nativeBinding().NativePersistentBudget.receipt(databasePath, runId))); }
  static recoverOrphans(databasePath) { return Object.freeze(JSON.parse(nativeBinding().NativePersistentBudget.recover(databasePath))); }
}

import { createConservativeRecoveryOperationContract } from "./recovery-operation-contract.mjs";
import { nativeBinding } from "../native.mjs";

export class HostRecoveryJournal {
  #native = new (nativeBinding().NativeRecoveryJournal)();
  #threadId = null;
  constructor(index, operationContract = createConservativeRecoveryOperationContract()) {
    this.index = index;
    this.operationContract = operationContract;
    this.pending = Promise.resolve();
    this.lastError = null;
  }

  #apply(method, input = {}) { return JSON.parse(this.#native.apply(method, JSON.stringify(input))); }

  async #persist(action) {
    try {
      await this.index[action.method](...action.args);
      this.#apply("commit");
    } catch (error) {
      this.#apply("abort");
      throw error;
    }
  }

  beginRun(run) {
    return this.#enqueue(async () => {
      if (typeof run.threadId !== "string" || !run.threadId.trim() || run.threadId.length > 160 || /[\u0000-\u001f\u007f]/.test(run.threadId)) {
        throw new Error("recovery run requires its bound root thread identity");
      }
      const action = this.#apply("begin", run);
      await this.#persist(action);
      this.#threadId = run.threadId;
      this.lastError = null;
    });
  }

  observe(event) {
    return this.#enqueue(async () => {
      // The active session streams child events as well as root events. They
      // cannot settle a root model request or collide with its MCP item IDs.
      // Child writes are recovered from the Runtime's parent-bound ledger.
      // Missing identity is not permission to borrow the current root.
      if (!this.#threadId || event?.params?.threadId !== this.#threadId) return;
      const input = { event };
      for (let action = this.#apply("observe", input); action; action = this.#apply("observe", input)) {
        if (action.kind === "classify") {
          input.classification = this.operationContract.classify(action.toolCall);
          input.requestHash = input.classification.effect === "write"
            ? await this.operationContract.hash(action.toolCall, input.classification) : null;
        } else if (action.kind === "receipt") {
          input.resolution = (await this.operationContract.receipt({ event, toolCall: action.toolCall, result: action.toolCall.result ?? null }, action.classification)) ?? null;
        } else {
          await this.#persist(action);
        }
      }
    });
  }

  // Only runtime reconciliation, never optimistic Host events, terminalizes a run.
  completeRun() { return this.#enqueue(() => { const result = this.#apply("end"); this.#threadId = null; return result; }); }
  abandonRun() { return this.#enqueue(() => { this.#apply("end"); this.#threadId = null; }); }

  async flush() {
    await this.pending;
    if (this.lastError) throw this.lastError;
  }

  #enqueue(operation) {
    const queued = this.pending.then(operation);
    this.pending = queued.catch(error => { this.lastError = error; });
    return queued;
  }
}

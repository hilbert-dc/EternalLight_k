import { createConservativeRecoveryOperationContract } from "./recovery-operation-contract.mjs";
import { nativeBinding } from "../native.mjs";

export class HostRecoveryJournal {
  #native = new (nativeBinding().NativeRecoveryJournal)();
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
      const action = this.#apply("begin", run);
      await this.#persist(action);
      this.lastError = null;
    });
  }

  observe(event) {
    return this.#enqueue(async () => {
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
  completeRun() { return this.#enqueue(() => this.#apply("end")); }
  abandonRun() { return this.#enqueue(() => { this.#apply("end"); }); }

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

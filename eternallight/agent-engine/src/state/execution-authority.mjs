import { nativeBinding } from "../native.mjs";

/** Rust owns authority records; callers receive detached snapshots only. */
export class ExecutionAuthority {
  #native = new (nativeBinding().NativeExecutionAuthority)();
  #apply(method, input) {
    try { return Object.freeze(JSON.parse(this.#native.apply(method, JSON.stringify(input)))); }
    catch (error) {
      if (error.message.startsWith("EXECUTION_REVOKED:")) {
        error.message = error.message.slice("EXECUTION_REVOKED:".length);
        error.code = "EXECUTION_REVOKED";
      }
      throw error;
    }
  }
  issue(input) { return this.#apply("issue", input); }
  validate(input) { return this.#apply("validate", input); }
  revoke(input) { return this.#apply("revoke", input); }
  markDrained(runId) { return this.#apply("drained", { runId }); }
  markReconciled(runId) { return this.#apply("reconciled", { runId }); }
  terminalize(runId, outcome) { return this.#apply("terminalize", { runId, outcome }); }
  snapshot(runId) { return this.#apply("snapshot", { runId }); }
  release(runId) { this.#apply("release", { runId }); }
}

function requireMethod(value, method, label) {
  if (typeof value?.[method] !== "function") throw new Error(`${label} is missing ${method}`);
}

/**
 * Product-neutral runtime composition for one reusable Codex session.
 *
 * Products provide configured App Server and Bridge instances plus budget and
 * recovery adapters. This class owns their ordering so execution authority,
 * cancellation, budget settlement, and recovery cannot diverge per product.
 */
export class CodexSessionRuntime {
  constructor({ appServer, bridge, budgetFactory, bridgeOwnership = "owned", closeAppServer = (server) => server.close(), reconcile = async () => ({ outcome: "cancelled", unresolvedOperations: [] }) }) {
    for (const method of ["ensureSession", "resumeSession", "bindExecution", "sendTurn", "revokeExecution", "interruptTurn", "close", "subscribeEvents"]) {
      requireMethod(appServer, method, "Codex App Server");
    }
    for (const method of ["start", "beginTaskBudget", "beginExecutionLease", "cancelActiveRequests", "endExecutionLease", "endTaskBudget", "close"]) {
      requireMethod(bridge, method, "model bridge");
    }
    if (typeof budgetFactory !== "function") throw new Error("Codex runtime requires a budget factory");
    if (typeof closeAppServer !== "function") throw new Error("Codex runtime App Server closer must be a function");
    if (!["owned", "borrowed"].includes(bridgeOwnership)) throw new Error("Codex runtime bridge ownership is invalid");
    if (typeof reconcile !== "function") throw new Error("Codex runtime reconcile adapter must be a function");
    Object.assign(this, { appServer, bridge, budgetFactory, bridgeOwnership, closeAppServer, reconcile });
    this.active = null;
    this.closed = false;
    this.cleanup = null;
  }

  subscribeEvents(subscriber) {
    return this.appServer.subscribeEvents(subscriber);
  }

  ensureSession() {
    this.#assertOpen();
    return this.appServer.ensureSession();
  }

  resumeSession(threadId) {
    this.#assertOpen();
    return this.appServer.resumeSession(threadId);
  }

  async bindExecution({ lease, sessionPlan }) {
    this.#assertOpen();
    if (this.active || this.cleanup) throw new Error("Codex runtime already has an active execution");
    let resolveBinding;
    const bindingDone = new Promise((resolve) => { resolveBinding = resolve; });
    const execution = {
      lease,
      budget: null,
      revoked: false,
      budgetStarted: false,
      leaseStarted: false,
      appBound: false,
      revocation: null,
      bindingDone,
      resolveBinding,
    };
    this.active = execution;
    try {
      execution.budget = await this.budgetFactory({ lease, sessionPlan });
      if (!execution.budget) throw new Error("Codex runtime budget factory returned no budget");
      this.#assertExecutionCurrent(execution);
      await this.bridge.start();
      this.#assertExecutionCurrent(execution);
      await this.bridge.beginTaskBudget(execution.budget);
      execution.budgetStarted = true;
      this.#assertExecutionCurrent(execution);
      await this.bridge.beginExecutionLease(lease);
      execution.leaseStarted = true;
      this.#assertExecutionCurrent(execution);
      await this.appServer.bindExecution({ lease, sessionPlan });
      execution.appBound = true;
      this.#assertExecutionCurrent(execution);
    } catch (error) {
      try {
        if (execution.revoked && execution.appBound && execution.revokeApp) await execution.revokeApp();
      } finally {
        await this.#settleExecution(execution, "bind-failed").catch(() => undefined);
      }
      throw error;
    } finally {
      execution.resolveBinding();
    }
  }

  async sendTurn(input, options) {
    const execution = this.#activeExecution();
    try {
      this.#assertExecutionCurrent(execution);
      const dispatch = options?.dispatch;
      if (dispatch !== undefined && typeof dispatch !== "function") throw new Error("Codex runtime dispatch override must be a function");
      return await (dispatch ? dispatch(this.appServer) : this.appServer.sendTurn(input, options));
    } finally {
      await this.#settleExecution(execution, execution.revoked ? "revoked" : "terminal");
    }
  }

  async revokeExecution(request) {
    this.#assertOpen();
    const execution = this.active;
    if (!execution) throw new Error("Codex runtime has no active execution");
    if (execution.lease.runId !== request.runId || execution.lease.leaseId !== request.leaseId || execution.lease.epoch !== request.expectedEpoch) {
      throw new Error("Codex runtime execution lease mismatch");
    }
    execution.revoked = true;
    execution.revokeApp ??= () => execution.appRevocation ??= Promise.resolve().then(() => this.appServer.revokeExecution(request));
    execution.revocation ??= (async () => {
      const cancellation = Promise.resolve(this.bridge.cancelActiveRequests());
      // Revoke the App Server before draining transport can finish the active
      // turn and remove its lease. Attach both promises immediately.
      await Promise.all([cancellation, execution.appBound ? execution.revokeApp() : undefined]);
      await execution.bindingDone;
      return execution.appRevocation ?? (execution.appBound
        ? execution.revokeApp()
        : { revokedEpoch: request.revokedEpoch });
    })();
    return execution.revocation;
  }

  interruptTurn(request) {
    return this.appServer.interruptTurn(request);
  }

  reconcileRun(context) {
    return this.reconcile(context);
  }

  async close() {
    if (this.closed) return;
    this.closed = true;
    const execution = this.active;
    if (execution) {
      execution.revoked = true;
      try { await this.bridge.cancelActiveRequests(); } catch { /* Continue closing all resources. */ }
      await execution.bindingDone;
      try { await this.appServer.interruptTurn(); } catch { /* Continue closing all resources. */ }
      await this.#settleExecution(execution, "closed").catch(() => undefined);
    } else if (this.cleanup) await this.cleanup.catch(() => undefined);
    const closers = [Promise.resolve().then(() => this.closeAppServer(this.appServer))];
    if (this.bridgeOwnership === "owned") closers.push(Promise.resolve().then(() => this.bridge.close()));
    const results = await Promise.allSettled(closers);
    const failure = results.find((result) => result.status === "rejected");
    if (failure) throw failure.reason;
  }

  #activeExecution() {
    this.#assertOpen();
    if (!this.active) throw new Error("Codex runtime has no active execution");
    return this.active;
  }

  #assertOpen() {
    if (this.closed) throw new Error("Codex runtime is closed");
  }

  #assertExecutionCurrent(execution) {
    this.#assertOpen();
    if (this.active !== execution || execution.revoked) {
      throw Object.assign(new Error("Codex runtime execution was revoked"), { code: "TASK_CANCELLED" });
    }
  }

  #settleExecution(execution, _outcome) {
    if (this.cleanup) return this.cleanup;
    this.cleanup = (async () => {
      let failure = null;
      if (execution.leaseStarted) {
        try {
          await this.bridge.endExecutionLease(execution.lease);
        } catch (error) {
          failure = error;
        }
      }
      if (execution.budgetStarted) {
        try {
          await this.bridge.endTaskBudget(execution.budget);
        } catch (error) {
          failure ??= error;
        }
      }
      if (execution.appBound && typeof this.appServer.endExecution === "function") {
        try {
          await this.appServer.endExecution(execution.lease);
        } catch (error) {
          failure ??= error;
        }
      }
      if (this.active === execution) this.active = null;
      if (failure) throw failure;
    })().finally(() => { this.cleanup = null; });
    return this.cleanup;
  }
}

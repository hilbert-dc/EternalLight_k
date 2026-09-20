import { randomUUID } from "node:crypto";

import { defineConnection } from "../connections/connection-spec.mjs";
import { isTrustedEvidenceSnapshot } from "../capabilities/evidence-repository.mjs";
import { resolveSessionPlan } from "../session/resolver.mjs";
import { ExecutionAuthority } from "../state/execution-authority.mjs";
import { assertExecutionLease, createExecutionLease } from "../state/execution-lease.mjs";

const TERMINAL_STATES = new Set(["completed", "failed", "cancelled", "outcome-unknown"]);
const RUNTIME_METHODS = Object.freeze(["ensureSession", "resumeSession", "bindExecution", "sendTurn", "revokeExecution", "interruptTurn", "close", "subscribeEvents"]);

function deepFreeze(value) {
  if (!value || typeof value !== "object") return value;
  for (const child of Object.values(value)) deepFreeze(child);
  return Object.isFrozen(value) ? value : Object.freeze(value);
}

function boundedParams(value) {
  const clone = structuredClone(value ?? {});
  if (Buffer.byteLength(JSON.stringify(clone), "utf8") > 256 * 1024) throw new Error("engine event exceeds byte limit");
  return clone;
}

function cancellationError(message = "engine run was cancelled") {
  return Object.assign(new Error(message), { code: "TASK_CANCELLED" });
}

/** Product-neutral owner of reusable Codex sessions and single-turn runs. */
export class AgentEngineHost {
  constructor({ connections, evidenceProvider, qualifier = null, runtimeFactory, maxRetainedEvents = 2_000, now = () => new Date() }) {
    if (!Array.isArray(connections) || connections.length === 0) throw new Error("engine host requires connections");
    this.connections = Object.freeze(connections.map(defineConnection));
    this.connectionsById = new Map(this.connections.map((connection) => [connection.id, connection]));
    if (this.connectionsById.size !== this.connections.length) throw new Error("engine host connection ids must be unique");
    if (typeof evidenceProvider !== "function") throw new Error("engine host requires an evidence provider");
    if (typeof runtimeFactory !== "function") throw new Error("engine host requires a runtime factory");
    if (!Number.isInteger(maxRetainedEvents) || maxRetainedEvents < 1) throw new Error("maxRetainedEvents must be positive");
    Object.assign(this, { evidenceProvider, qualifier, runtimeFactory, maxRetainedEvents, now });
    this.sessions = new Map();
    this.runs = new Map();
  }

  async inspectConnection({ connectionId, policy = {}, now = this.now() }) {
    const connection = this.connectionsById.get(connectionId);
    if (!connection) throw new Error(`unknown connection: ${connectionId}`);
    const evidence = this.#trustedEvidence(await this.evidenceProvider({ connectionId, now }));
    try {
      const plan = resolveSessionPlan({ connections: [connection], evidence, policy: { ...policy, capabilityProfileId: policy.capabilityProfileId ?? "text-v1", defaultConnectionId: connectionId }, now });
      const route = plan.routes[0];
      return deepFreeze({ connection, status: "available", selectedProtocol: route.protocol, enabledCapabilities: route.enabledExtensions, authorizedProfiles: plan.authorizedCapabilityProfiles, expiresAt: plan.expiresAt, missing: [] });
    } catch (error) {
      return deepFreeze({ connection, status: "unavailable", selectedProtocol: null, enabledCapabilities: [], authorizedProfiles: [], expiresAt: null, missing: [String(error?.message ?? error)] });
    }
  }

  async qualifyCapabilities(request) {
    if (!this.qualifier) throw new Error("engine host has no capability qualifier");
    return typeof this.qualifier === "function" ? this.qualifier(request) : this.qualifier.qualifyCapabilities(request);
  }

  async resolveSession({ policy, evidence = null, now = this.now() }) {
    const snapshot = this.#trustedEvidence(evidence ?? await this.evidenceProvider({ now }));
    return resolveSessionPlan({ connections: this.connections, evidenceSnapshot: snapshot, policy, now });
  }

  async startSession({ sessionPlan, sessionId = randomUUID(), runtimeOptions = {} }) {
    if (this.sessions.has(sessionId)) throw new Error(`session already exists: ${sessionId}`);
    const session = { sessionId, sessionPlan, runtime: null, authority: new ExecutionAuthority(), state: "preparing", threadId: null, activeRunId: null, unsubscribeRuntime: null, closePromise: null };
    this.sessions.set(sessionId, session);
    try {
      session.runtime = await this.runtimeFactory({ sessionPlan, runtimeOptions });
      this.#assertSessionPreparing(session, "preparing");
      this.#assertRuntime(session.runtime);
      session.unsubscribeRuntime = session.runtime.subscribeEvents((event) => this.#recordRuntimeEvent(session, event));
      this.#assertSessionPreparing(session, "preparing");
      session.threadId = await session.runtime.ensureSession();
      this.#assertSessionPreparing(session, "preparing");
      session.state = "idle";
      return deepFreeze({ sessionId, threadId: session.threadId, planId: sessionPlan.id });
    } catch (error) {
      if (!["closing", "closed"].includes(session.state)) session.state = "failed";
      await this.#closeRuntime(session);
      throw error;
    }
  }

  async resumeSession({ sessionPlan, threadId, sessionId = randomUUID(), runtimeOptions = {}, recovery = null }) {
    if (this.sessions.has(sessionId)) throw new Error(`session already exists: ${sessionId}`);
    if (recovery && typeof recovery.reconcile !== "function") throw new Error("session recovery must provide reconcile()");
    const session = { sessionId, sessionPlan, runtime: null, authority: new ExecutionAuthority(), state: "recovering", threadId: null, activeRunId: null, unsubscribeRuntime: null, closePromise: null };
    this.sessions.set(sessionId, session);
    try {
      const recoveryResult = recovery ? await recovery.reconcile({ sessionId, threadId, sessionPlan }) : { canContinue: true, unresolvedRequests: [] };
      this.#assertSessionPreparing(session, "recovering");
      if (recoveryResult.canContinue !== true) {
        session.state = "recovery-blocked";
        return deepFreeze({ sessionId, threadId, planId: sessionPlan.id, recovery: recoveryResult });
      }
      session.runtime = await this.runtimeFactory({ sessionPlan, runtimeOptions });
      this.#assertSessionPreparing(session, "recovering");
      this.#assertRuntime(session.runtime);
      session.unsubscribeRuntime = session.runtime.subscribeEvents((event) => this.#recordRuntimeEvent(session, event));
      session.threadId = await session.runtime.resumeSession(threadId);
      this.#assertSessionPreparing(session, "recovering");
      session.state = "idle";
      return deepFreeze({ sessionId, threadId: session.threadId, planId: sessionPlan.id, recovery: recoveryResult });
    } catch (error) {
      if (!["closing", "closed"].includes(session.state)) session.state = "failed";
      await this.#closeRuntime(session);
      throw error;
    }
  }

  async startRun({ sessionId, input, budgetId, runId = randomUUID(), mode = "interactive", maxDurationMs, turnOptions = {} }) {
    if (this.runs.has(runId)) throw new Error(`run already exists: ${runId}`);
    const session = this.#session(sessionId);
    if (session.state !== "idle" || session.activeRunId) throw new Error(`session is not idle: ${sessionId}`);
    const lease = createExecutionLease({ sessionPlan: session.sessionPlan, runId, budgetId, mode, now: this.now(), ...(maxDurationMs === undefined ? {} : { maxDurationMs }) });
    assertExecutionLease(lease, session.sessionPlan, { now: this.now().getTime() });
    const run = { runId, sessionId, sessionPlan: session.sessionPlan, lease, epoch: lease.epoch, state: "preparing", threadId: session.threadId, turn: null, sequence: 0, events: [], subscribers: new Set(), pendingItems: new Map(), completion: null, cancelPromise: null };
    this.runs.set(runId, run);
    session.activeRunId = runId;
    session.authority.issue({ lease });
    try {
      await session.runtime.bindExecution({ lease, sessionPlan: session.sessionPlan });
      if (run.state !== "preparing") throw cancellationError();
      run.state = "running";
      this.#record(run, { method: "engine/run/started", params: { threadId: run.threadId } });
      run.completion = this.#completeRun(session, run, input, turnOptions);
      return Object.freeze({ sessionId, runId, threadId: run.threadId, lease, completion: run.completion });
    } catch (error) {
      if (run.state === "cancelling" && run.cancelPromise) await run.cancelPromise;
      else if (!TERMINAL_STATES.has(run.state)) this.#terminalize(session, run, "failed", error);
      throw error;
    }
  }

  async cancelRun(runId, reason = "cancelled-by-host") {
    const run = this.#run(runId);
    if (TERMINAL_STATES.has(run.state)) return deepFreeze({ runId, status: run.state, revoked: run.state === "cancelled", interrupted: false, reconciliationRequired: run.state === "outcome-unknown" });
    if (run.cancelPromise) return run.cancelPromise;
    const session = this.#session(run.sessionId);
    run.state = "cancelling";
    this.#record(run, { method: "engine/run/cancellation-requested", params: { reason: String(reason).slice(0, 240) } });
    run.cancelPromise = (async () => {
      const authority = session.authority.revoke({ runId, leaseId: run.lease.leaseId, expectedEpoch: run.epoch, reason });
      const revocation = await session.runtime.revokeExecution({ runId, leaseId: run.lease.leaseId, expectedEpoch: run.epoch, revokedEpoch: authority.epoch, reason });
      if (Number.isInteger(revocation?.revokedEpoch) && revocation.revokedEpoch !== authority.epoch) throw new Error("runtime revocation epoch does not match execution authority");
      run.epoch = authority.epoch;
      this.#record(run, { method: "engine/run/execution-revoked", params: { revokedEpoch: run.epoch } });
      const interrupted = await session.runtime.interruptTurn({ runId, turnId: run.turn?.id ?? null });
      session.authority.markDrained(runId);
      const reconciliation = typeof session.runtime.reconcileRun === "function" ? await session.runtime.reconcileRun({ runId }) : { outcome: "cancelled", unresolvedOperations: [] };
      session.authority.markReconciled(runId);
      const reconciliationRequired = reconciliation?.outcome === "outcome-unknown" || (reconciliation?.unresolvedOperations?.length ?? 0) > 0;
      this.#terminalize(session, run, reconciliationRequired ? "outcome-unknown" : "cancelled");
      return deepFreeze({ runId, status: run.state, revoked: true, revokedEpoch: run.epoch, interrupted: Boolean(interrupted), reconciliationRequired });
    })().catch((error) => {
      session.state = "recovery-blocked";
      run.state = "cancellation-failed";
      this.#record(run, { method: "engine/run/cancellation-failed", params: { message: String(error?.message ?? error).slice(0, 500) } });
      throw error;
    });
    return run.cancelPromise;
  }

  subscribeEvents({ runId, afterSequence = 0 }, subscriber) {
    const run = this.#run(runId);
    if (!Number.isInteger(afterSequence) || afterSequence < 0 || typeof subscriber !== "function") throw new Error("event subscription is invalid");
    const firstAvailable = run.events[0]?.sequence ?? run.sequence + 1;
    if (afterSequence > 0 && afterSequence < firstAvailable - 1) throw new Error(`event cursor expired; first available sequence is ${firstAvailable}`);
    for (const event of run.events) if (event.sequence > afterSequence) { try { subscriber(event); } catch { /* Isolate replay consumers. */ } }
    run.subscribers.add(subscriber);
    return () => run.subscribers.delete(subscriber);
  }

  runStatus(runId) {
    const run = this.#run(runId);
    return deepFreeze({ runId, sessionId: run.sessionId, threadId: run.threadId, state: run.state, sequence: run.sequence, leaseId: run.lease.leaseId, epoch: run.epoch });
  }

  sessionStatus(sessionId) {
    const session = this.#session(sessionId);
    return deepFreeze({ sessionId, threadId: session.threadId, state: session.state, activeRunId: session.activeRunId, planId: session.sessionPlan.id });
  }

  closeRun(runId) {
    const run = this.#run(runId);
    if (!TERMINAL_STATES.has(run.state)) throw new Error("cannot release a non-terminal run");
    this.#session(run.sessionId).authority.release(runId);
    this.runs.delete(runId);
  }

  async closeSession(sessionId) {
    const session = this.#session(sessionId);
    if (session.closePromise) return session.closePromise;
    session.state = "closing";
    session.closePromise = (async () => {
      if (session.activeRunId) await this.cancelRun(session.activeRunId, "session-closing");
      await this.#closeRuntime(session);
      session.state = "closed";
      this.sessions.delete(sessionId);
    })();
    return session.closePromise;
  }

  async #completeRun(session, run, input, turnOptions) {
    try {
      const result = await session.runtime.sendTurn(input, turnOptions);
      run.turn = result.turn ?? null;
      if (run.state === "cancelling" || result.turn?.status === "interrupted") {
        if (run.cancelPromise) await run.cancelPromise;
        else {
          session.authority.revoke({ runId: run.runId, leaseId: run.lease.leaseId, expectedEpoch: run.epoch, reason: "runtime-interrupted" });
          session.authority.markDrained(run.runId);
          session.authority.markReconciled(run.runId);
          this.#terminalize(session, run, "cancelled");
        }
      } else this.#terminalize(session, run, "completed");
      return Object.freeze({ ...result, sessionId: session.sessionId, runId: run.runId, status: run.state });
    } catch (error) {
      if (run.state === "cancelling" && run.cancelPromise) await run.cancelPromise;
      else if (error?.code === "TASK_CANCELLED") {
        session.authority.revoke({ runId: run.runId, leaseId: run.lease.leaseId, expectedEpoch: run.epoch, reason: "runtime-cancelled" });
        session.authority.markDrained(run.runId);
        session.authority.markReconciled(run.runId);
        this.#terminalize(session, run, "cancelled");
      }
      else this.#terminalize(session, run, "failed", error);
      if (run.state === "cancelled") return Object.freeze({ sessionId: session.sessionId, runId: run.runId, status: run.state, turn: run.turn });
      throw error;
    }
  }

  #terminalize(session, run, state, error = null) {
    if (TERMINAL_STATES.has(run.state)) return;
    run.state = state;
    session.authority.terminalize(run.runId, state);
    if (session.activeRunId === run.runId) session.activeRunId = null;
    if (!new Set(["closing", "closed"]).has(session.state)) {
      session.state = state === "outcome-unknown" ? "recovery-blocked" : "idle";
    }
    this.#record(run, { method: `engine/run/${state}`, params: { threadId: run.threadId, turnId: run.turn?.id ?? null, ...(error ? { message: String(error?.message ?? error).slice(0, 500) } : {}) } });
  }

  #recordRuntimeEvent(session, event) {
    const run = this.runs.get(session.activeRunId);
    if (run) this.#record(run, event);
  }

  #record(run, event) {
    const method = event?.method ?? "engine/unknown";
    const params = boundedParams(event?.params);
    const eventThreadId = params.threadId ?? params.thread?.id ?? run.threadId;
    if (eventThreadId === run.threadId) this.#updateStateFromEvent(run, method, params);
    const item = params.item ?? {};
    const record = deepFreeze({ sequence: ++run.sequence, runId: run.runId, threadId: eventThreadId, parentThreadId: params.parentThreadId ?? null, turnId: params.turnId ?? params.turn?.id ?? run.turn?.id ?? null, itemId: params.itemId ?? item.id ?? null, requestId: params.requestId ?? null, callId: params.callId ?? item.callId ?? null, connectionId: params.connectionId ?? null, epoch: run.epoch, emittedAt: this.now().toISOString(), method, params });
    run.events.push(record);
    if (run.events.length > this.maxRetainedEvents) run.events.splice(0, run.events.length - this.maxRetainedEvents);
    for (const subscriber of run.subscribers) { try { subscriber(record); } catch { /* Isolate consumers. */ } }
  }

  #updateStateFromEvent(run, method, params) {
    if (TERMINAL_STATES.has(run.state) || run.state === "cancelling") return;
    const item = params.item ?? {};
    const itemId = params.itemId ?? item.id ?? null;
    if (method === "turn/started") run.state = "running";
    else if (/approval|userInput/i.test(method)) run.state = "waiting-user";
    else if (method === "item/started" && itemId) {
      const waitingState = item.type === "collabAgentToolCall" ? "waiting-child" : /tool/i.test(item.type ?? "") ? "waiting-tool" : null;
      if (waitingState) { run.pendingItems.set(itemId, waitingState); run.state = waitingState; }
    } else if (method === "item/completed" && itemId) {
      run.pendingItems.delete(itemId);
      run.state = [...run.pendingItems.values()].at(-1) ?? "running";
    }
  }

  #assertRuntime(runtime) {
    for (const method of RUNTIME_METHODS) if (typeof runtime?.[method] !== "function") throw new Error(`engine runtime is missing ${method}`);
  }

  #trustedEvidence(value) {
    if (!isTrustedEvidenceSnapshot(value)) throw new Error("engine host requires a trusted evidence snapshot");
    return value;
  }

  #assertSessionPreparing(session, expectedState) {
    if (session.state !== expectedState) throw cancellationError("session closed during preparation");
  }

  async #closeRuntime(session) {
    session.unsubscribeRuntime?.();
    session.unsubscribeRuntime = null;
    await session.runtime?.close();
  }

  #session(sessionId) {
    const session = this.sessions.get(sessionId);
    if (!session) throw new Error(`unknown session: ${sessionId}`);
    return session;
  }

  #run(runId) {
    const run = this.runs.get(runId);
    if (!run) throw new Error(`unknown run: ${runId}`);
    return run;
  }
}

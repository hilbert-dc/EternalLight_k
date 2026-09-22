import { randomUUID } from "node:crypto";
import { nativeBinding } from "../native.mjs";
import { defineConnection } from "../connections/connection-spec.mjs";
import { isTrustedEvidenceSnapshot } from "../capabilities/evidence-repository.mjs";
import { resolveSessionPlan } from "../session/resolver.mjs";

function freeze(value) { if (value && typeof value === "object") { Object.values(value).forEach(freeze); Object.freeze(value); } return value; }
function errorFrom(value) {
  const message = typeof value === "string" ? value : value?.message ?? "engine operation failed";
  if (message.startsWith("TASK_CANCELLED:")) return Object.assign(new Error(message.slice(15)), { code: "TASK_CANCELLED" });
  return Object.assign(new Error(message), { code: value?.code });
}
const RUNTIME_METHODS = ["ensureSession", "resumeSession", "bindExecution", "sendTurn", "revokeExecution", "interruptTurn", "close", "subscribeEvents"];

/** Product callbacks and Promise delivery for the native session/run owner. */
export class AgentEngineHost {
  constructor({ connections, evidenceProvider, qualifier = null, runtimeFactory, maxRetainedEvents = 2000, now = () => new Date() }) {
    if (!Array.isArray(connections) || !connections.length) throw new Error("engine host requires connections");
    this.connections = Object.freeze(connections.map(defineConnection));
    this.connectionsById = new Map(this.connections.map(connection => [connection.id, connection]));
    if (this.connectionsById.size !== connections.length) throw new Error("engine host connection ids must be unique");
    if (typeof evidenceProvider !== "function" || typeof runtimeFactory !== "function") throw new Error("engine host requires evidence provider and runtime factory functions");
    Object.assign(this, { evidenceProvider, qualifier, runtimeFactory, now });
    this.native = new (nativeBinding().NativeHostCoordinator)(maxRetainedEvents);
    this.bindings = new Map(); this.runBindings = new Map(); this.pending = new Map(); this.completions = new Map(); this.subscribers = new Map(); this.nextId = 0;
  }
  async inspectConnection({ connectionId, policy = {}, now = this.now() }) {
    const connection = this.connectionsById.get(connectionId);
    if (!connection) throw new Error(`unknown connection: ${connectionId}`);
    const evidence = this.#trusted(await this.evidenceProvider({ connectionId, now }));
    try {
      const plan = resolveSessionPlan({ connections: [connection], evidence, policy: { ...policy, capabilityProfileId: policy.capabilityProfileId ?? "text-v1", defaultConnectionId: connectionId }, now });
      return freeze({ connection, status: "available", selectedProtocol: plan.routes[0].protocol, enabledCapabilities: plan.routes[0].enabledExtensions, authorizedProfiles: plan.authorizedCapabilityProfiles, expiresAt: plan.expiresAt, missing: [] });
    } catch (error) { return freeze({ connection, status: "unavailable", selectedProtocol: null, enabledCapabilities: [], authorizedProfiles: [], expiresAt: null, missing: [String(error?.message ?? error)] }); }
  }
  async qualifyCapabilities(request) {
    if (!this.qualifier) throw new Error("engine host has no capability qualifier");
    return typeof this.qualifier === "function" ? this.qualifier(request) : this.qualifier.qualifyCapabilities(request);
  }
  async resolveSession({ policy, evidence = null, now = this.now() }) {
    return resolveSessionPlan({ connections: this.connections, evidenceSnapshot: this.#trusted(evidence ?? await this.evidenceProvider({ now })), policy, now });
  }
  async startSession({ sessionPlan, sessionId = randomUUID(), runtimeOptions = {} }) {
    if (!this.bindings.has(sessionId)) this.bindings.set(sessionId, { runtimeOptions, runtime: null, unsubscribe: null });
    return this.#command("startSession", { sessionPlan, sessionId });
  }
  async resumeSession({ sessionPlan, threadId, sessionId = randomUUID(), runtimeOptions = {}, recovery = null }) {
    if (recovery && typeof recovery.reconcile !== "function") throw new Error("session recovery must provide reconcile()");
    if (!this.bindings.has(sessionId)) this.bindings.set(sessionId, { runtimeOptions, recovery, runtime: null, unsubscribe: null });
    return this.#command("resumeSession", { sessionPlan, threadId, sessionId });
  }
  async startRun({ sessionId, input, budgetId, runId = randomUUID(), mode = "interactive", maxDurationMs, turnOptions = {} }) {
    const inserted = !this.runBindings.has(runId);
    if (inserted) this.runBindings.set(runId, { turnOptions });
    try { return await this.#command("startRun", { sessionId, input, budgetId, runId, mode, maxDurationMs }); }
    catch (error) { if (inserted) this.runBindings.delete(runId); throw error; }
  }
  cancelRun(runId, reason = "cancelled-by-host") { return this.#command("cancelRun", { runId, reason }); }
  closeSession(sessionId) { return this.#command("closeSession", { sessionId }); }
  closeRun(runId) {
    this.#apply("closeRun", { runId });
    this.runBindings.delete(runId); this.completions.delete(runId); this.subscribers.delete(runId);
  }
  runStatus(runId) { return freeze(this.#read("runStatus", { runId })); }
  sessionStatus(sessionId) { return freeze(this.#read("sessionStatus", { sessionId })); }
  subscribeEvents({ runId, afterSequence = 0 }, subscriber) {
    if (typeof subscriber !== "function") throw new Error("event subscription is invalid");
    for (const event of this.#read("replay", { runId, afterSequence })) { try { subscriber(freeze(event)); } catch { /* Isolated consumer. */ } }
    if (!this.subscribers.has(runId)) this.subscribers.set(runId, new Set());
    const subscribers = this.subscribers.get(runId); subscribers.add(subscriber);
    return () => subscribers.delete(subscriber);
  }
  #trusted(value) { if (!isTrustedEvidenceSnapshot(value)) throw new Error("engine host requires a trusted evidence snapshot"); return value; }
  #read(method, params) { return JSON.parse(this.native.apply(method, JSON.stringify({ ...params, now: this.now().toISOString() }))); }
  #command(method, params) {
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      try { this.#apply("command", { id, method, params }); }
      catch (error) { this.pending.delete(id); reject(errorFrom(error)); }
    });
  }
  #apply(method, input) {
    for (const effect of this.#read(method, input)) {
      if (effect.kind === "resolve" || effect.kind === "reject" || effect.kind === "started") {
        const pending = this.pending.get(effect.id); this.pending.delete(effect.id);
        if (effect.kind === "reject") pending?.reject(errorFrom(effect.error));
        else if (effect.kind === "started") {
          let resolve, reject;
          const completion = new Promise((yes, no) => { resolve = yes; reject = no; });
          void completion.catch(() => {});
          this.completions.set(effect.runId, { resolve, reject });
          pending?.resolve(Object.freeze({ ...freeze(effect.result), completion }));
        } else pending?.resolve(freeze(effect.result));
      } else if (effect.kind === "completion") {
        const delivery = this.completions.get(effect.runId);
        if (effect.error) delivery?.reject(errorFrom(effect.error)); else delivery?.resolve(freeze(effect.result));
      } else if (effect.kind === "event") {
        const record = freeze(effect.event);
        for (const subscriber of this.subscribers.get(effect.runId) ?? []) { try { subscriber(record); } catch { /* Isolated consumer. */ } }
      } else if (effect.kind === "releaseSession") {
        this.bindings.delete(effect.sessionId);
      } else if (effect.kind === "callback") {
        void Promise.resolve().then(() => this.#callback(effect)).then(
          result => this.#apply("callback", { id: effect.id, result: result ?? null }),
          error => this.#apply("callback", { id: effect.id, error: { message: String(error?.message ?? error), code: error?.code } }),
        );
      } else throw new Error(`Unknown native host effect: ${effect.kind}`);
    }
  }
  async #callback({ sessionId, runId, method, params }) {
    const binding = this.bindings.get(sessionId);
    switch (method) {
      case "recoverSession": return binding.recovery ? binding.recovery.reconcile(params) : { canContinue: true, unresolvedRequests: [] };
      case "runtimeFactory": {
        binding.runtime = await this.runtimeFactory({ ...params, runtimeOptions: binding.runtimeOptions });
        for (const name of RUNTIME_METHODS) if (typeof binding.runtime?.[name] !== "function") throw new Error(`engine runtime is missing ${name}`);
        return null;
      }
      case "subscribe": binding.unsubscribe = binding.runtime.subscribeEvents(event => this.#apply("event", { sessionId, event })); return null;
      case "ensureSession": return binding.runtime.ensureSession();
      case "resumeSession": return binding.runtime.resumeSession(params.threadId);
      case "bindExecution": return binding.runtime.bindExecution(params);
      case "sendTurn": return binding.runtime.sendTurn(params.input, this.runBindings.get(runId)?.turnOptions);
      case "revokeExecution": return binding.runtime.revokeExecution(params);
      case "interruptTurn": return binding.runtime.interruptTurn(params);
      case "reconcileRun": return binding.runtime.reconcileRun ? binding.runtime.reconcileRun(params) : { outcome: "cancelled", unresolvedOperations: [] };
      case "close": {
        const unsubscribe = binding.unsubscribe; binding.unsubscribe = null;
        try { unsubscribe?.(); } finally { await binding.runtime?.close(); }
        return null;
      }
      default: throw new Error(`Unknown native host callback: ${method}`);
    }
  }
}

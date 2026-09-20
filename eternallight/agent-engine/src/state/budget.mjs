import { nativeBinding } from "../native.mjs";

export class TaskBudget {
  #native;
  constructor(options = {}, native = null) { this.#native = native ?? new (nativeBinding().NativeTaskBudget)(JSON.stringify({ options, now: Date.now() })); }
  get state() { return JSON.parse(this.#native.inspect()); }
  get id() { return this.state.id; }
  get startedAt() { return this.state.startedAt; }
  get maxDurationMs() { return this.state.maxDurationMs; }
  get maxRequests() { return this.state.maxRequests; }
  get maxTokens() { return this.state.maxTokens; }
  get reservations() { return new Map(Object.entries(this.state.reservations)); }
  get settlements() { return new Map(Object.entries(this.state.settlements)); }
  transition(method, parameters = {}) {
    return JSON.parse(this.#native.apply(`budget.${method}`, JSON.stringify(parameters)));
  }
  reserveRequest(parameters = {}, now = Date.now()) { return this.transition("reserve", { parameters, now }); }
  settleReservation(id, usage = null, { outcome = "completed" } = {}) { return Object.freeze(this.transition("settle", { id, usage, outcome })); }
  observeUsage(usage) { return this.transition("observe", { usage }) ?? undefined; }
  markOutstandingUnknown() { return Object.freeze(this.transition("unknown")); }
  remainingDurationMs(now = Date.now()) { return this.transition("remaining", { now }); }
  snapshot() { return Object.freeze(this.transition("snapshot")); }
}

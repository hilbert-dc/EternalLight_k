import { nativeBinding, nativeCall } from "../native.mjs";

export function createExecutionLease({ sessionPlan, runId, budgetId, epoch = 1, mode = "interactive", now = new Date(), maxDurationMs = null, deadlineAtMs }) {
  const lease = nativeCall("lease.create", { sessionPlan, runId, budgetId, epoch, mode, now: now instanceof Date ? now.getTime() : Date.parse(now), planExpiry: Date.parse(sessionPlan?.expiresAt), maxDurationMs, deadlineAtMs });
  return Object.freeze({ ...lease, issuedAt: new Date(lease.issuedAt).toISOString(), deadline: new Date(lease.deadline).toISOString(), routeGrants: Object.freeze(lease.routeGrants.map(Object.freeze)) });
}

export function assertExecutionLease(lease, sessionPlan, { epoch = lease?.epoch, now = Date.now() } = {}) {
  nativeCall("lease.validate", { lease, sessionPlan, epoch, now, deadline: Date.parse(lease?.deadline), planExpiry: Date.parse(sessionPlan?.expiresAt) });
  return lease;
}

export class RouteLineage {
  #native;
  constructor(lease) { this.#native = new (nativeBinding().NativeRouteLineage)(JSON.stringify(lease)); }
  authorize(input) { return JSON.parse(this.#native.authorize(JSON.stringify(input))); }
}

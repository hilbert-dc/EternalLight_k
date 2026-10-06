import { ENGINE_CONTRACT } from "../contracts/capabilities.mjs";
import { ensureTrustedEvidenceSnapshot, compileEvidenceSession } from "../capabilities/evidence-repository.mjs";
import { scopeCovers } from "../capabilities/evidence-store.mjs";
import { evaluateCapabilityProof } from "../capabilities/proof-contract.mjs";
import { nativeCall } from "../native.mjs";

const COMPILED_PLANS = new WeakSet();
export function assertTrustedSessionPlan(plan) {
  if (!COMPILED_PLANS.has(plan)) throw new Error("renewal requires a trusted evidence compiled session plan");
  return plan;
}
export function renewSessionPlan(previousPlan, sessionPlan, { now = new Date() } = {}) {
  assertTrustedSessionPlan(sessionPlan);
  nativeCall("session.renew", { previousPlan, sessionPlan, now: now.getTime() });
  return sessionPlan;
}

export function isQualifiedEvidence(record, requiredScope = null, expectedDependencies = {}) {
  return Boolean(record && evaluateCapabilityProof(record, { expectedDependencies }).status === "qualified"
    && (!requiredScope || scopeCovers(record.scope, requiredScope)));
}
function deepFreeze(value) {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) deepFreeze(child);
    Object.freeze(value);
  }
  return value;
}
export function resolveProtocolSelections(connections, evidence, options = {}) {
  const now = options.now ?? new Date();
  const snapshot = ensureTrustedEvidenceSnapshot(evidence, { now });
  return deepFreeze(compileEvidenceSession(snapshot, "protocols", {
    connections, now: now.getTime(), policy: options,
    engineContract: options.engineContract ?? ENGINE_CONTRACT,
    requiredCapabilities: options.requiredCapabilities,
  }));
}
export function resolveSessionPlan({ connections, evidence, evidenceSnapshot, policy, engineContract = ENGINE_CONTRACT, now = new Date() }) {
  const snapshot = ensureTrustedEvidenceSnapshot(evidenceSnapshot ?? evidence, { now });
  const plan = deepFreeze(compileEvidenceSession(snapshot, "session", { connections, policy, engineContract, now: now.getTime() }));
  COMPILED_PLANS.add(plan);
  return plan;
}

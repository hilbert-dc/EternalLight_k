import assert from "node:assert/strict";
import { test } from "node:test";
import { trustedTestEvidence, verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { inspectCodexConnection, resolveCodexProvider } from "../codex-runtime.mjs";

const now = new Date("2026-09-19T01:00:00Z");
const inspected = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
function records(expiresAt, evidenceRefPrefix, checkedAt = "2026-09-19T00:00:00Z") {
  return verifiedBaselineEvidence(inspected.connection, { protocol: "responses", expiresAt, checkedAt, evidenceRefPrefix });
}
const resolve = (evidence, minimumEvidenceValidityMs) => resolveCodexProvider({
  provider: inspected.provider, model: inspected.model, evidence, now, minimumEvidenceValidityMs,
});

test("renewal selects existing fresh proofs, never rewrites an expiring proof", () => {
  const old = records("2026-09-19T01:02:00Z", "short");
  const frozen = structuredClone(old);
  assert.equal(resolve(old, 0).sessionPlan.expiresAt, "2026-09-19T01:02:00.000Z");
  assert.throws(() => resolve(old, 240_000));
  const fresh = records("2026-09-20T00:00:00Z", "fresh", "2026-09-19T00:30:00Z");
  const renewed = resolve([...old, ...fresh], 240_000);
  assert.equal(renewed.sessionPlan.expiresAt, "2026-09-20T00:00:00.000Z");
  assert.deepEqual(old, frozen);
});

test("renewal cannot drop a still-valid failure merely because it expires soon", () => {
  const fresh = records("2026-09-20T00:00:00Z", "fresh");
  const failure = { ...fresh.find(record => record.capability === "text"),
    status: "failed", measurementStatus: "failed", evidenceRef: "failed-access",
    checkedAt: now.toISOString(), expiresAt: "2026-09-19T01:02:00Z",
    sample: { category: "authentication", statusCode: 401 },
    assertions: [{ id: "failed-access", result: "failed", expected: true, observed: false, traceRef: "failed-access" }],
    reportRef: null, reportDigest: null, source: "probe",
  };
  const trusted = trustedTestEvidence([...fresh, failure], { now }).records;
  assert.throws(() => resolve(trusted, 240_000), /credential|failed|protocol|qualified|verified|resolve/i);
});

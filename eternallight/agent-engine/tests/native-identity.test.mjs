import assert from "node:assert/strict";
import { test } from "node:test";
import { digest, stableJson } from "../src/contracts/capabilities.mjs";
import { nativeBinding } from "../src/native.mjs";
import { defineConnection } from "../src/connections/connection-spec.mjs";
import { verifiedBaselineEvidence } from "../src/testing/verified-evidence.mjs";

test("native canonical identity is stable across nested key order and JSON round trips", () => {
  const first = { b: [{ z: 1e-9, a: "字😀" }], a: false };
  const second = { a: false, b: [{ a: "字😀", z: 1e-9 }] };
  assert.equal(digest(first), digest(second));
  assert.equal(digest(first), digest(JSON.parse(stableJson(first))));
  assert.notEqual(digest({ a: null }), digest({}));
  assert.notEqual(digest([1, 2]), digest([2, 1]));
  assert.equal(stableJson({ absent: undefined, holes: [undefined] }), '{"holes":[null]}');
});

test("native evidence report import checks content digest before accepting records", () => {
  const connection = defineConnection({ id: "test", provider: "test", model: "test", endpoint: "https://example.invalid", credentialRef: "TEST_API_KEY", protocol: "chat-completions" });
  const seed = verifiedBaselineEvidence(connection)[0];
  const body = { records: [seed] };
  const reportId = digest(body);
  const records = [{ ...seed, reportRef: reportId, reportDigest: reportId }];
  const reports = { [reportId]: { ...body, reportId } };
  const owner = nativeBinding().NativeEvidenceSnapshot.fromReports(JSON.stringify({ records, reports }));
  const snapshot = JSON.parse(owner.apply("snapshot", '{"createdAt":"2026-09-21T00:00:00.000Z"}'));
  snapshot.records[0].status = "failed";
  assert.equal(JSON.parse(owner.apply("snapshot", '{"createdAt":"2026-09-21T00:00:00.000Z"}')).records[0].status, "verified");
  reports[reportId].tampered = true;
  assert.throws(() => nativeBinding().NativeEvidenceSnapshot.fromReports(JSON.stringify({ records, reports })), /digest mismatch/);
});

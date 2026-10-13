import test from "node:test";
import assert from "node:assert/strict";
import { modelFailureDiagnostic } from "../model-failure-diagnostic.mjs";

test("failure diagnostics preserve correlation and redact before truncating", () => {
  const secret = "fixture-provider-secret";
  const result = modelFailureDiagnostic({ status: "failed", runId: "run-one", requestId: "request-two",
    errorCode: "provider_http_error", errorStatusCode: 503, retryable: true,
    errorMessage: `${"文".repeat(995)}${secret} private-bridge-token`,
    tools: [{ arguments: "private manuscript" }], headers: { authorization: secret } }, [secret, "private-bridge-token"]);
  assert.equal(result.runId, "run-one");
  assert.equal(result.requestId, "request-two");
  assert.equal(result.errorCode, "provider_http_error");
  assert.equal(result.errorStatusCode, 503);
  assert.equal(Array.from(result.errorMessage).length, 1000);
  assert.ok(!JSON.stringify(result).includes("fixture"));
  assert.ok(!JSON.stringify(result).includes("private"));
  assert.ok(!Object.hasOwn(result, "tools"));
  assert.ok(!Object.hasOwn(result, "headers"));
  assert.equal(modelFailureDiagnostic({ status: "completed" }), null);
});

test("each credential is removed and non-string diagnostic values are discarded", () => {
  const result = modelFailureDiagnostic({ status: "failed", errorMessage: "provider-secret bridge-secret", errorCode: { private: "value" }, errorStatusCode: "503" }, ["provider-secret", "bridge-secret"]);
  assert.equal(result.errorMessage, "[REDACTED] [REDACTED]");
  assert.equal(result.errorCode, null);
  assert.equal(result.errorStatusCode, null);
});

test("capacity diagnostics expose only consistent counts from local admission", () => {
  const contextCapacity = { boundary: "route", estimatedInputTokens: 101, inputLimitTokens: 100, excessTokens: 1, input: "private manuscript" };
  const observation = { status: "failed", errorCode: "context_input_budget_exceeded", contextCapacity };
  assert.deepEqual(modelFailureDiagnostic(observation).contextCapacity,
    { boundary: "route", estimatedInputTokens: 101, inputLimitTokens: 100, excessTokens: 1 });
  assert.equal(modelFailureDiagnostic({ ...observation, errorCode: "provider_http_error" }).contextCapacity, null);
  for (const change of [{ excessTokens: 2 }, { boundary: "private" }, { estimatedInputTokens: Infinity }, { excessTokens: -1 }]) {
    assert.equal(modelFailureDiagnostic({ ...observation, contextCapacity: { ...contextCapacity, ...change } }).contextCapacity, null);
  }
});

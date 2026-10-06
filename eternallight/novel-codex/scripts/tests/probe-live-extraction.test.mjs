import assert from "node:assert/strict";
import { test } from "node:test";
import { extractionPlan, observeExtraction, finalizeProbeStage, probeLiveExtraction, sourceWasDelivered, toolSummary } from "../probe-live-extraction.mjs";

const options = { scope: "single-sample-conversation-extraction", maxRequests: 12, maxTokens: 100000,
  timeoutSeconds: 240, evidencePath: "/missing/evidence", credentialSaltPath: "/missing/salt" };

test("extraction probe uses one bounded Work and gates real execution before credential access", async () => {
  assert.deepEqual(extractionPlan(options).sharedWorkLimits, { requests: 12, tokens: 100000, seconds: 240 });
  for (const patch of [{ maxRequests: 13 }, { maxTokens: 100001 }, { timeoutSeconds: 241 }, { maxRequests: 1.5 }, { scope: "all-projects" }]) {
    assert.throws(() => extractionPlan({ ...options, ...patch }));
  }
  await assert.rejects(probeLiveExtraction(options), { message: "EXECUTION_REQUIRES_MAIN_AUTHORIZATION" });
  await assert.rejects(probeLiveExtraction({ ...options, execute: true, confirmCost: "secret" }), { message: "COST_CONFIRMATION_REQUIRED" });
});

test("actual MCP receipt summary retains counts without credentials, prose, arguments or reasoning", () => {
  const secret = "private-value";
  const item = { tool: "mcp__novel_workspace__submit_commitment_extraction", status: "completed", arguments: secret,
    reasoning: secret, result: { content: [{ type: "text", text: JSON.stringify({ candidateCount: 3, privateValue: secret }) }] } };
  assert.deepEqual(toolSummary(item), { name: "submit_commitment_extraction", completed: true, failure: null, candidateCount: 3 });
  assert.deepEqual(toolSummary({ ...item, result: { isError: true, error: { message: secret } } }),
    { name: "submit_commitment_extraction", completed: false, failure: null, candidateCount: null });
  assert.deepEqual(toolSummary({ tool: secret, status: "failed", error: secret, result: secret }),
    { name: "other", completed: false, failure: null, candidateCount: null });
});

test("completed read_context results prove the exact whole source in either current text representation", () => {
  const source = { objectId: "draft-chapter-one", revisionId: "source-one", body: "午夜，来信。\n\n---\n\n钟响了🔔。" };
  const draft = { objectId: source.objectId, sourceRevisionId: source.revisionId,
    content: { body: source.body }, textRepresentation: "body" };
  const result = value => ({ content: [{ type: "text", text: JSON.stringify({ relevantText: [value] }) }] });
  const item = { tool: "mcp__novel_workspace__read_context", status: "completed", result: result(draft) };
  assert.equal(sourceWasDelivered(item, source), true);
  const projected = { ...draft, content: { sceneGroups: [
    { id: "first", body: " 午夜，来信。 ", locked: false },
    { id: "last", body: "钟响了🔔。", locked: true },
  ] }, textRepresentation: "sceneGroups" };
  assert.equal(sourceWasDelivered({ ...item, result: result(projected) }, source), true);
  for (const changed of [
    { ...draft, sourceRevisionId: "later-source" }, { ...draft, objectId: "draft-other" },
    { ...draft, content: { body: "只有节选" } },
    { ...projected, content: { sceneGroups: projected.content.sceneGroups.slice(0, 1) } },
    { ...projected, content: { sceneGroups: [{ body: null }] } },
  ]) assert.equal(sourceWasDelivered({ ...item, result: result(changed) }, source), false);
  assert.equal(sourceWasDelivered({ ...item, status: "failed" }, source), false);
  assert.equal(sourceWasDelivered({ ...item, result: { ...result(draft), isError: true } }, source), false);
  assert.equal(sourceWasDelivered({ ...item, tool: "mcp__novel_workspace__resolve_story_evidence" }, source), false);
});

test("noncompleted model turns cannot be promoted by probe Host finalization", async () => {
  for (const status of ["failed", "interrupted", "inProgress", "private-provider-message"]) {
    const observation = {}, calls = [];
    await assert.rejects(finalizeProbeStage({ task: {}, result: { turn: { status } }, observation,
      runtimeRequest: async path => calls.push(path) }), { probeCode: "MODEL_TURN_NOT_COMPLETED" });
    assert.deepEqual(calls, []);
    assert.deepEqual(observation, { modelTurnStatus: status === "private-provider-message" ? "unknown" : status });
  }
});


test("extraction observations preserve budget diagnostics without private payloads and cap both stages together", () => {
  const report = { observations: [], observationsTruncated: false };
  const secret = "private-credential-and-model-prose";
  const value = { requestId: secret, status: "failed", errorCategory: "budget", errorCode: "task_token_budget_exhausted",
    errorStatusCode: 400, errorMessage: secret, arguments: secret, reasoning: secret, toolCalls: [{ name: secret }] };
  observeExtraction(report, "advisory", "response", value);
  assert.equal(report.observations[0].errorCode, "task_token_budget_exhausted");
  assert.equal(report.observations[0].operation, "advisory");
  observeExtraction(report, "extractCommitments", "response", { ...value, errorCode: secret, errorCategory: secret });
  assert.equal(report.observations[1].errorCode, null);
  assert.equal(report.observations[1].errorCategory, null);
  observeExtraction(report, secret, "response", value);
  observeExtraction(report, "advisory", secret, value);
  assert.equal(report.observations.length, 2);
  for (let i = 0; i < 30; i++) observeExtraction(report, "extractCommitments", "request", value);
  const saved = structuredClone(report.observations);
  assert.equal(report.observationsTruncated, false);
  observeExtraction(report, "advisory", "response", value);
  assert.deepEqual(report, { observations: saved, observationsTruncated: true });
  assert.equal(JSON.stringify(report).includes(secret), false);
});


test("failed MCP queries retain only fixed field and Runtime classifications", () => {
  const summarize = (result, tool = "query_story") => toolSummary({ tool, status: "completed", result });
  for (const [path, field] of [["queryPlan.steps[2].operation.limit", "steps[].operation.limit"],
    ["plan.ranking", "ranking"], ["plan.private-source-id", null]]) {
    const feedback = { isError: true, content: [{ type: "text", text: JSON.stringify({
      code: "invalid_query_plan", path, expected: "private-source-id",
    }) }] };
    const summary = summarize(feedback);
    assert.deepEqual(summary.failure, { code: "invalid_query_plan", field });
    assert.equal(JSON.stringify(summary).includes("private-source-id"), false);
  }
  const runtime = { isError: true, content: [{ type: "text", text: JSON.stringify({ error: {
    code: "invalid_request", message: 'InvalidContent("invalid query operation")' } }) }] };
  assert.deepEqual(summarize(runtime).failure, { code: "query_operation_invalid", field: null });
  for (const message of ["private-source-id", "plan.steps[private].id exceeds 32 UTF-8 bytes",
    "plan.question exceeds 1000 UTF-8 bytes: private-source-id"]) {
    const result = summarize({ isError: true, content: [{ type: "text", text: message }] });
    assert.deepEqual(result.failure, { code: "unclassified", field: null });
    assert.equal(JSON.stringify(result).includes("private"), false);
  }
  assert.equal(summarize({ content: [{ type: "text", text: JSON.stringify({ code: "invalid_query_plan", path: "plan.question" }) }] }).failure, null, "Successful content is not an error");
});

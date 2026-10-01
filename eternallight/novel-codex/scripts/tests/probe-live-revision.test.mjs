import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { CodexAppServerAdapter } from "../../electron/codex-adapter.mjs";
import { assertFeedbackIsolation, assertRevisionScope, closeProbeHosts, parseRevisionOptions, probeLiveRevision, revisionPlan,
  summarizeFeedback, summarizeObservation, summarizeToolResult } from "../probe-live-revision.mjs";

const options = { scope: "single-sample-revision-model-feedback", maxRequests: 8, maxTokens: 180000,
  timeoutSeconds: 180, evidencePath: "/missing/evidence", credentialSaltPath: "/missing/salt" };
const script = fileURLToPath(new URL("../probe-live-revision.mjs", import.meta.url));
const argv = ["--scope", options.scope, "--max-requests", "8", "--max-tokens", "180000", "--timeout-seconds", "180",
  "--evidence", options.evidencePath, "--credential-salt", options.credentialSaltPath];

test("modeling validation diagnostics retain only exact safe categories", () => {
  for (const [message, expected] of [
    ['InvalidContent("entity candidate is invalid")', "entity_candidate_invalid"],
    ['InvalidContent("knowledge candidate identity or shape is invalid")', "knowledge_candidate_invalid"],
    ['InvalidContent("too many knowledge or entity candidates")', "knowledge_item_limit"],
    ['InvalidContent("entity candidate is invalid private-value")', "invalid_request"],
  ]) {
    const report = summarizeToolResult({tool:"publish_knowledge_batch",status:"failed",
      result:{error:{code:"invalid_request",message}}},"source");
    assert.equal(report.errorCode,expected);
    assert.ok(!JSON.stringify(report).includes("private-value"));
  }
});

test("probe teardown accepts the real synchronous adapter and attempts every host after failure", async () => {
  const adapter = new CodexAppServerAdapter({ command: [process.execPath], cwd: process.cwd(), env: {} });
  const record = {};
  await closeProbeHosts({ bridge: { close: async () => {} }, adapter }, record);
  assert.deepEqual(record, {});
  assert.equal(adapter.process, null);
  const calls = [];
  const failed = {};
  await closeProbeHosts({ bridge: { close: () => { calls.push("bridge"); throw new Error("private diagnostic"); } },
    adapter: { finishTaskOperation: async () => { calls.push("adapter-stop"); throw new Error("private diagnostic"); },
      close: async () => { calls.push("adapter"); throw new Error("private diagnostic"); } } }, failed);
  assert.deepEqual(calls, ["bridge", "adapter-stop", "adapter"]);
  assert.deepEqual(failed, { shutdownFailed: true });
  await closeProbeHosts({}, {});
});

test("parse and budget validation reject malformed, duplicated and unbounded inputs", () => {
  assert.deepEqual(parseRevisionOptions(argv), options);
  for (const args of [["--api-key", "private-key"], ["--execute", "--execute"], ["--max-requests"],
    ["--max-requests", "1e3"], ["--max-tokens", "-1"], ["--max-tokens", "0"]]) {
    assert.throws(() => parseRevisionOptions(args));
  }
  for (const patch of [{ scope: "full-journey" }, { maxRequests: 11 }, { maxTokens: 260001 },
    { timeoutSeconds: Infinity }, { timeoutSeconds: 181 }, { maxTokens: 1.5 }, { evidencePath: "" }]) {
    assert.throws(() => revisionPlan({ ...options, ...patch }));
  }
  const smaller = revisionPlan({ ...options, maxRequests: 2, maxTokens: 1000, timeoutSeconds: 10 });
  assert.deepEqual(smaller.maximumTotal, { requests: 6, tokens: 3000 });
  assert.notEqual(smaller.costConfirmation, revisionPlan(options).costConfirmation);
});

test("programmatic authorization and exact cost gates precede reads of nonexistent evidence", async () => {
  await assert.rejects(probeLiveRevision(options), { message: "EXECUTION_REQUIRES_MAIN_AUTHORIZATION" });
  for (const confirmCost of [undefined, true, "yes", revisionPlan({ ...options, maxRequests: 7 }).costConfirmation]) {
    await assert.rejects(probeLiveRevision({ ...options, execute: true, confirmCost }), { message: "COST_CONFIRMATION_REQUIRED" });
  }
});

test("CLI plan and blocked execution never access credentials and never echo user secrets", () => {
  const help = spawnSync(process.execPath, [script, "--help"], { encoding: "utf8" });
  assert.equal(help.status, 0);
  assert.match(help.stdout, /Developer-only source checkout; requires electron\/tests\/helpers\/mcp-startup\.mjs/);
  const plan = spawnSync(process.execPath, [script, ...argv], { encoding: "utf8" });
  assert.equal(plan.status, 0, plan.stderr);
  assert.deepEqual(JSON.parse(plan.stdout), revisionPlan(options));
  const secret = "private-key-without-sk-prefix";
  const denied = spawnSync(process.execPath, [script, ...argv, "--execute", "--confirm-cost", secret], { encoding: "utf8" });
  assert.equal(denied.status, 2);
  assert.deepEqual(JSON.parse(denied.stderr), { status: "blocked", code: "COST_CONFIRMATION_REQUIRED" });
  assert.ok(!(denied.stdout + denied.stderr).includes(secret));
});

test("observation whitelist omits arbitrary credentials, arguments, reasoning and vendor errors", () => {
  const secret = "not-a-standard-key-secret";
  const result = summarizeObservation({ requestId: secret, status: "failed", errorCategory: "authentication", errorStatusCode: 401,
    toolCalls: [{ name: "read_context", arguments: secret }, { name: secret }], errorMessage: secret,
    reasoning: secret, headers: { authorization: secret }, usage: secret }, "response");
  assert.deepEqual({ ...result, requestRef: "digest" }, { kind: "response", requestRef: "digest", status: "failed",
    connectionMatches: false, modelMatches: false, errorCategory: "authentication", errorCode: null, httpStatus: 401, tools: ["read_context", "other"] });
  assert.ok(!JSON.stringify(result).includes(secret));
  assert.equal(summarizeObservation({ errorStatusCode: -1, toolCalls: secret }, "response").httpStatus, null);
  assert.equal(summarizeObservation({ connectionId: "deepseek-flash", model: "deepseek-flash" }, "request").modelMatches, true);
  assert.equal(summarizeObservation({ logicalModel: "deepseek-flash" }, "response").modelMatches, true);
  assert.equal(summarizeObservation({ errorCategory: "budget", errorCode: "task_token_budget_exhausted" }, "response").errorCode, "task_token_budget_exhausted");
  assert.equal(summarizeObservation({ errorCode: secret }, "response").errorCode, null);
  assert.equal(summarizeToolResult({ status: "failed" }, null).error, true);
});

test("tool result requires fresh structured candidate evidence and bounded traversal", () => {
  const item = { tool: "query_story", status: "completed", result: { content: [{ type: "text", text: JSON.stringify({
    steps: [{ results: [{ sourceRevisionId: "fresh", confirmationState: "candidate", assertion: { evidenceRef: "anchor" } }] }],
    evidence: { anchor: { sourceRevisionId: "fresh", quote: "她藏信。", privateReasoning: "secret" } } }) }] } };
  assert.deepEqual(summarizeToolResult(item, "fresh"), { name: "query_story", status: "completed", error: false,
    errorCode: null, sourceSeen: true, candidateSeen: true, bounded: true });
  assert.equal(summarizeToolResult({ ...item, status: "failed", result: { content: [{ type: "text", text: JSON.stringify({
    error: { code: "invalid_request", message: 'InvalidContent("evidence_anchor_mismatch")' } }) }] } }, "fresh").errorCode, "evidence_anchor_mismatch");
  assert.equal(summarizeToolResult(item, "old").candidateSeen, false);
  const separated = { ...item, result: { steps: [{ results: [{ sourceRevisionId: "fresh", confirmationState: "candidate", assertion: { evidenceRef: "anchor" } }] }],
    evidence: { anchor: { sourceRevisionId: "old", quote: "旧稿" } } } };
  assert.equal(summarizeToolResult(separated, "fresh").candidateSeen, false);
  assert.equal(summarizeToolResult({ ...item, result: "x".repeat(65537) }, "fresh").bounded, false);
  assert.equal(summarizeToolResult({ ...item, result: Array.from({ length: 65 }, () => ({})) }, "fresh").bounded, false);
  assert.ok(!JSON.stringify(summarizeToolResult(item, "fresh")).includes("secret"));
});

test("revision scope catches protected scene, metadata, ordering and body inconsistencies", () => {
  const before = { title: "密函", sceneGroups: [{ id: "letter", body: "她藏信。", locked: false }, { id: "end", body: "塔灭。", locked: true }] };
  const after = { ...before, sceneGroups: [{ ...before.sceneGroups[0], body: "她藏信，准备留下证词。" }, before.sceneGroups[1]], body: "她藏信，准备留下证词。\n\n---\n\n塔灭。" };
  assert.equal(assertRevisionScope(before, after).scopePreserved, true);
  for (const mutate of [draft => { draft.sceneGroups[1].body = "塔亮。"; }, draft => { draft.sceneGroups.reverse(); },
    draft => { draft.sceneGroups[1].locked = false; }, draft => { draft.body = "不一致"; }, draft => { draft.title = "另一章"; }]) {
    const changed = structuredClone(after); mutate(changed);
    assert.throws(() => assertRevisionScope(before, changed));
  }
});

test("persisted goal judgments bind new source and exact Unicode evidence without parsing final prose", () => {
  const draft = { id: "fresh", sceneGroups: [{ id: "letter", body: "🌙她藏信，准备作证。" }] };
  const feedback = { authority: "candidateAnalysis", state: "current", assessment: { draftRevisionId: "fresh", outcomes: [
    { id: "retain-letter", status: "supported", evidence: [{ groupId: "letter", quote: "她藏信", start: 1, end: 4 }] },
    { id: "testify-motive", status: "insufficientEvidence", evidence: [], reason: "private-key" }] } };
  const result = summarizeFeedback(feedback, draft);
  assert.deepEqual(result.goals.map(({ id, status }) => ({ id, status })), [
    { id: "retain-letter", status: "supported" }, { id: "testify-motive", status: "insufficientEvidence" }]);
  assert.ok(!JSON.stringify(result).includes("private-key"));
  assert.throws(() => summarizeFeedback("private-key", draft), { message: "FEEDBACK_SOURCE_MISMATCH" });
  for (const mutate of [value => { value.assessment.draftRevisionId = "stale"; }, value => { value.authority = "confirmed"; },
    value => { value.assessment.outcomes[0].evidence[0].start = 2; },
    value => { value.assessment.outcomes[0].evidence[0].quote = "她烧信"; },
    value => { value.assessment.outcomes.push(value.assessment.outcomes[0]); }]) {
    const changed = structuredClone(feedback); mutate(changed);
    assert.throws(() => summarizeFeedback(changed, draft));
  }
});

test("candidate feedback may change aggregate digest but never chapters, Canon revision or knowledge", () => {
  const before = { revision: 0, chapters: [{ candidate: { id: "draft", body: "🌙她藏信" }, accepted: null }], sourceDigest: "before-modeling" };
  const after = { ...structuredClone(before), sourceDigest: "after-candidate-report" };
  const knowledge = { sources: [{ sourceRevisionId: "draft", confirmationState: "candidate" }] };
  assertFeedbackIsolation(before, after, knowledge, structuredClone(knowledge));
  for (const mutate of [value => { value.chapters[0].candidate.body = "她烧信"; },
    value => { value.chapters[0].accepted = value.chapters[0].candidate; }, value => { value.revision++; }]) {
    const changed = structuredClone(after); mutate(changed);
    assert.throws(() => assertFeedbackIsolation(before, changed, knowledge, knowledge));
  }
  assert.throws(() => assertFeedbackIsolation(before, after, knowledge, { sources: [] }));
});

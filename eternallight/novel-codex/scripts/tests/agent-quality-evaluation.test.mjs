import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { attributeOverhead } from "../agent-overhead-report.mjs";
import { digest, prepareBlindReview, assessQuality, checkRun, qualityDimensions } from "../creative-quality-evaluation.mjs";
const suite = JSON.parse(await readFile(new URL("../fixtures/creative-quality-suite.json", import.meta.url), "utf8"));
const ledger = { native: { story_work_steps_v1: [{ id: "s", operation: "advisory", work_task_id: "w" }], story_work_attempts_v1: [{ attempt_id: "r", step_id: "s" }] }, engine: [{ run_id: "r", state_json: { requests: 2, tokens: 100, unknownTokens: 20 } }] };
const event = (type, id, fields = {}) => ({ session: "s", payload: { type, call_id: id, ...fields } });
test("attribution keeps unknown usage separate and counts duplicate/repeated/partial traces", () => {
  const call = id => event("function_call", id, { name: "read_context", arguments: '{"expectedRevision":0}' });
  const output = id => event("function_call_output", id, { output: "same" });
  const report = attributeOverhead([call("a"), output("a"), call("a"), call("b"), output("b"), call("c"), output("orphan")], ledger, v => v.length);
  assert.deepEqual(report.providerByOperation.advisory, { requests: 2, knownTokens: 100, unknownReservedTokens: 20, attempts: 1 });
  assert.equal(report.coverage.duplicateEvents, 1); assert.equal(report.coverage.missingOutputs, 1); assert.equal(report.coverage.orphanOutputs, 1);
  assert.equal(report.toolTotals.read_context.identicalRepeatOutputTokens, 4);
  assert.throws(() => attributeOverhead([], { ...ledger, engine: [...ledger.engine, ...ledger.engine] }, v => v.length), /double count/);
});

function run(c, variant) {
  const output = c.kind === "analysis" ? { report: "存在不确定性，需要原文证据。", sourceAfter: c.source } : structuredClone(c.source);
  if (c.kind !== "analysis") output.groups.find(g => g.id === "edit").body += "她停了一下。";
  return { caseId: c.id, variant, sourceDigest: digest(c), status: "completed", output,
    receipt: { taskId: "test-task", taskStatus: "completed", outputRevisionId: "test-output" },
    provenance: { sourceCommit: "test", model: "test", configurationDigest: "test", evidenceSha256: "test" }, usage: { tokens: variant === "baseline" ? 100 : 1 } };
}
function bundle() {
  return prepareBlindReview(suite, suite.cases.flatMap(c => [run(c, "baseline"), run(c, "candidate")]), () => 0);
}
function ratings(b) {
  const reviews = structuredClone(b.template); reviews.reviewerId = "synthetic-test-only";
  for (const r of reviews.reviews) {
    r.verdict = "tie"; r.reason = "仅验证评测程序，不是真实质量评分。";
    r.evidence = [{ side: "A", quote: b.packet.pairs.find(p => p.caseId === r.caseId).A.slice(0, 4) }];
    for (const side of ["A", "B"]) r[side] = { satisfactory: true, scores: Object.fromEntries(qualityDimensions.map(d => [d, 4])) };
  }
  return reviews;
}
test("blind packet excludes variant, model, costs; templates cannot pass as human evidence", () => {
  const b = bundle(); assert.equal(b.packet.pairs.length, 8);
  const encoded = JSON.stringify(b.packet); assert.ok(!encoded.includes('"variant"') && !encoded.includes('"usage"') && !encoded.includes('"provenance"'));
  assert.equal(assessQuality(b.packet, b.key, b.template).status, "quality_evidence_pending");
  const llm = ratings(b); llm.reviewerKind = "llm";
  assert.equal(assessQuality(b.packet, b.key, llm).status, "quality_evidence_pending");
});
test("cheaper candidate cannot compensate for literary regression or unauthorized changes", () => {
  const b = bundle(), r = ratings(b); r.reviews[0].B.scores.voice = 2;
  assert.equal(assessQuality(b.packet, b.key, r).status, "rejected");
  const c = suite.cases[0], edited = run(c, "candidate"); edited.output.groups[0].body += "侵入修改";
  assert.ok(checkRun(c, edited).includes("protected_text_changed:before"));
  edited.sourceDigest = "stale"; assert.ok(checkRun(c, edited).includes("source_or_rubric_mismatch"));
});
test("failures remain in denominator and incomplete, tampered or ambiguous reviews never pass", () => {
  const b = bundle(); b.key.pairs[0].sides.A.failures.push("not_delivered");
  assert.equal(assessQuality(b.packet, b.key, ratings(b)).status, "rejected");
  const fresh = bundle(), r = ratings(fresh); r.reviews[0].verdict = "mixed";
  assert.equal(assessQuality(fresh.packet, fresh.key, r).status, "quality_evidence_pending");
  r.reviews[0].verdict = "tie"; r.reviews[0].evidence[0].quote = "不存在的引文";
  assert.equal(assessQuality(fresh.packet, fresh.key, r).status, "quality_evidence_pending");
  fresh.packet.pairs[0].A += "修改"; assert.throws(() => assessQuality(fresh.packet, fresh.key, r), /packet changed/);
});

test("missing runs and changed model settings cannot become valid pairs", () => {
  const runs = suite.cases.flatMap(c => [run(c, "baseline"), run(c, "candidate")]);
  runs[1].provenance.model = "different-model";
  let b = prepareBlindReview(suite, runs, () => 0);
  assert.equal(assessQuality(b.packet, b.key, ratings(b)).status, "rejected");
  b = prepareBlindReview(suite, runs.slice(1), () => 0);
  assert.ok(b.key.pairs[0].sides.A.failures.includes("missing_run"));
  assert.equal(assessQuality(b.packet, b.key, ratings(b)).status, "rejected");
  assert.throws(() => prepareBlindReview(suite, [...runs, runs[0]]), /duplicate run/);
});

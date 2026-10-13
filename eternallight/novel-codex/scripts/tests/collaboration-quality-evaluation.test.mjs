import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { collaborationPlan, prepareCollaborationReview } from "../collaboration-quality-evaluation.mjs";
import { digest, assessQuality } from "../creative-quality-evaluation.mjs";
const suite = JSON.parse(await readFile(new URL("../fixtures/creative-quality-suite.json", import.meta.url), "utf8"));
function runs() { return collaborationPlan(suite).runs.map(r => {
  const c = suite.cases.find(c => c.id === r.caseId), output = c.kind === "analysis" ? { report: "合成测试意见", sourceAfter: c.source } : structuredClone(c.source);
  if (c.kind !== "analysis") output.groups.find(g => g.id === "edit").body += "她没有说话。";
  return { ...r, status: "completed", output, sourceDigest: digest(c),
    receipt: { taskId: r.caseId + r.arm, taskStatus: "completed", outputRevisionId: "test" },
    provenance: { sourceCommit: "test", model: "test", configurationDigest: "test", evidenceSha256: "test" },
    workBudget: { maxRequests: 10, maxTokens: 100000, durationSeconds: 180 }, usage: null,
    conditionEvidence: { question: "是否保留了作者目标与原文有效部分？", rootThreadId: "root", evidenceSha256: "test", selfReviewRef: r.arm === "self_review" ? "note" : null,
      children: r.arm === "specialist" ? [{ threadId: "child", parentThreadId: "root", reportRef: "report", exposure: "suppliedTextOnly" }] : [] } };
}); }
test("unexecuted experiment keeps all 24 runs and all missing pairs", () => {
  const b = prepareCollaborationReview(suite, [], () => 0);
  assert.equal(b.expectedRuns, 24); assert.equal(b.suppliedRuns, 0);
  for (const c of b.comparisons) for (const p of c.key.pairs) for (const s of Object.values(p.sides)) assert.ok(s.failures.includes("missing_run"));
});
test("all three comparisons are blind, unrated and retain unknown cost", () => {
  const b = prepareCollaborationReview(suite, runs(), () => 0);
  assert.equal(b.comparisons.length, 3);
  for (const c of b.comparisons) {
    assert.equal(assessQuality(c.packet, c.key, c.template).status, "quality_evidence_pending");
    assert.ok(!JSON.stringify(c.packet).includes('"conditionEvidence"'));
    assert.ok(!JSON.stringify(c.packet).includes('"arm"'));
    assert.equal(c.key.pairs[0].sides.A.usage, null);
  }
});
test("fake collaboration, unequal budgets, build drift and missing self-check cannot pass", () => {
  const r = runs(); r[0].workBudget.maxTokens++; r[1].conditionEvidence.selfReviewRef = null;
  r[2].conditionEvidence.children[0].parentThreadId = "other"; r[2].provenance.sourceCommit = "other"; r[2].conditionEvidence.question = "另一个问题";
  const b = prepareCollaborationReview(suite, r, () => 0);
  const all = b.comparisons.flatMap(c => c.key.pairs.flatMap(p => Object.values(p.sides).flatMap(s => s.failures)));
  for (const reason of ["comparison_work_budget_mismatch", "missing_self_review", "missing_specialist_evidence", "comparison_build_mismatch", "comparison_investigation_question_mismatch"]) assert.ok(all.includes(reason), reason);
  assert.throws(() => prepareCollaborationReview(suite, [...r, r[0]]), /duplicate condition/);
});

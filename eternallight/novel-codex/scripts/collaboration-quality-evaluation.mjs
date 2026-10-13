// Offline experiment preparation. Production executions and authentic reviewer evidence
// are inputs, never synthesized here. Reuses the existing quality-first paired gate.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { digest, validateSuite, prepareBlindReview } from "./creative-quality-evaluation.mjs";
import { saveReport } from "./tool-evaluation-report.mjs";

export const collaborationArms = Object.freeze({
  solo: "单 Agent 完成原任务，不调用子 Agent；保留正常完成检查。",
  self_review: "单 Agent 完成后对一个明确创作疑难进行定向自检，比较修订代价；不调用子 Agent。",
  specialist: "主 Agent 将同一疑难交给隔离专家调查，核对证据后自行决定是否采用；记录实际材料暴露范围。",
});
export function collaborationPlan(suite) {
  validateSuite(suite);
  return { version: 1, suiteDigest: digest(suite), arms: collaborationArms,
    runs: suite.cases.flatMap(c => Object.keys(collaborationArms).map(arm => ({ caseId: c.id, arm, sourceDigest: digest(c), status: "not_started" }))),
    conditions: ["每个样本三组使用相同材料、作者目标、模型配置、来源读取义务和完整 Work 额度；额度包含子任务与综合。",
      "同一比较批次冻结代码与方法资源；运行顺序轮换。每个条件单独新任务，保留首次运行、失败和未知用量。",
      "conditionEvidence 引用真实执行记录：rootThreadId、children 的 threadId/parentThreadId、同一预先登记的 question、定向自检 selfReviewRef、专家 reportRef/exposure。不以角色提示词作为执行证据。",
      "只向评审提供 packet.json；key.json、条件名称、用量与模型信息单独保存。真人未评分则保持 pending。"],
    qualityGain: "unvalidated" };
}
function conditionFailures(run) {
  const e = run.conditionEvidence, failures = [];
  if (!e?.rootThreadId || !Array.isArray(e.children) || !e.evidenceSha256) return ["missing_condition_evidence"];
  if (run.arm !== "specialist" && e.children.length) failures.push("unexpected_delegation");
  if (run.arm !== "solo" && (typeof e.question !== "string" || !e.question.trim())) failures.push("missing_investigation_question");
  if (run.arm === "self_review" && !e.selfReviewRef) failures.push("missing_self_review");
  if (run.arm === "specialist" && (!e.children.length || e.children.some(c => !c.threadId || c.threadId === e.rootThreadId
      || c.parentThreadId !== e.rootThreadId || !c.reportRef || !c.exposure))) failures.push("missing_specialist_evidence");
  const b = run.workBudget;
  if (!b || !["maxRequests", "maxTokens", "durationSeconds"].every(k => Number.isSafeInteger(b[k]) && b[k] > 0)) failures.push("missing_bounded_work_budget");
  return failures;
}
export function prepareCollaborationReview(suite, runs, choose) {
  const plan = collaborationPlan(suite), indexed = new Map();
  for (const run of runs) {
    assert.ok(Object.hasOwn(collaborationArms, run.arm) && suite.cases.some(c => c.id === run.caseId), "unknown condition");
    const id = `${run.caseId}:${run.arm}`;
    assert.ok(!indexed.has(id), "duplicate condition; repeated trials require a separate batch"); indexed.set(id, run);
  }
  plan.runs = plan.runs.map(r => ({ ...r, status: indexed.get(`${r.caseId}:${r.arm}`)?.status ?? "not_started" }));
  const comparisons = [];
  for (const [baseline, candidate] of [["solo", "self_review"], ["solo", "specialist"], ["self_review", "specialist"]]) {
    const paired = runs.filter(r => [baseline, candidate].includes(r.arm)).map(r => ({ ...r, variant: r.arm === baseline ? "baseline" : "candidate" }));
    const bundle = prepareBlindReview(suite, paired, choose);
    for (const pair of bundle.key.pairs) {
      const left = indexed.get(`${pair.caseId}:${baseline}`), right = indexed.get(`${pair.caseId}:${candidate}`);
      const budgetMismatch = left && right && ["maxRequests", "maxTokens", "durationSeconds"].some(k => left.workBudget?.[k] !== right.workBudget?.[k]);
      const selfReview = indexed.get(`${pair.caseId}:self_review`), specialist = indexed.get(`${pair.caseId}:specialist`);
      const questionMismatch = selfReview && specialist && selfReview.conditionEvidence?.question !== specialist.conditionEvidence?.question;
      const buildMismatch = left && right && left.provenance?.sourceCommit !== right.provenance?.sourceCommit;
      for (const side of Object.values(pair.sides)) {
        const run = side.variant === "baseline" ? left : right;
        if (run) side.failures.push(...conditionFailures(run));
        if (budgetMismatch) side.failures.push("comparison_work_budget_mismatch");
        if (buildMismatch) side.failures.push("comparison_build_mismatch");
        if (questionMismatch) side.failures.push("comparison_investigation_question_mismatch");
      }
    }
    comparisons.push({ baseline, candidate, ...bundle });
  }
  return { plan, comparisons, expectedRuns: plan.runs.length, suppliedRuns: runs.length,
    limitations: ["三个比较重复使用同一组产物，不是三组独立实验；不得重复计算请求或费用。",
      "本程序校验记录一致性，不能认证手填证据的真实性；执行日志和真人身份需另核。",
      "模型评分不能代替真人读者；缺失、失败、条件不符与未知成本不构成质量收益证据。"] };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [suitePath, runsPath, directory] = process.argv.slice(2);
  assert.ok(suitePath && runsPath && directory, "suite.json runs.json output-directory (use [] for an unexecuted plan)");
  const result = prepareCollaborationReview(JSON.parse(await readFile(suitePath, "utf8")), JSON.parse(await readFile(runsPath, "utf8")));
  const { comparisons, ...manifest } = result;
  await saveReport(resolve(directory, "manifest.json"), manifest);
  // Opaque bundle directories avoid revealing the experiment arms to readers.
  for (let i = 0; i < comparisons.length; i++) {
    const { baseline, candidate, ...bundle } = comparisons[i];
    bundle.key.conditions = { baseline, candidate };
    for (const [name, value] of Object.entries(bundle)) await saveReport(resolve(directory, `comparison-${i + 1}`, `${name}.json`), value);
  }
}

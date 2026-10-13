// Offline quality gate. Runs must come from the production application; this is not an agent loop.
import assert from "node:assert/strict";
import { createHash, randomInt } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { saveReport } from "./tool-evaluation-report.mjs";

export const digest = value => createHash("sha256").update(JSON.stringify(value)).digest("hex");
export const qualityDimensions = ["intent", "continuity", "voice", "causality", "readingExperience"];
export function validateSuite(suite) {
  assert.equal(suite.version, 1); assert.ok(suite.cases.length >= 8);
  assert.equal(new Set(suite.cases.map(c => c.id)).size, suite.cases.length);
  for (const c of suite.cases) {
    assert.ok(c.id && c.authorIntent && c.rubric && c.source.groups.length);
    assert.ok(["revision", "continuation", "analysis"].includes(c.kind));
    assert.ok(c.protectedGroupIds.every(id => c.source.groups.some(g => g.id === id)));
    assert.ok(qualityDimensions.every(d => typeof c.rubric[d] === "string" && c.rubric[d].length > 0));
  }
  return suite;
}
export function checkRun(c, run) {
  const failures = [];
  if (!run) return ["missing_run"];
  if (run.sourceDigest !== digest(c)) failures.push("source_or_rubric_mismatch");
  if (run.status !== "completed") failures.push("not_delivered");
  if (!run.receipt?.taskId || run.receipt.taskStatus !== "completed" || !run.receipt.outputRevisionId) failures.push("missing_domain_receipt");
  if (!run.provenance?.sourceCommit || !run.provenance?.model || !run.provenance?.configurationDigest || !run.provenance?.evidenceSha256) failures.push("missing_provenance");
  if (c.kind === "analysis") {
    if (typeof run.output?.report !== "string" || !run.output.report.trim()) failures.push("missing_analysis");
    if (JSON.stringify(run.output?.sourceAfter) !== JSON.stringify(c.source)) failures.push("analysis_modified_source");
  } else {
    const output = run.output;
    if (!Array.isArray(output?.groups)) return [...failures, "missing_prose"];
    if (output.title !== c.source.title) failures.push("title_changed");
    if (JSON.stringify(output.groups.map(g => g.id)) !== JSON.stringify(c.source.groups.map(g => g.id))) failures.push("group_identity_or_order_changed");
    for (const g of c.source.groups) {
      const after = output.groups.find(v => v.id === g.id);
      if (!after || after.locked !== g.locked) failures.push(`metadata_changed:${g.id}`);
      if ((g.locked || c.protectedGroupIds.includes(g.id)) && after?.body !== g.body) failures.push(`protected_text_changed:${g.id}`);
    }
    if (output.groups.some(g => typeof g.body !== "string")) failures.push("invalid_body");
    if (JSON.stringify(output) === JSON.stringify(c.source)) failures.push("no_revision");
  }
  return failures;
}
const presented = output => output?.report ?? output?.groups?.map(g => g.body).join("\n\n") ?? "（未交付）";

export function prepareBlindReview(suite, runs, choose = () => randomInt(2)) {
  validateSuite(suite);
  const indexed = new Map();
  for (const r of runs) {
    assert.ok(suite.cases.some(c => c.id === r.caseId) && ["baseline", "candidate"].includes(r.variant), "unknown case or variant");
    const key = `${r.caseId}:${r.variant}`; assert.ok(!indexed.has(key), "duplicate run; use a separate paired batch for repeats"); indexed.set(key, r);
  }
  const key = { suiteDigest: digest(suite), pairs: [] }, packet = { version: 1, suiteDigest: digest(suite), instructions: suite.reviewInstructions, pairs: [] };
  for (const c of suite.cases) {
    const variants = choose() ? ["candidate", "baseline"] : ["baseline", "candidate"];
    const pair = { caseId: c.id, authorIntent: c.authorIntent, source: c.source, rubric: c.rubric };
    const mapping = { caseId: c.id, sides: {} };
    const baseline = indexed.get(`${c.id}:baseline`), candidate = indexed.get(`${c.id}:candidate`);
    const comparisonFailures = [];
    if (baseline && candidate) {
      for (const field of ["model", "configurationDigest"])
        if (baseline.provenance?.[field] !== candidate.provenance?.[field]) comparisonFailures.push(`comparison_${field}_mismatch`);
    }
    for (let i = 0; i < 2; i++) {
      const side = i === 0 ? "A" : "B", variant = variants[i], run = indexed.get(`${c.id}:${variant}`);
      pair[side] = presented(run?.output);
      mapping.sides[side] = { variant, runDigest: run ? digest(run) : null, failures: [...checkRun(c, run), ...comparisonFailures], usage: run?.usage ?? null };
    }
    packet.pairs.push(pair); key.pairs.push(mapping);
  }
  key.packetDigest = digest(packet);
  const template = { packetDigest: key.packetDigest, reviewerKind: "human", reviewerId: "", reviews: packet.pairs.map(p => ({ caseId: p.caseId,
    verdict: "unrated", A: { satisfactory: null, scores: {} }, B: { satisfactory: null, scores: {} }, reason: "", evidence: [] })) };
  return { packet, key, template };
}

export function assessQuality(packet, key, reviews) {
  assert.equal(key.packetDigest, digest(packet), "blind packet changed");
  assert.equal(key.suiteDigest, packet.suiteDigest);
  assert.equal(reviews.packetDigest, key.packetDigest, "ratings belong to another packet");
  const failures = [], pending = [], comparisons = [], seen = new Set();
  for (const r of reviews.reviews) {
    assert.ok(!seen.has(r.caseId) && packet.pairs.some(p => p.caseId === r.caseId), "duplicate or unknown review"); seen.add(r.caseId);
  }
  for (const pair of packet.pairs) {
    const mapping = key.pairs.find(p => p.caseId === pair.caseId); assert.ok(mapping);
    assert.deepEqual(Object.values(mapping.sides).map(s => s.variant).sort(), ["baseline", "candidate"]);
    for (const [side, run] of Object.entries(mapping.sides)) for (const failure of run.failures) failures.push(`${pair.caseId}:${run.variant}:${failure}`);
    const r = reviews.reviews.find(r => r.caseId === pair.caseId);
    const rated = reviews.reviewerKind === "human" && !!reviews.reviewerId?.trim() && r && ["A", "B", "tie", "mixed", "uncertain"].includes(r.verdict)
      && r.reason?.trim() && r.evidence?.length && r.evidence.every(e => ["A", "B"].includes(e.side) && e.quote?.length && pair[e.side].includes(e.quote))
      && ["A", "B"].every(side => typeof r[side]?.satisfactory === "boolean" && qualityDimensions.every(d => Number.isInteger(r[side].scores?.[d]) && r[side].scores[d] >= 1 && r[side].scores[d] <= 5));
    if (!rated || r.verdict === "uncertain" || r.verdict === "mixed") { pending.push(pair.caseId); continue; }
    const candidate = Object.keys(mapping.sides).find(side => mapping.sides[side].variant === "candidate"), baseline = candidate === "A" ? "B" : "A";
    if (!r[baseline].satisfactory || !r[candidate].satisfactory) failures.push(`${pair.caseId}:unsatisfactory_pair`);
    if (r.verdict === baseline || qualityDimensions.some(d => r[candidate].scores[d] < r[baseline].scores[d])) failures.push(`${pair.caseId}:quality_regression`);
    comparisons.push({ caseId: pair.caseId, verdict: r.verdict === "tie" ? "tie" : mapping.sides[r.verdict].variant });
  }
  return { version: 1, status: failures.length ? "rejected" : pending.length || comparisons.length < 8 ? "quality_evidence_pending" : "supports_limited_rollout",
    failures, pending, comparisons, costs: key.pairs.map(p => ({ caseId: p.caseId, runs: Object.values(p.sides).map(s => ({ variant: s.variant, usage: s.usage })) })),
    limitations: ["Human identity and production receipts require evidence review; local JSON cannot authenticate their authors.",
      "Eight pairs support a limited product decision, not statistical equivalence or universal literary improvement.",
      "Costs never offset a quality regression; missing or failed runs remain in the denominator."] };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...paths] = process.argv.slice(2);
  const read = async path => JSON.parse(await readFile(path, "utf8"));
  if (command === "prepare") {
    assert.equal(paths.length, 3, "prepare suite.json runs.json output-directory");
    const bundle = prepareBlindReview(await read(paths[0]), await read(paths[1]));
    for (const [name, value] of Object.entries(bundle)) await saveReport(resolve(paths[2], `${name}.json`), value);
  } else if (command === "assess") {
    assert.equal(paths.length, 4, "assess packet.json key.json completed-reviews.json report.json");
    const report = assessQuality(...await Promise.all(paths.slice(0, 3).map(read))); await saveReport(paths[3], report);
    console.log(JSON.stringify({ status: report.status, failures: report.failures, pending: report.pending }));
    if (report.status !== "supports_limited_rollout") process.exitCode = 1;
  } else throw Error("Use prepare or assess; no model requests are made by this tool.");
}

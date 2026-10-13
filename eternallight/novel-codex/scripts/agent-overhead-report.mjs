// Offline observation only. Never treat local token estimates as provider billing.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve, basename } from "node:path";
import { fileURLToPath } from "node:url";
import { getEncoding } from "js-tiktoken";
import { saveReport } from "./tool-evaluation-report.mjs";

const hash = value => createHash("sha256").update(value).digest("hex");
const canonical = value => JSON.stringify(value && typeof value === "object"
  ? Array.isArray(value) ? value.map(v => JSON.parse(canonical(v)))
    : Object.fromEntries(Object.keys(value).sort().map(k => [k, JSON.parse(canonical(value[k]))])) : value);
const parsed = value => typeof value === "string" ? JSON.parse(value) : value;
function outputText(output) {
  if (typeof output === "string") return output;
  assert.ok(Array.isArray(output), "unsupported trace output");
  return output.filter(v => typeof v.text === "string" && !/^Wall time: .*\nOutput:$/.test(v.text)).map(v => v.text).join("\n");
}
const count = value => { assert.ok(Number.isSafeInteger(value) && value >= 0, "invalid usage counter"); return value; };

export function attributeOverhead(trace, ledgers, tokenCount) {
  assert.ok(Array.isArray(trace) && Array.isArray(ledgers.engine), "trace and Engine ledger are required");
  const steps = new Map(ledgers.native.story_work_steps_v1.map(s => [s.id, s]));
  const attempts = new Map(ledgers.native.story_work_attempts_v1.map(a => [a.attempt_id, a]));
  const providerByOperation = {}, providerByWork = {}, seenRuns = new Set();
  const addUsage = (map, key, usage) => {
    const row = map[key] ??= { requests: 0, knownTokens: 0, unknownReservedTokens: 0, attempts: 0 };
    row.requests += count(usage.requests); row.knownTokens += count(usage.tokens);
    row.unknownReservedTokens += count(usage.unknownTokens); row.attempts++;
  };
  for (const row of ledgers.engine) {
    assert.ok(!seenRuns.has(row.run_id), "duplicate Engine run would double count usage"); seenRuns.add(row.run_id);
    const usage = parsed(row.state_json), step = steps.get(attempts.get(row.run_id)?.step_id);
    addUsage(providerByOperation, step?.operation ?? "unmapped", usage);
    addUsage(providerByWork, step?.work_task_id ?? "unmapped", usage);
  }
  const calls = new Map(), outputs = new Map(), sessions = new Set(), toolTotals = {};
  let duplicates = 0;
  for (const event of trace) {
    const p = event.payload;
    if (!["function_call", "function_call_output"].includes(p?.type)) continue;
    assert.ok(typeof event.session === "string" && typeof p.call_id === "string", "trace identity missing");
    sessions.add(event.session);
    const key = `${event.session}:${p.call_id}`, map = p.type === "function_call" ? calls : outputs;
    if (map.has(key)) { assert.equal(canonical(map.get(key)), canonical(p), "conflicting trace duplicate"); duplicates++; }
    else map.set(key, p);
  }
  const repeated = new Map();
  let orphanOutputs = 0, missingOutputs = 0;
  for (const [key, call] of calls) {
    const row = toolTotals[call.name] ??= { calls: 0, outputs: 0, inputLocalTokens: 0, outputLocalTokens: 0, repeatedArguments: 0, identicalRepeatOutputTokens: 0 };
    row.calls++; row.inputLocalTokens += tokenCount(typeof call.arguments === "string" ? call.arguments : JSON.stringify(call.arguments));
    const args = parsed(call.arguments);
    // Scope repeats to a session; identical state reads can still be necessary.
    const signature = `${key.slice(0, -call.call_id.length)}:${call.name}:${canonical(args)}`;
    const output = outputs.get(key), previous = repeated.get(signature);
    if (previous) row.repeatedArguments++;
    if (!output) { missingOutputs++; repeated.set(signature, previous ?? {}); continue; }
    const text = outputText(output.output), tokens = tokenCount(text), digest = hash(text);
    row.outputs++; row.outputLocalTokens += tokens;
    if (previous?.digest === digest) row.identicalRepeatOutputTokens += tokens;
    repeated.set(signature, { digest });
  }
  for (const key of outputs.keys()) if (!calls.has(key)) orphanOutputs++;
  return { version: 1, evidence: "existingTraceAttribution", providerByOperation, providerByWork, toolTotals,
    coverage: { sessions: sessions.size, calls: calls.size, outputs: outputs.size, duplicateEvents: duplicates, missingOutputs, orphanOutputs },
    limitations: ["Selected trace coverage is not a complete provider prompt capture.",
      "Local tool tokens exclude schemas, history replay, reasoning and cache effects; never add them to Engine usage.",
      "Operation is not cognitive activity: advisory includes useful creative work and verification.",
      "Repeated arguments or identical outputs do not prove waste; versions, intervening writes and recovery matter.",
      "Unknown reservations are not actual billed tokens. Unmapped usage remains in the report."] };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [tracePath, ledgerPath, outputPath] = process.argv.slice(2);
  assert.ok(tracePath && ledgerPath && outputPath, "Usage: agent-overhead-report.mjs trace.json ledgers.json report.json");
  const [trace, ledger] = await Promise.all([readFile(tracePath, "utf8"), readFile(ledgerPath, "utf8")]);
  const encoding = getEncoding("cl100k_base");
  const report = attributeOverhead(JSON.parse(trace), JSON.parse(ledger), text => encoding.encode(text).length);
  report.localEncoding = "cl100k_base";
  report.sources = [[tracePath, trace], [ledgerPath, ledger]].map(([path, data]) => ({ path: basename(path), sha256: hash(data) }));
  await saveReport(outputPath, report);
  console.log(JSON.stringify({ report: resolve(outputPath), coverage: report.coverage, providerByOperation: report.providerByOperation }));
}

// Observations of returned text, not a usefulness score or billing ledger.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { saveReport } from "./tool-evaluation-report.mjs";

const hash = text => createHash("sha256").update(text).digest("hex");
const canonical = value => JSON.stringify(value, function (_, item) {
  return item && typeof item === "object" && !Array.isArray(item)
    ? Object.fromEntries(Object.keys(item).sort().map(key => [key, item[key]])) : item;
});
const bins = () => Object.fromEntries(["sameTurnSameContext", "afterCompaction", "differentTurn", "unknownTurn"]
  .map(key => [key, { returns: 0, utf8Bytes: 0 }]));
const metric = () => ({ returns: 0, utf8Bytes: 0, repeats: bins() });
function observe(stat, seen, key, text, scope) {
  const bytes = Buffer.byteLength(text);
  stat.returns++; stat.utf8Bytes += bytes;
  const prior = seen.get(key);
  if (prior) {
    const kind = !scope.turn || !prior.turn ? "unknownTurn"
      : prior.turn !== scope.turn ? "differentTurn"
        : prior.epoch !== scope.epoch ? "afterCompaction" : "sameTurnSameContext";
    stat.repeats[kind].returns++; stat.repeats[kind].utf8Bytes += bytes;
  }
  seen.set(key, scope);
}
function outputText(output) {
  const parts = typeof output === "string" ? [output]
    : Array.isArray(output) && output.every(part => typeof part.text === "string"
      && ["text", "input_text"].includes(part.type)) ? output.map(part => part.text) : null;
  if (!parts) return null;
  return parts.join("\n").replace(/^Wall time: [^\n]*\nOutput:\s*\n?/, "");
}

export function auditContextReturns(events, { from, to } = {}) {
  const start = from == null ? -Infinity : Date.parse(from);
  const end = to == null ? Infinity : Date.parse(to);
  assert.ok(!Number.isNaN(start) && !Number.isNaN(end) && start < end, "invalid audit interval");
  const calls = new Map(), seen = new Map(), textSeen = new Map(), standardsSeen = new Map(), outputs = new Set();
  const tools = Object.create(null);
  const acceptance = metric();
  const issues = { unmatchedOutputs: 0, nonTextOutputs: 0, invalidArguments: 0, invalidTimestamps: 0,
    unstructuredOutputs: 0, duplicateCallIds: 0, duplicateOutputIds: 0 };
  let turn = null, epoch = 0, compactions = 0;
  for (const event of events) {
    const time = Date.parse(event.timestamp);
    if (!Number.isFinite(time)) { issues.invalidTimestamps++; continue; }
    if (time > end) continue;
    const selected = time > start;
    const payload = event.payload ?? {};
    if (event.type === "turn_context") turn = payload.turn_id ?? null;
    if (event.type === "compacted") { epoch++; if (selected) compactions++; }
    if (event.type !== "response_item") continue;
    if (payload.type === "function_call") {
      if (calls.has(payload.call_id)) { if (selected) issues.duplicateCallIds++; continue; }
      let args;
      try { args = canonical(JSON.parse(payload.arguments)); }
      catch { if (selected) issues.invalidArguments++; }
      calls.set(payload.call_id, { name: payload.name, namespace: payload.namespace ?? "", args,
        turn: payload.internal_chat_message_metadata_passthrough?.turn_id ?? turn });
    }
    if (!selected || payload.type !== "function_call_output") continue;
    if (outputs.has(payload.call_id)) { issues.duplicateOutputIds++; continue; }
    outputs.add(payload.call_id);
    const call = calls.get(payload.call_id);
    if (!call || typeof call.name !== "string") { issues.unmatchedOutputs++; continue; }
    const text = outputText(payload.output);
    if (text === null) { issues.nonTextOutputs++; continue; }
    const name = call.namespace ? `${call.namespace}.${call.name}` : call.name;
    const stat = tools[name] ??= { ...metric(), identicalText: metric(), errors: 0 };
    // Unknown arguments never establish identical calls. No raw arguments/text in the report.
    const key = hash(canonical([name, call.args ?? payload.call_id, text]));
    const scope = { turn: call.turn, epoch };
    observe(stat, seen, key, text, scope);
    observe(stat.identicalText, textSeen, hash(canonical([name, text])), text, scope);
    let value;
    try { value = JSON.parse(text); } catch { issues.unstructuredOutputs++; continue; }
    if (value?.isError === true || value?.error != null) stat.errors++;
    if (call.name === "read_work_status" && value?.chapterAcceptance
      && typeof value.chapterAcceptance === "object") {
      const content = canonical(value.chapterAcceptance);
      observe(acceptance, standardsSeen, hash(content), content, scope);
    }
  }
  return { interval: { after: from ?? null, through: to ?? null }, compactions, tools,
    acceptanceFields: acceptance, issues };
}

export const auditLimitations = [
  "UTF-8 bytes are tool-return observations, not provider tokens, charges, or measured savings.",
  "Identical returns in one turn/context are candidates for review, not proof of waste or semantic uselessness.",
  "Compaction and new turns may require rereading. Missing turn identity remains unknown.",
  "Call repeats require identical arguments/text; identicalText ignores arguments. Both and acceptance-field counts overlap; never add them.",
  "Only exact returned text matches; changing counters or metadata can hide repeated source content.",
  "Only explicit JSON error/isError values are counted as errors; unstructured results remain unclassified.",
  "No replay amplification, literary quality, delivery status or provider usage is inferred from these events.",
];

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2), output = args.shift(), options = {};
  for (const option of ["from", "to"]) {
    const index = args.indexOf(`--${option}`);
    if (index >= 0) {
      assert.ok(args[index + 1], `--${option} requires an ISO timestamp`);
      options[option] = args[index + 1]; args.splice(index, 2);
    }
  }
  assert.ok(output && args.length && args.every(path => !path.startsWith("--")),
    "Usage: context-return-audit.mjs output.json [--from ISO] [--to ISO] rollout.jsonl ...");
  assert.ok(args.every(path => resolve(path) !== resolve(output)), "output must not overwrite input");
  const sessions = [], identities = new Set();
  for (const path of args) {
    const text = await readFile(path, "utf8");
    const events = text.split(/\r?\n/).filter(line => line.trim()).map(line => JSON.parse(line));
    const ids = new Set(events.filter(event => event.type === "session_meta").map(event => event.payload?.id));
    const [sessionId] = ids;
    assert.ok(ids.size === 1 && typeof sessionId === "string" && sessionId && !identities.has(sessionId),
      "missing or duplicate session identity");
    identities.add(sessionId);
    sessions.push({ sessionId, source: { path: resolve(path), sha256: hash(text) },
      ...auditContextReturns(events, options) });
  }
  await saveReport(output, { version: 1, evidence: "observed_tool_returns", sessions, limitations: auditLimitations });
  console.log(JSON.stringify({ report: resolve(output), sessions: sessions.length }));
}

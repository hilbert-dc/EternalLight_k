// Read-only analysis of provider-reported rollout counters. Never a billing ledger.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { saveReport } from "./tool-evaluation-report.mjs";

const counter = value => Number.isSafeInteger(value) && value >= 0;
const fields = ["input_tokens", "cached_input_tokens", "output_tokens", "reasoning_output_tokens", "total_tokens"];
const sum = values => {
  if (values.some(value => value === null)) return null;
  const result = values.reduce((a, b) => a + b, 0);
  assert.ok(Number.isSafeInteger(result), "usage sum exceeds safe integer range");
  return result;
};

function price(usage, rates) {
  if (!rates) return null;
  assert.ok(typeof rates.currency === "string" && rates.currency.trim(), "explicit currency required");
  for (const key of ["uncachedInputPerMillion", "cachedInputPerMillion", "outputPerMillion"]) {
    assert.ok(Number.isFinite(rates[key]) && rates[key] >= 0, `invalid rate: ${key}`);
  }
  if (!usage || usage.cachedInputTokens === null) return null;
  const amount = (usage.uncachedInputTokens * rates.uncachedInputPerMillion
    + usage.cachedInputTokens * rates.cachedInputPerMillion
    + usage.outputTokens * rates.outputPerMillion) / 1e6;
  assert.ok(Number.isFinite(amount), "cost exceeds finite range");
  return { currency: rates.currency, amount, basis: "explicit_rates_not_invoice" };
}

export function summarizeContextUsage(sessions, { ratesBySession = {} } = {}) {
  assert.ok(Array.isArray(sessions) && sessions.length, "at least one session is required");
  const seen = new Set();
  const rows = sessions.map(({ sessionId, events }) => {
    assert.ok(typeof sessionId === "string" && sessionId && !seen.has(sessionId), "missing or duplicate session identity");
    assert.ok(Array.isArray(events), "events must be an array");
    seen.add(sessionId);
    const metadata = events.filter(event => event.type === "session_meta");
    const inheritedHistory = metadata.some(event => event.payload?.id !== sessionId);
    const scopedRecords = events.filter(event => event.type === "token_usage_record" && event.payload?.thread_id === sessionId);
    const ownTurns = new Set(scopedRecords.map(event => event.payload.turn_id).filter(Boolean));
    let currentTurn = null;
    const ownedEvents = events.filter(event => {
      if (event.type === "turn_context") currentTurn = event.payload?.turn_id ?? null;
      return !inheritedHistory || ownTurns.has(currentTurn);
    });
    // Full-history children contain the parent's session_meta and token_count
    // events. Use explicitly owned counters; never charge inherited history twice.
    const usageEvents = scopedRecords.length ? scopedRecords.map(event => ({ type: "event_msg", payload: {
      type: "token_count", info: { total_token_usage: event.payload.thread_token_usage },
    } })) : inheritedHistory ? [] : events;
    let latest = null, previous = null, samples = 0, compactions = 0;
    const issues = new Set();
    compactions = ownedEvents.filter(event => event.type === "compacted").length;
    for (const event of usageEvents) {
      if (event.type !== "event_msg" || event.payload?.type !== "token_count") continue;
      const value = event.payload.info?.total_token_usage;
      if (!value) continue; // Rate-limit-only notifications contain no usage.
      samples++;
      if (!["input_tokens", "output_tokens", "total_tokens"].every(key => counter(value[key]))
        || fields.some(key => value[key] != null && !counter(value[key]))) {
        issues.add("invalid_counter"); continue;
      }
      if (value.total_tokens !== value.input_tokens + value.output_tokens
        || (value.cached_input_tokens ?? 0) > value.input_tokens
        || (value.reasoning_output_tokens ?? 0) > value.output_tokens) issues.add("inconsistent_counter");
      if (previous && fields.some(key => value[key] != null && previous[key] != null && value[key] < previous[key])) {
        issues.add("cumulative_counter_decreased");
      }
      previous = { ...previous, ...Object.fromEntries(fields.filter(key => value[key] != null).map(key => [key, value[key]])) };
      latest = value;
    }
    if (!latest) issues.add("usage_missing");
    const usage = latest && issues.size === 0 ? {
      inputTokens: latest.input_tokens,
      cachedInputTokens: latest.cached_input_tokens ?? null,
      uncachedInputTokens: latest.cached_input_tokens == null ? null : latest.input_tokens - latest.cached_input_tokens,
      outputTokens: latest.output_tokens,
      reasoningOutputTokens: latest.reasoning_output_tokens ?? null,
      totalTokens: latest.total_tokens,
      cacheHitRatio: latest.input_tokens > 0 && latest.cached_input_tokens != null
        ? latest.cached_input_tokens / latest.input_tokens : null,
    } : null;
    const parents = new Set(metadata.filter(event => event.payload?.id === sessionId).map(event => event.payload?.source?.subagent?.thread_spawn?.parent_thread_id)
      .filter(value => typeof value === "string" && value));
    assert.ok(parents.size <= 1, "conflicting session parent identities");
    const parentSessionId = [...parents][0] ?? null;
    const contextForks = ownedEvents.filter(event => event.type === "response_item"
      && event.payload?.type === "function_call" && event.payload?.name === "spawn_agent")
      .map(event => {
        try {
          const args = JSON.parse(event.payload.arguments);
          return args.fork_turns === "none" ? "isolated" : args.fork_turns === "all" ? "fullHistory" : "unspecifiedOrUnknown";
        } catch { return "unspecifiedOrUnknown"; }
      });
    return { sessionId, parentSessionId, contextForks: {
      isolated: contextForks.filter(value => value === "isolated").length,
      fullHistory: contextForks.filter(value => value === "fullHistory").length,
      unspecifiedOrUnknown: contextForks.filter(value => value === "unspecifiedOrUnknown").length,
      meaning: "requested_context_modes_not_successful_spawns_or_proof_of_blind_review",
    }, counterSource: scopedRecords.length ? "owned_thread_token_usage" : inheritedHistory ? "unavailable_inherited_history" : "rollout_total_token_usage",
    inheritedHistory, usageSamples: samples, compactions, usage, issues: [...issues],
      estimatedCost: price(usage, ratesBySession[sessionId]) };
  });
  const aggregate = Object.fromEntries(["inputTokens", "cachedInputTokens", "uncachedInputTokens", "outputTokens", "reasoningOutputTokens", "totalTokens"]
    .map(key => [key, sum(rows.map(row => row.usage?.[key] ?? null))]));
  const costs = rows.map(row => row.estimatedCost);
  const currency = costs[0]?.currency;
  const included = new Set(rows.map(row => row.sessionId));
  const orphanSessions = rows.filter(row => row.parentSessionId && !included.has(row.parentSessionId)).map(row => row.sessionId);
  // Never add tree totals to aggregate: these are overlapping explanatory views.
  const trees = rows.filter(row => !row.parentSessionId || !included.has(row.parentSessionId)).map(root => {
    const descendants = new Set([root.sessionId]);
    let size;
    do {
      size = descendants.size;
      for (const row of rows) if (descendants.has(row.parentSessionId)) descendants.add(row.sessionId);
    } while (size !== descendants.size);
    const members = rows.filter(row => descendants.has(row.sessionId));
    return { rootSessionId: root.sessionId, sessionIds: members.map(row => row.sessionId),
      knownTotalTokens: sum(members.map(row => row.usage?.totalTokens ?? null)),
      descendantTotalTokens: sum(members.filter(row => row !== root).map(row => row.usage?.totalTokens ?? null)) };
  });
  assert.equal(new Set(trees.flatMap(tree => tree.sessionIds)).size, rows.length, "cyclic session lineage");
  return { version: 1, evidence: "provider_reported_cumulative_rollout_usage", sessions: rows,
    lineage: { trees, orphanSessions, completeness: "supplied_sessions_only", additiveToAggregate: false },
    aggregate, estimatedCost: currency && costs.every(cost => cost?.currency === currency)
      ? { currency, amount: costs.reduce((value, cost) => value + cost.amount, 0), basis: "explicit_rates_not_invoice" } : null,
    limitations: [
      "Coverage is limited to supplied sessions, including their earlier cumulative usage; not automatically the entire Work.",
      "Lineage uses recorded parent_thread_id. Tree totals overlap aggregate; unprovided descendants cannot be inferred or treated as zero.",
      "Cumulative usage samples are not request counts and are never summed within a session.",
      "Cached input is included in input; reasoning output is included in output. Missing counters remain unknown.",
      "Compaction events do not isolate compaction costs; their reported usage remains included in cumulative totals.",
      "Missing/failed responses may have unreported usage. Consult the Engine ledger for unknown reservations separately.",
      "No cost is inferred without explicit rates; estimates exclude other charges and are not invoices.",
      "Token reduction does not establish literary quality, delivered chapters, or optimization benefit. Compare matched tasks with verified outcomes.",
    ] };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const output = args.shift();
  const rateIndex = args.indexOf("--rates");
  let ratesBySession = {};
  if (rateIndex !== -1) {
    const ratePath = args[rateIndex + 1];
    assert.ok(ratePath, "--rates requires a JSON file keyed by session id");
    ratesBySession = JSON.parse(await readFile(ratePath, "utf8"));
    args.splice(rateIndex, 2);
  }
  assert.ok(output && args.length, "Usage: context-efficiency-report.mjs output.json [--rates rates.json] rollout.jsonl ...");
  assert.ok(args.every(path => resolve(path) !== resolve(output)), "output must not overwrite an input rollout");
  const sources = [], sessions = [];
  for (const path of args) {
    const text = await readFile(path, "utf8");
    const events = text.split(/\r?\n/).filter(line => line.trim()).map(line => JSON.parse(line));
    const owner = events[0]?.type === "session_meta" ? events[0].payload?.id : null;
    assert.ok(typeof owner === "string" && owner, "rollout must start with its owning session identity");
    sessions.push({ sessionId: owner, events });
    sources.push({ path: resolve(path), sha256: createHash("sha256").update(text).digest("hex") });
  }
  const report = { ...summarizeContextUsage(sessions, { ratesBySession }), sources };
  await saveReport(output, report);
  console.log(JSON.stringify({ report: resolve(output), aggregate: report.aggregate, estimatedCost: report.estimatedCost }));
}

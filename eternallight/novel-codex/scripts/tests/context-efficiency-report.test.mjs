import test from "node:test";
import assert from "node:assert/strict";
import { summarizeContextUsage } from "../context-efficiency-report.mjs";

const sample = (input, cached, output = 10, reasoning = 4) => ({ type: "event_msg", payload: {
  type: "token_count", info: { total_token_usage: {
    input_tokens: input, cached_input_tokens: cached, output_tokens: output,
    reasoning_output_tokens: reasoning, total_tokens: input + output,
  } },
} });
const session = events => ({ sessionId: "s", events });
const rates = { currency: "CNY", uncachedInputPerMillion: 2, cachedInputPerMillion: 0.2, outputPerMillion: 8 };

test("recorded child lineage exposes full-history investigation costs without double counting", () => {
  const child = (id, parent) => ({ sessionId: id, events: [
    { type: "session_meta", payload: { id, source: { subagent: { thread_spawn: { parent_thread_id: parent } } } } },
    sample(100, 90),
  ] });
  const root = session([sample(200, 180), { type: "response_item", payload: {
    type: "function_call", name: "spawn_agent", arguments: JSON.stringify({ fork_turns: "all" }),
  } }]);
  const report = summarizeContextUsage([root, child("child", "s"), child("grandchild", "child")]);
  assert.equal(report.aggregate.totalTokens, 430);
  assert.equal(report.lineage.trees[0].knownTotalTokens, 430);
  assert.equal(report.lineage.trees[0].descendantTotalTokens, 220);
  assert.equal(report.sessions[0].contextForks.fullHistory, 1);
  assert.equal(report.lineage.additiveToAggregate, false);
  assert.deepEqual(summarizeContextUsage([child("child", "missing")]).lineage.orphanSessions, ["child"]);
  assert.throws(() => summarizeContextUsage([child("a", "b"), child("b", "a")]), /cyclic/);
});

test("full-history fork ignores inherited parent metadata and counters", () => {
  const own = { input_tokens: 100, cached_input_tokens: 90, output_tokens: 10, reasoning_output_tokens: 4, total_tokens: 110 };
  const events = [
    { type: "session_meta", payload: { id: "child", source: { subagent: { thread_spawn: { parent_thread_id: "parent" } } } } },
    { type: "session_meta", payload: { id: "parent", source: { subagent: { thread_spawn: { parent_thread_id: "grandparent" } } } } },
    sample(5000, 4000),
    { type: "token_usage_record", payload: { thread_id: "parent", thread_token_usage: { ...own, input_tokens: 5000, total_tokens: 5010 } } },
    { type: "token_usage_record", payload: { thread_id: "child", turn_id: "child-turn", thread_token_usage: own } },
  ];
  const report = summarizeContextUsage([{ sessionId: "child", events }]);
  assert.equal(report.aggregate.totalTokens, 110);
  assert.equal(report.sessions[0].parentSessionId, "parent");
  assert.equal(report.sessions[0].counterSource, "owned_thread_token_usage");
  assert.equal(summarizeContextUsage([{ sessionId: "child", events: events.slice(0, 3) }]).aggregate.totalTokens, null);
});

test("cumulative samples, cache and reasoning are not counted twice; compaction stays included", () => {
  const report = summarizeContextUsage([session([sample(100, 90), sample(100, 90), { type: "compacted" }, sample(200, 180, 20, 8)])], { ratesBySession: { s: rates } });
  assert.equal(report.aggregate.totalTokens, 220);
  assert.equal(report.aggregate.uncachedInputTokens, 20);
  assert.equal(report.sessions[0].compactions, 1);
  assert.equal(report.sessions[0].usageSamples, 3);
  assert.equal(report.sessions[0].usage.cacheHitRatio, 0.9);
  assert.equal(report.estimatedCost.amount, (20 * 2 + 180 * 0.2 + 20 * 8) / 1e6);
});

test("absent rates/cache/usage are unknown, not free; invalid counters cannot become savings", () => {
  const report = summarizeContextUsage([session([sample(100, undefined)])]);
  assert.equal(report.aggregate.cachedInputTokens, null);
  assert.equal(report.aggregate.uncachedInputTokens, null);
  assert.equal(report.estimatedCost, null);
  assert.equal(summarizeContextUsage([session([sample(100, undefined)])], { ratesBySession: { s: rates } }).estimatedCost, null);
  for (const events of [[], [sample(100, 101)], [sample(-1, 0)], [sample(200, 100), sample(100, 90)]]) {
    const result = summarizeContextUsage([session(events)], { ratesBySession: { s: rates } });
    assert.ok(result.sessions[0].issues.length);
    assert.equal(result.aggregate.totalTokens, null);
    assert.equal(result.estimatedCost, null);
  }
});

test("multiple sessions remain distinct and mixed currencies or missing coverage have no grand total cost", () => {
  const sessions = [session([sample(100, 90)]), { sessionId: "child", events: [sample(100, 0)] }];
  const report = summarizeContextUsage(sessions, { ratesBySession: { s: rates, child: { ...rates, currency: "USD" } } });
  assert.equal(report.aggregate.totalTokens, 220);
  assert.equal(report.estimatedCost, null);
  assert.throws(() => summarizeContextUsage([sessions[0], sessions[0]]), /duplicate session/);
  assert.throws(() => summarizeContextUsage(sessions, { ratesBySession: { s: { ...rates, outputPerMillion: -1 } } }), /invalid rate/);
});

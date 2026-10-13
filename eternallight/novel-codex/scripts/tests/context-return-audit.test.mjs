import test from "node:test";
import assert from "node:assert/strict";
import { auditContextReturns } from "../context-return-audit.mjs";

const at = "2026-10-11T13:00:00Z";
const event = (type, payload, timestamp = at) => ({ timestamp, type, payload });
const context = turn_id => event("turn_context", { turn_id });
const call = (id, text = '{"body":"原文"}', args = "{}", name = "read_source_content") => [
  event("response_item", { type: "function_call", name, call_id: id, arguments: args }),
  event("response_item", { type: "function_call_output", call_id: id, output: [
    { type: "input_text", text: "Wall time: 0.1 seconds\nOutput:" },
    { type: "input_text", text },
  ] }),
];

test("identical returns distinguish same context, compaction, new turns and unknown scope", () => {
  const events = [...call("unknown"), context("a"), ...call("a"), ...call("b"),
    event("compacted", {}), ...call("c"), context("b"), ...call("d")];
  const report = auditContextReturns(events), stat = report.tools.read_source_content;
  assert.equal(stat.returns, 5);
  assert.equal(stat.utf8Bytes, Buffer.byteLength('{"body":"原文"}') * 5);
  for (const item of Object.values(stat.repeats)) assert.equal(item.returns, 1);
  assert.equal(report.compactions, 1);
  assert.ok(!JSON.stringify(report).includes("原文"));
});

test("arguments, source versions and exact text prevent falsely matching different reads", () => {
  const report = auditContextReturns([context("a"), ...call("a", "same", '{"page":1,"version":1}'),
    ...call("b", "same", '{"version":1,"page":1}'), ...call("c", "same", '{"page":2,"version":1}'),
    ...call("d", "same", '{"page":1,"version":2}'), ...call("e", "same ", '{"page":1,"version":1}')]);
  assert.equal(report.tools.read_source_content.repeats.sameTurnSameContext.returns, 1);
  assert.equal(report.tools.read_source_content.identicalText.repeats.sameTurnSameContext.returns, 3);
  assert.equal(report.issues.unstructuredOutputs, 5);
});

test("repeated standards are visible even when status changes; figures overlap", () => {
  const report = auditContextReturns([context("a"),
    ...call("a", '{"chapterAcceptance":{"criteria":["正文完整"]},"version":1}', "{}", "read_work_status"),
    ...call("b", '{"version":2,"chapterAcceptance":{"criteria":["正文完整"]}}', "{}", "read_work_status"),
    ...call("c", '{"chapterAcceptanceSource":{"sourceId":"new"}}', "{}", "read_work_status")]);
  assert.equal(report.acceptanceFields.returns, 2);
  assert.equal(report.acceptanceFields.repeats.sameTurnSameContext.returns, 1);
  assert.equal(report.tools.read_work_status.repeats.sameTurnSameContext.returns, 0);
});

test("window excludes prior duplicate content but retains call identity for a boundary output", () => {
  const before = "2026-10-11T12:59:59Z", after = "2026-10-11T13:00:01Z";
  const first = call("a").map(e => ({ ...e, timestamp: before }));
  const boundary = call("b"); boundary[0].timestamp = before;
  const report = auditContextReturns([context("a"), ...first, ...boundary,
    ...call("c").map(e => ({ ...e, timestamp: after }))], { from: before, to: at });
  assert.equal(report.tools.read_source_content.returns, 1);
  assert.equal(report.tools.read_source_content.repeats.sameTurnSameContext.returns, 0);
  assert.throws(() => auditContextReturns([], { from: "bad" }), /invalid audit interval/);
});

test("malformed, unmatched and duplicate evidence is reported, never silently counted as successful rereads", () => {
  const first = call("a", '{"isError":true,"error":"private detail"}');
  const unmatched = event("response_item", { type: "function_call_output", call_id: "missing", output: "text" });
  const image = call("image"); image[1].payload.output = [{ type: "image", data: "private" }];
  const report = auditContextReturns([context("a"), ...first, ...first, unmatched, ...image,
    ...call("bad", "text", "malformed"), event("response_item", {}, "bad-time")]);
  assert.equal(report.tools.read_source_content.errors, 1);
  assert.equal(report.issues.duplicateCallIds, 1);
  assert.equal(report.issues.duplicateOutputIds, 1);
  assert.equal(report.issues.unmatchedOutputs, 1);
  assert.equal(report.issues.nonTextOutputs, 1);
  assert.equal(report.issues.invalidArguments, 1);
  assert.equal(report.issues.invalidTimestamps, 1);
  assert.ok(!JSON.stringify(report).includes("private"));
});

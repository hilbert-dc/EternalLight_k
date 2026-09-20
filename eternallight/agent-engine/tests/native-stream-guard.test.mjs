import assert from "node:assert/strict";
import { test } from "node:test";
import { nativeBinding } from "../src/native.mjs";

const context = { registry: null, tools: [{ type: "function", namespace: null, name: "read", schema: { type: "object" } }], toolChoice: "auto" };
const call = { type: "function_call", name: "read", call_id: "call", arguments: "{}" };

test("Rust production stream closes on cancellation and drops buffered tools", () => {
  const stream = new (nativeBinding().NativeResponsesStream)(JSON.stringify(context));
  assert.deepEqual(JSON.parse(stream.push(JSON.stringify({ type: "response.output_item.done", item: call }))), []);
  stream.close();
  assert.throws(() => stream.finish(), /closed/);
});

test("Rust production stream rejects malformed input without logging payloads", () => {
  const stream = new (nativeBinding().NativeResponsesStream)(JSON.stringify(context));
  assert.throws(() => stream.push("not-for-logs"), (error) => /invalid stream event/.test(error.message) && !error.message.includes("not-for-logs"));
  assert.throws(() => stream.finish(), /closed/);
});

test("Rust stream failure cannot commit a buffered tool or affect another stream", () => {
  const stream = new (nativeBinding().NativeResponsesStream)(JSON.stringify(context));
  const other = new (nativeBinding().NativeResponsesStream)(JSON.stringify(context));
  stream.push(JSON.stringify({ type: "response.output_item.done", item: call }));
  assert.throws(() => stream.push(JSON.stringify({ type: "response.failed" })), /successfully/);
  assert.throws(() => stream.finish(), /closed/);
  const completed = { type: "response.completed", response: { status: "completed", output: [] } };
  assert.deepEqual(JSON.parse(other.push(JSON.stringify(completed))), []);
  assert.deepEqual(JSON.parse(other.finish()), [completed]);
  other.close();
});

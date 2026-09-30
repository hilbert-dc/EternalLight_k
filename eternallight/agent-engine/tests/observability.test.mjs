import assert from "node:assert/strict";
import { test } from "node:test";

import { BoundedExecutionTrace, CONFORMANCE_FAILURE_CATEGORIES, classifyConformanceFailure } from "../src/index.mjs";

test("bounded execution traces preserve source order without unbounded content", () => {
  let now = 100;
  const trace = new BoundedExecutionTrace({ runId: "run-1", maxEvents: 2, maxStringLength: 32, clock: () => now++ });
  trace.record("bridge", "bridge.request.prepared", { threadId: "child", text: "x".repeat(100) });
  trace.record("bridge", "bridge.response.completed", { threadId: "child" });
  assert.equal(trace.record("mcp", "mcp.call.completed", {}), null);
  assert.deepEqual(trace.snapshot(), {
    version: 1,
    runId: "run-1",
    events: [
      { version: 1, runId: "run-1", source: "bridge", sourceSequence: 1, occurredAtMs: 100, type: "bridge.request.prepared", details: { threadId: "child", text: "x".repeat(32) } },
      { version: 1, runId: "run-1", source: "bridge", sourceSequence: 2, occurredAtMs: 101, type: "bridge.response.completed", details: { threadId: "child" } },
    ],
    droppedCount: 1,
    incomplete: true,
  });
});

test("bounded execution traces retain late milestones after reasoning deltas fill the buffer", () => {
  const trace = new BoundedExecutionTrace({ runId: "run-milestones", maxEvents: 3 });
  trace.record("codex", "agent.event", { method: "item/reasoning/textDelta", index: 0 });
  trace.record("codex", "agent.event", { method: "item/agentMessage/delta", index: 1 });
  trace.record("codex", "agent.event", { method: "item/reasoning/DELTA", index: 2 });
  trace.record("mcp", "mcp.call.rejected", { reason: "identity", toolName: "echo" });
  const snapshot = trace.snapshot();
  assert.equal(snapshot.events.length, 3);
  assert.equal(snapshot.events.at(-1).type, "mcp.call.rejected");
  assert.equal(snapshot.droppedCount, 1);
  assert.equal(classifyConformanceFailure(snapshot, { expectedTool: "echo", timedOut: true }), "identity-rejected");
});

test("conformance failure classification distinguishes exposure, selection, execution and delivery", () => {
  assert.ok(CONFORMANCE_FAILURE_CATEGORIES.includes("tool-not-exposed"));
  const make = () => new BoundedExecutionTrace({ runId: "run" });
  const missing = make();
  missing.record("bridge", "bridge.request.prepared", { parentThreadId: "root", tools: ["other"] });
  assert.equal(classifyConformanceFailure(missing.snapshot(), { expectedTool: "echo" }), "tool-not-exposed");

  const unselected = make();
  unselected.record("bridge", "bridge.request.prepared", { parentThreadId: "root", tools: ["echo"] });
  unselected.record("bridge", "bridge.response.completed", { parentThreadId: "root", toolCalls: [] });
  assert.equal(classifyConformanceFailure(unselected.snapshot(), { expectedTool: "echo" }), "tool-not-selected");

  const namespaced = make();
  namespaced.record("bridge", "bridge.request.prepared", {
    parentThreadId: "root",
    tools: ["mcp__engine_conformance.engine_conformance_echo"],
    encodedTools: ["n23__mcp__engine_conformance__engine_conformance_echo"],
  });
  namespaced.record("bridge", "bridge.response.completed", { parentThreadId: "root", toolCalls: [] });
  assert.equal(classifyConformanceFailure(namespaced.snapshot(), { expectedTool: "engine_conformance_echo", requestScope: "child" }), "tool-not-selected");

  const textual = make();
  textual.record("bridge", "bridge.request.prepared", { parentThreadId: "root", tools: ["echo"] });
  textual.record("bridge", "bridge.response.completed", { parentThreadId: "root", toolCalls: [], textLooksLikeToolCall: true });
  assert.equal(classifyConformanceFailure(textual.snapshot(), { expectedTool: "echo", timedOut: true }), "tool-call-as-text");

  const unexecuted = make();
  unexecuted.record("bridge", "bridge.request.prepared", { parentThreadId: "root", tools: ["echo"] });
  unexecuted.record("bridge", "bridge.response.completed", { parentThreadId: "root", toolCalls: [{ name: "echo", callId: "call-1" }] });
  assert.equal(classifyConformanceFailure(unexecuted.snapshot(), { expectedTool: "echo" }), "execution-error");
  assert.equal(classifyConformanceFailure(unexecuted.snapshot(), { expectedTool: "echo", timedOut: true }), "execution-error");

  const undelivered = make();
  undelivered.record("bridge", "bridge.request.prepared", { parentThreadId: "root", tools: ["echo"], toolResultCallIds: [] });
  undelivered.record("bridge", "bridge.response.completed", { parentThreadId: "root", toolCalls: [{ name: "echo", callId: "call-1" }] });
  undelivered.record("mcp", "mcp.call.completed", { toolName: "echo", callId: "call-1" });
  assert.equal(classifyConformanceFailure(undelivered.snapshot(), { expectedTool: "echo" }), "transport-result-missing");

  const structured = make();
  structured.record("bridge", "bridge.request.prepared", { parentThreadId: "root", tools: [{ namespace: "engine", name: "echo" }] });
  structured.record("bridge", "bridge.response.completed", { parentThreadId: "root", toolCalls: [] });
  assert.equal(classifyConformanceFailure(structured.snapshot(), { expectedTool: "engine.echo" }), "tool-not-selected");

  const root = make();
  root.record("bridge", "bridge.request.prepared", { parentThreadId: null, tools: ["spawn_agent"] });
  root.record("bridge", "bridge.response.completed", { parentThreadId: null, toolCalls: [{ name: "spawn_agent" }] });
  assert.equal(classifyConformanceFailure(root.snapshot(), { expectedTool: "spawn_agent", requestScope: "root" }), "execution-error");

  const lifecycle = make();
  lifecycle.record("bridge", "bridge.request.prepared", { parentThreadId: "root", tools: ["gate"] });
  lifecycle.record("codex", "agent.event", { method: "error", errorMessage: "provider failed" });
  lifecycle.record("codex", "agent.event", { method: "thread/status/changed", status: "systemError" });
  assert.equal(classifyConformanceFailure(lifecycle.snapshot(), { expectedTool: "echo" }), "lifecycle-error");
});

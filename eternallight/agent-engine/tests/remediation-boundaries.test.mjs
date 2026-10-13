import assert from "node:assert/strict";
import { test } from "node:test";
import { responsesCodec } from "../src/codecs/responses.mjs";
import { chatCompletionsCodec } from "../src/codecs/chat-completions.mjs";
import { messagesCodec } from "../src/codecs/messages.mjs";
import { generateContentCodec } from "../src/codecs/generate-content.mjs";
import { ToolRegistry } from "../src/codecs/tool-registry.mjs";
import { textDigest, visibleTextDigests } from "../src/observability/message-evidence.mjs";
import { CAPABILITIES, TrustedEvidenceRepository, defineCapabilityReport, defineConnection, digest, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";
import { CodexConformanceRunner } from "../qualification/codex-conformance-runner.mjs";

const connection = { endpoint: "https://example.invalid", model: "fixture", declaredLimits: { contextTokens: 128000, outputTokens: 4000 } };
const tool = { type: "function", name: "read", parameters: { type: "object", properties: {} } };
const call = { type: "function_call", call_id: "call", name: "read", arguments: "{}" };
const stream = (events) => new Response(events.map((event) => `data: ${JSON.stringify(event)}\n\n`).join("")).body;

test("native Responses never releases executable items on truncation, failure, or conflicting terminal output", async () => {
  const { context } = responsesCodec.createRequest({ input: [], tools: [tool] }, connection, "test");
  const done = { type: "response.output_item.done", item: call };
  for (const events of [
    [done],
    [done, { type: "response.failed" }],
    [done, { type: "response.completed", response: { status: "completed", output: [] } }],
    [done, { type: "response.completed", response: { status: "completed", output: [{ ...call, arguments: '{"changed":true}' }] } }],
  ]) {
    const received = [];
    await assert.rejects(async () => {
      for await (const event of responsesCodec.parseStream(stream(events), context)) received.push(event);
    });
    assert.deepEqual(received, []);
  }
  const events = [done, { type: "response.completed", response: { status: "completed", output: [call] } }];
  assert.deepEqual(await Array.fromAsync(responsesCodec.parseStream(stream(events), context)), events);
});

test("Responses preserves reasoning lifecycle and the entire suffix after a buffered tool", async () => {
  const { context } = responsesCodec.createRequest({ input: [], tools: [tool] }, connection, "test");
  const reasoning = { type: "reasoning", id: "rs_test", summary: [] };
  const progress = [
    { type: "response.output_item.added", item: reasoning },
    { type: "response.reasoning_summary_part.added", item_id: "rs_test", summary_index: 0, part: { type: "summary_text", text: "" } },
    { type: "response.reasoning_summary_text.delta", item_id: "rs_test", summary_index: 0, delta: "Checking" },
    { type: "response.reasoning_summary_text.done", item_id: "rs_test", summary_index: 0, text: "Checking" },
    { type: "response.reasoning_summary_part.done", item_id: "rs_test", summary_index: 0, part: { type: "summary_text", text: "Checking" } },
    { type: "response.output_item.done", item: reasoning },
  ];
  for (const before of [true, false]) {
    const toolEvents = [{ type: "response.output_item.added", item: call }, { type: "response.output_item.done", item: call }];
    const prefix = before ? progress : [];
    const suffix = before ? [] : progress;
    const events = [...prefix, ...toolEvents, ...suffix, { type: "response.completed", response: { status: "completed", output: before ? [reasoning, call] : [call, reasoning] } }];
    assert.deepEqual(await Array.fromAsync(responsesCodec.parseStream(stream(events), context)), events);
    const received = [];
    await assert.rejects(async () => {
      for await (const event of responsesCodec.parseStream(stream([...prefix, ...toolEvents, ...suffix, { type: "response.failed" }]), context)) received.push(event);
    });
    assert.deepEqual(received, prefix);
  }
});

test("native Responses enforces full tool identity and rejects unadvertised executable item types", () => {
  const { context } = responsesCodec.createRequest({ input: [], tools: [tool, { type: "namespace", name: "other", tools: [tool] }], tool_choice: { type: "function", name: "read" } }, connection, "test");
  assert.throws(() => responsesCodec.parseResponse({ status: "completed", output: [{ ...call, namespace: "other" }] }, context), /another tool/);
  const none = responsesCodec.createRequest({ input: [], tools: [], tool_choice: "none" }, connection, "test");
  assert.throws(() => responsesCodec.parseResponse({ status: "completed", output: [{ type: "tool_search_call", execution: "client" }] }, none.context), /unsupported executable/);
  assert.throws(() => responsesCodec.parseResponse({ status: "failed", output: [call] }, context), /complete successfully/);
});

test("all codecs accept bounded token-sized multilingual messages, independent of labels", () => {
  const text = "这是小说的故事。".repeat(500);
  for (const codec of [responsesCodec, chatCompletionsCodec, messagesCodec, generateContentCodec]) {
    assert.doesNotThrow(() => codec.createRequest({ instructions: "You are a helpful writer. ".repeat(800), input: [{ type: "message", role: "user", content: text }], tools: [] }, connection, "test"));
    assert.doesNotThrow(() => codec.createRequest({ input: [{ type: "message", role: "user", content: "🙂".repeat(11000) }], tools: [] }, connection, "test"));
    assert.throws(() => codec.createRequest({ input: [{ type: "message", role: "user", content: "🙂".repeat(70000) }], tools: [] }, connection, "test"), /256KiB item byte limit/);
  }
});

test("adapted agent messages retain sender and recipient without becoming assistant statements", () => {
  const { body } = chatCompletionsCodec.createRequest({ input: [{ type: "agent_message", author: "parent", recipient: "child", content: [{ type: "input_text", text: "continue" }] }], tools: [] }, connection, "test");
  assert.equal(body.messages[0].role, "user");
  assert.match(body.messages[0].content, /Sender: "parent"; recipient: "child"/);
});

test("aliases remain stable when new tools collide with prior semantic-looking names", () => {
  const first = new ToolRegistry();
  const original = first.register({ namespace: "a", name: "b" }).wireName;
  const next = new ToolRegistry();
  next.register({ name: "a__b" });
  assert.equal(next.register({ namespace: "a", name: "b" }).wireName, original);
  assert.notEqual(next.wireName(null, "a__b"), original);
});

test("Generate Content evidence includes text, never function arguments or opaque fields", () => {
  assert.deepEqual(visibleTextDigests({ contents: [{ parts: [{ text: " visible " }, { functionCall: { args: { text: "hidden" } } }, { encrypted_content: "hidden" }] }] }), [textDigest("visible")]);
});

test("namespace descriptions and flattened definitions obey the single-item hard cap", () => {
  for (const namespaceToolsMode of ["native", "flatten"]) {
    const create = description => responsesCodec.createRequest({ input: [], tools: [{ type: "namespace", name: "large", description, tools: [tool] }] }, { ...connection, protocolOptions: { namespaceToolsMode } }, "test");
    const compressible = "x".repeat(40001);
    const { body } = create(compressible);
    if (namespaceToolsMode === "native") assert.equal(body.tools[0].description, compressible);
    else assert.ok(body.tools[0].description.startsWith(`${compressible}\n`));
    assert.doesNotThrow(() => create("界".repeat(12000)));
    assert.throws(() => create("a".repeat(256 * 1024 + 1)), /256KiB item byte limit/);
  }
});

test("directed runner evidence is adopted with exact credential scopes and stable auto identities", async () => {
  const parent = defineConnection({ ...connection, id: "parent", provider: "fixture", credentialRef: "PARENT_KEY", protocol: "auto" });
  const child = defineConnection({ ...connection, id: "child", provider: "fixture", credentialRef: "CHILD_KEY", protocol: "auto" });
  const parentEvidence = verifiedBaselineEvidence(parent, { protocol: "responses", credentialScope: "parent-scope" });
  const childEvidence = verifiedBaselineEvidence(child, { protocol: "responses", credentialScope: "child-scope" });
  const runner = new CodexConformanceRunner({ command: [process.execPath], baseEnvironment: {}, baseInstructions: "fixture", runProbe: async () => ({ observations: { childIdentityObserved: true, directedRouteObserved: true }, eventDigest: digest("route") }) });
  const result = await runner.verifyDirectedEdge({ parentApiKey: "fixture", childApiKey: "fixture", parentConnection: parent, childConnection: child, parentCredentialScope: "parent-scope", childCredentialScope: "child-scope", parentProtocol: "responses", childProtocol: "responses", parentProbeEvidence: parentEvidence, childProbeEvidence: childEvidence });
  assert.equal(result.record.status, "verified");
  const report = defineCapabilityReport({ connection: parent, result: { records: [result.record] }, startedAt: new Date(), completedAt: new Date() });
  const repo = await TrustedEvidenceRepository.load({ records: [{ ...result.record, reportRef: report.reportId, reportDigest: report.reportId }], reportStore: { read: async () => report } });
  const edge = { parentConnectionId: parent.id, childConnectionId: child.id };
  const policy = { defaultConnectionId: parent.id, allowedChildConnectionIds: [child.id], allowedChildEdges: [edge], credentialScopes: { parent: "parent-scope", child: "child-scope" } };
  const baseline = parentEvidence.filter((record) => record.capability !== CAPABILITIES.CHILD_DIRECTED_ROUTE);
  const evidence = [...baseline, ...childEvidence, ...repo.resolveSnapshot().records];
  assert.deepEqual(resolveSessionPlan({ connections: [parent, child], evidence, policy }).allowedChildEdges, [edge]);
  const rotatedEvidence = [...baseline, ...verifiedBaselineEvidence(child, { protocol: "responses", credentialScope: "rotated" }), ...repo.resolveSnapshot().records];
  assert.deepEqual(resolveSessionPlan({ connections: [parent, child], evidence: rotatedEvidence, policy: { ...policy, credentialScopes: { parent: "parent-scope", child: "rotated" } } }).allowedChildEdges, []);
});

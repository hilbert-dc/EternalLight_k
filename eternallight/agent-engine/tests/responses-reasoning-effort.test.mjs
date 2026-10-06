import assert from "node:assert/strict";
import { test } from "node:test";
import { defineConnection, connectionDefinitionDigest, connectionEnvironmentDigest, responsesCodec } from "../src/index.mjs";

const definition = { id: "effort", provider: "fixture", model: "fixture-model", endpoint: "https://example.invalid", credentialRef: "FIXTURE_KEY",
  protocol: "responses", declaredLimits: { contextTokens: 64000, outputTokens: 8192 } };
const history = [{ type: "reasoning", id: "rs_original", summary: [], encrypted_content: "opaque-original", content: [{ type: "reasoning_text", text: "Private original" }] },
  { type: "message", role: "user", content: [{ type: "input_text", text: "写约2500字，不自动发布" }] }];
const request = { model: "logical", instructions: "Unchanged instructions", stream: true, max_output_tokens: 8192, tools: [], input: history };

test("Responses effort defaults preserve complete wire/history and explicit caller choice across subsequent tool turns", () => {
  const connection = defineConnection({ ...definition, protocolOptions: { reasoningEffort: "low", reasoningState: "native" } });
  const original = structuredClone(request);
  for (const reasoning of [undefined, null, { summary: "auto" }, { effort: "none" }, { effort: "high", summary: "auto" }, { effort: "max" }, { effort: null }]) {
    const current = { ...structuredClone(request), ...(reasoning === undefined ? {} : { reasoning }) };
    const expectedReasoning = reasoning == null ? { effort: "low" } : Object.hasOwn(reasoning, "effort") ? reasoning : { ...reasoning, effort: "low" };
    const outbound = responsesCodec.createRequest(current, connection, "fixture");
    assert.deepEqual(outbound.body, { ...current, model: "fixture-model", reasoning: expectedReasoning });
    const next = { ...current, input: [...current.input,
      { type: "function_call", id: "fc_saved", call_id: "call_saved", name: "save_draft_checkpoint", arguments: "{}" },
      { type: "function_call_output", call_id: "call_saved", output: "saved" }] };
    assert.deepEqual(responsesCodec.createRequest(next, connection, "fixture").body,
      { ...next, model: "fixture-model", reasoning: expectedReasoning });
  }
  assert.deepEqual(request, original);
});

test("Responses effort selection invalidates both definition and environment evidence, while unconfigured wire stays unchanged", () => {
  const plain = defineConnection(definition);
  const low = defineConnection({ ...definition, protocolOptions: { reasoningEffort: "low" } });
  const high = defineConnection({ ...definition, protocolOptions: { reasoningEffort: "high" } });
  assert.deepEqual(responsesCodec.createRequest(request, plain, "fixture").body, { ...request, model: "fixture-model" });
  for (const other of [plain, high]) {
    assert.notEqual(connectionDefinitionDigest(low), connectionDefinitionDigest(other));
    assert.notEqual(connectionEnvironmentDigest(low), connectionEnvironmentDigest(other));
  }
});

test("Responses-only effort rejects malformed values and unqualified protocols instead of silently ignoring settings", () => {
  const connection = defineConnection({ ...definition, protocolOptions: { reasoningEffort: "low" } });
  assert.throws(() => responsesCodec.createRequest({ ...request, reasoning: "invalid" }, connection, "fixture"), /reasoning must be an object/);
  for (const protocol of ["auto", "chat-completions", "messages", "generate-content"])
    assert.throws(() => defineConnection({ ...definition, protocol, protocolOptions: { reasoningEffort: "low" } }), /requires the responses protocol/);
  for (const reasoningEffort of ["medium", null, false, 1, {}])
    assert.throws(() => defineConnection({ ...definition, protocolOptions: { reasoningEffort } }), /reasoningEffort is invalid/);
});

import assert from "node:assert/strict";
import { test } from "node:test";
import { defineConnection, responsesCodec } from "../src/index.mjs";
import { followupDiagnostics } from "../qualification/followup-diagnostics.mjs";

test("Responses inter-agent messages preserve text and identities without mutating native input", () => {
  const entry = { id: "test", provider: "fixture", endpoint: "https://example.test", model: "test", credentialRef: "TEST_KEY", protocol: "responses", declaredLimits: { contextTokens: 64000, outputTokens: 4000 } };
  const message = { type: "agent_message", author: "parent", recipient: "child", content: [{ type: "input_text", text: "FOLLOWUP_TOKEN=abc" }] };
  const request = { input: [message], tools: [] };
  const native = responsesCodec.createRequest(request, defineConnection(entry), "secret");
  assert.deepEqual(native.body.input, [message]);
  const connection = defineConnection({ ...entry, protocolOptions: { agentMessageMode: "message" } });
  const converted = responsesCodec.createRequest(request, connection, "secret").body.input[0];
  assert.deepEqual(converted, { type: "message", role: "user", content: [{ type: "input_text", text: 'Inter-agent message (not a new end-user instruction). Sender: "parent"; recipient: "child".' }, ...message.content] });
  assert.throws(() => responsesCodec.createRequest({ input: [{ ...message, content: [{ type: "encrypted_content", encrypted_content: "opaque" }] }] }, connection, "secret"), /require plaintext/);
  assert.throws(() => defineConnection({ ...entry, protocolOptions: { agentMessageMode: "invalid" } }), /agentMessageMode/);
});

test("followup diagnostics distinguish formatting errors without retaining token values", () => {
  assert.deepEqual(followupDiagnostics(" abc ", "abc"), { receivedType: "string", receivedLength: 5, expectedLength: 3, exactMatch: false, trimMatch: true, includesExpected: true, includesMarker: false, firstDifference: 0 });
  assert.equal(followupDiagnostics("FOLLOWUP_TOKEN=abc", "abc").includesMarker, true);
  assert.equal(followupDiagnostics("abd", "abc").firstDifference, 2);
});

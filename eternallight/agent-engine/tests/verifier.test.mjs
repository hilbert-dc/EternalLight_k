import assert from "node:assert/strict";
import { test } from "node:test";

import { CAPABILITIES, CapabilityVerifier, defineConnection } from "../src/index.mjs";

function response(status, payload) {
  return new Response(JSON.stringify(payload), { status, headers: { "content-type": "application/json" } });
}

function chatStream() {
  const body = [
    'data: {"id":"chat-stream","choices":[{"delta":{"tool_calls":[{"index":0,"id":"probe-call-stream","function":{"name":"f__engine_probe","arguments":"{\\"nonce\\":\\"agent-engine\\"}"}}]}}]}',
    'data: {"id":"chat-stream","choices":[{"delta":{},"finish_reason":"tool_calls"}]}',
    'data: {"id":"chat-stream","choices":[],"usage":{"prompt_tokens":20,"completion_tokens":4,"total_tokens":24}}',
    "data: [DONE]",
  ].join("\n\n");
  return new Response(body, { status: 200, headers: { "content-type": "text/event-stream" } });
}

test("auto verification selects the first conforming protocol without model-specific rules", async () => {
  const requests = [];
  const verifier = new CapabilityVerifier({
    fetchImpl: async (url, options) => {
      requests.push({ url, body: JSON.parse(options.body) });
      if (url.endsWith("/v1/responses")) return response(404, { error: { message: "not supported" } });
      if (requests.at(-1).body.stream) return chatStream();
      const toolName = requests.at(-1).body.tools[0].function.name;
      return response(200, {
        id: "chat-probe",
        choices: [{ finish_reason: "tool_calls", message: { role: "assistant", content: null, tool_calls: [{ id: "probe-call", type: "function", function: { name: "f__engine_probe", arguments: "{\"nonce\":\"agent-engine\"}" } }] } }],
        usage: { prompt_tokens: 20, completion_tokens: 4, total_tokens: 24 },
      });
    },
  });
  const connection = defineConnection({ id: "auto-model", provider: "Any", endpoint: "https://models.example.test", model: "model-x", credentialRef: "MODEL_X_KEY", protocol: "auto", declaredLimits: { contextTokens: 32_000, outputTokens: 2_000 } });
  const result = await verifier.verify(connection, { apiKey: "secret", protocols: ["responses", "chat-completions"], now: new Date("2026-09-19T00:00:00Z") });
  assert.equal(result.selectedProtocol, "chat-completions");
  assert.deepEqual(result.attempts, [
    { protocol: "responses", status: "failed", category: "unsupportedProtocol", message: "not supported" },
    { protocol: "chat-completions", status: "verified" },
  ]);
  assert.deepEqual(result.records.map((record) => [record.capability, record.status]), [
    [CAPABILITIES.TEXT, "failed"],
    [CAPABILITIES.STRUCTURED_TOOL_CALLS, "failed"],
    [CAPABILITIES.TEXT, "verified"],
    [CAPABILITIES.STRUCTURED_TOOL_CALLS, "verified"],
    [CAPABILITIES.STREAMING, "verified"],
    [CAPABILITIES.USAGE, "verified"],
  ]);
  assert.equal(requests[1].body.model, "model-x");
  assert.equal(requests[1].body.tools[0].function.name, "f__engine_probe");
  assert.equal(requests[2].body.stream, true);
});

test("verification does not certify native streaming when a codec only supports buffered conversion", async () => {
  const verifier = new CapabilityVerifier({
    fetchImpl: async () => response(200, {
      id: "msg-probe",
      content: [{ type: "tool_use", id: "probe-call", name: "f__engine_probe", input: { nonce: "agent-engine" } }],
      stop_reason: "tool_use",
      usage: { input_tokens: 20, output_tokens: 4 },
    }),
  });
  const connection = defineConnection({ id: "messages-model", provider: "Any", endpoint: "https://models.example.test", model: "model-y", credentialRef: "MODEL_Y_KEY", protocol: "messages", declaredLimits: { contextTokens: 32_000, outputTokens: 2_000 } });
  const result = await verifier.verify(connection, { apiKey: "secret", now: new Date("2026-09-19T00:00:00Z") });
  const streaming = result.records.find((record) => record.capability === CAPABILITIES.STREAMING);
  assert.equal(streaming.status, "unknown");
  assert.equal(streaming.sample.reason, "codec-does-not-support-native-streaming");
});

test("verification retries without forced tool choice when a thinking model rejects it", async () => {
  const requests = [];
  const verifier = new CapabilityVerifier({
    fetchImpl: async (_url, options) => {
      const body = JSON.parse(options.body);
      requests.push(body);
      if (body.tool_choice) return response(400, { error: { message: "Thinking mode does not support this tool_choice" } });
      if (body.stream) return chatStream();
      return response(200, {
        id: "thinking-probe",
        choices: [{ finish_reason: "tool_calls", message: { role: "assistant", content: null, tool_calls: [{ id: "probe-call", type: "function", function: { name: "f__engine_probe", arguments: "{\"nonce\":\"agent-engine\"}" } }] } }],
        usage: { prompt_tokens: 20, completion_tokens: 4, total_tokens: 24 },
      });
    },
  });
  const connection = defineConnection({ id: "thinking-model", provider: "Any", endpoint: "https://models.example.test", model: "model-thinking", credentialRef: "MODEL_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 32_000, outputTokens: 2_000 } });
  const result = await verifier.verify(connection, { apiKey: "secret" });
  assert.equal(result.selectedProtocol, "chat-completions");
  assert.equal(requests.length, 3);
  assert.equal(requests[0].tool_choice, "required");
  assert.equal("tool_choice" in requests[1], false);
  assert.equal("tool_choice" in requests[2], false);
  assert.equal(requests[2].stream, true);
});

test("verification stops after authentication failure and exposes no credential in evidence", async () => {
  const verifier = new CapabilityVerifier({ fetchImpl: async () => response(401, { error: { message: "unauthorized" } }) });
  const connection = defineConnection({ id: "auto-model", provider: "Any", endpoint: "https://models.example.test", model: "model-x", credentialRef: "MODEL_X_KEY", protocol: "auto", declaredLimits: { contextTokens: 32_000, outputTokens: 2_000 } });
  await assert.rejects(
    verifier.verify(connection, { apiKey: "never-persist-this", protocols: ["responses", "chat-completions"] }),
    (error) => {
      assert.equal(error.attempts.length, 1);
      assert.equal(error.attempts[0].category, "authentication");
      assert.deepEqual(error.records.map((record) => [record.capability, record.status, record.sample.category]), [
        [CAPABILITIES.TEXT, "failed", "authentication"],
        [CAPABILITIES.STRUCTURED_TOOL_CALLS, "failed", "authentication"],
      ]);
      assert.doesNotMatch(JSON.stringify(error.attempts), /never-persist-this/);
      return true;
    },
  );
});

test("streaming authentication failures revoke the protocol attempt instead of becoming unknown", async () => {
  let requestCount = 0;
  const verifier = new CapabilityVerifier({
    fetchImpl: async (_url, options) => {
      requestCount += 1;
      return JSON.parse(options.body).stream
        ? response(401, { error: { message: "stream credential rejected" } })
        : response(200, {
          id: "chat-probe",
          choices: [{ finish_reason: "tool_calls", message: { role: "assistant", content: null, tool_calls: [{ id: "probe-call", type: "function", function: { name: "f__engine_probe", arguments: "{\"nonce\":\"agent-engine\"}" } }] } }],
        });
    },
  });
  const connection = defineConnection({ id: "chat-model", provider: "Any", endpoint: "https://models.example.test", model: "model-x", credentialRef: "MODEL_X_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 32_000, outputTokens: 2_000 } });
  await assert.rejects(verifier.verify(connection, { apiKey: "secret" }), (error) => {
    assert.equal(error.attempts[0].category, "authentication");
    assert.equal(error.records.every((record) => record.status === "failed"), true);
    return true;
  });
  assert.equal(requestCount, 2);
});

test("verification bounds response bytes and distinguishes non-authentication 403", async () => {
  const connection = defineConnection({ id: "chat-model", provider: "Any", endpoint: "https://models.example.test", model: "model-x", credentialRef: "MODEL_X_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 32_000, outputTokens: 2_000 } });
  const oversized = new CapabilityVerifier({
    maxResponseBytes: 64,
    fetchImpl: async () => response(200, { padding: "x".repeat(256) }),
  });
  await assert.rejects(oversized.verify(connection, { apiKey: "secret" }), (error) => {
    assert.equal(error.attempts[0].category, "transport");
    assert.match(error.attempts[0].message, /byte limit/);
    return true;
  });

  const forbidden = new CapabilityVerifier({
    fetchImpl: async () => response(403, { error: { type: "permission_denied", message: "model is not enabled" } }),
  });
  await assert.rejects(forbidden.verify(connection, { apiKey: "secret" }), (error) => {
    assert.equal(error.attempts[0].category, "invalidRequest");
    return true;
  });
});

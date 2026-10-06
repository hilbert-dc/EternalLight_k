import assert from "node:assert/strict";
import { test } from "node:test";

import { chatCompletionsCodec, defineConnection, generateContentCodec, messagesCodec, responsesCodec } from "../src/index.mjs";

const request = {
  model: "logical-model",
  instructions: "Use tools when needed.",
  input: [
    { type: "message", role: "user", content: [{ type: "input_text", text: "Read the project" }] },
    { type: "function_call", call_id: "call_1", name: "read_context", arguments: "{\"projectId\":\"p1\"}" },
    { type: "function_call_output", call_id: "call_1", output: "{\"title\":\"Story\"}" },
  ],
  tools: [{ type: "function", name: "read_context", description: "Read context", parameters: { type: "object", properties: { projectId: { type: "string" } }, required: ["projectId"] } }],
  stream: true,
};

function connection(protocol) {
  return defineConnection({ id: protocol, provider: protocol, endpoint: "https://api.example.test", model: `model-${protocol}`, credentialRef: "TEST_API_KEY", protocol, declaredLimits: { contextTokens: 100_000, outputTokens: 4_000 } });
}

test("chat-completions codec preserves tool call correlation in both directions", () => {
  const outbound = chatCompletionsCodec.createRequest(request, connection("chat-completions"), "secret");
  assert.equal(outbound.url, "https://api.example.test/v1/chat/completions");
  assert.equal(outbound.headers.authorization, "Bearer secret");
  assert.equal(outbound.body.stream, true);
  assert.deepEqual(outbound.body.stream_options, { include_usage: true });
  assert.equal(outbound.body.messages[2].tool_calls[0].id, "call_1");
  assert.equal(outbound.body.messages[3].tool_call_id, "call_1");
  const response = chatCompletionsCodec.parseResponse({ id: "chat-1", choices: [{ finish_reason: "tool_calls", message: { role: "assistant", content: "", tool_calls: [{ id: "call_2", type: "function", function: { name: "f__read_context", arguments: "{\"projectId\":\"p1\"}" } }] } }], usage: { prompt_tokens: 10, completion_tokens: 4, total_tokens: 14 } }, outbound.context);
  assert.equal(response.output[0].call_id, "call_2");
  assert.equal(response.output[0].name, "read_context");
  assert.deepEqual(response.usage, { input_tokens: 10, output_tokens: 4, total_tokens: 14, input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } });
});

test("chat-completions codec emits incremental Responses events", async () => {
  const stream = [
    'data: {"id":"chat-stream","choices":[{"delta":{"content":"正在"}}]}',
    'data: {"id":"chat-stream","choices":[{"delta":{"content":"读取","tool_calls":[{"index":0,"id":"call_2","function":{"name":"f__read_context","arguments":"{\\"project"}}]}}]}',
    'data: {"id":"chat-stream","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"Id\\":\\"p1\\"}"}}]}}],"usage":{"prompt_tokens":8,"completion_tokens":3,"total_tokens":11}}',
    'data: {"id":"chat-stream","choices":[{"delta":{},"finish_reason":"tool_calls"}]}',
    "data: [DONE]",
  ].join("\n\n");
  const events = [];
  const context = chatCompletionsCodec.createRequest(request, connection("chat-completions"), "secret").context;
  for await (const event of chatCompletionsCodec.parseStream(new Response(stream).body, context)) events.push(event);
  assert.deepEqual(events.filter((event) => event.type === "response.output_text.delta").map((event) => event.delta), ["正在", "读取"]);
  const completed = events.at(-1);
  assert.equal(completed.type, "response.completed");
  assert.equal(completed.response.output[0].content[0].text, "正在读取");
  assert.deepEqual(completed.response.output[1], {
    id: "fc_call_2",
    type: "function_call",
    call_id: "call_2",
    name: "read_context",
    arguments: '{"projectId":"p1"}',
    status: "completed",
  });
  assert.deepEqual(completed.response.usage, { input_tokens: 8, output_tokens: 3, total_tokens: 11, input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } });
});

test("chat-completions codec rejects truncated streams instead of emitting completion", async () => {
  const stream = 'data: {"id":"truncated","choices":[{"delta":{"content":"partial"}}]}\n\ndata: [DONE]\n\n';
  await assert.rejects(async () => {
    const context = chatCompletionsCodec.createRequest(request, connection("chat-completions"), "secret").context;
    for await (const _event of chatCompletionsCodec.parseStream(new Response(stream).body, context)) {
      // Consume the stream to force terminal validation.
    }
  }, /finish reason/);
});

test("chat-completions never commits a finished tool candidate without the protocol end marker", async () => {
  const stream = [
    'data: {"id":"missing-done","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-late","function":{"name":"f__read_context","arguments":"{\\"projectId\\":\\"p1\\"}"}}]}}]}',
    'data: {"id":"missing-done","choices":[{"delta":{},"finish_reason":"tool_calls"}]}',
  ].join("\n\n");
  const context = chatCompletionsCodec.createRequest(request, connection("chat-completions"), "secret").context;
  const events = [];
  await assert.rejects(async () => {
    for await (const event of chatCompletionsCodec.parseStream(new Response(stream).body, context)) events.push(event);
  }, /without \[DONE\]/);
  assert.equal(events.some((event) => event.type === "response.output_item.done"), false);
});

test("buffered codecs preserve incomplete and filtered terminal states", () => {
  const chat = chatCompletionsCodec.parseResponse({ id: "chat-short", choices: [{ finish_reason: "length", message: { role: "assistant", content: "partial" } }] });
  assert.equal(chat.status, "incomplete");
  const messages = messagesCodec.parseResponse({ id: "msg-short", stop_reason: "max_tokens", content: [{ type: "text", text: "partial" }] });
  assert.equal(messages.status, "incomplete");
  const generate = generateContentCodec.parseResponse({ responseId: "gen-filtered", candidates: [{ finishReason: "SAFETY", content: { parts: [] } }] });
  assert.equal(generate.status, "failed");
});

test("chat-completions codec flattens and restores Codex MCP namespaces", () => {
  const namespacedRequest = {
    ...request,
    stream: false,
    tools: [{ type: "namespace", name: "mcp__novel_workspace", description: "Novel tools", tools: [{ type: "function", name: "read_context", description: "Read context", parameters: { type: "object", properties: {} }, strict: false }] }],
  };
  const outbound = chatCompletionsCodec.createRequest(namespacedRequest, connection("chat-completions"), "secret");
  assert.equal(outbound.body.tools[0].function.name, "n20__mcp__novel_workspace__read_context");
  const response = chatCompletionsCodec.parseResponse({ id: "chat-ns", choices: [{ finish_reason: "tool_calls", message: { role: "assistant", tool_calls: [{ id: "call_ns", type: "function", function: { name: "n20__mcp__novel_workspace__read_context", arguments: "{}" } }] } }] }, outbound.context);
  assert.deepEqual(response.output[0], {
    id: response.output[0].id,
    type: "function_call",
    call_id: "call_ns",
    name: "read_context",
    namespace: "mcp__novel_workspace",
    arguments: "{}",
    status: "completed",
  });
});

test("chat-completions codec restores non-MCP Codex namespaces", () => {
  const namespacedRequest = {
    ...request,
    stream: false,
    tools: [{ type: "namespace", name: "multi_agent_v2", description: "Delegation tools", tools: [{ type: "function", name: "spawn_agent", description: "Delegate work", parameters: { type: "object", properties: {} }, strict: false }] }],
  };
  const outbound = chatCompletionsCodec.createRequest(namespacedRequest, connection("chat-completions"), "secret");
  assert.equal(outbound.body.tools[0].function.name, "n14__multi_agent_v2__spawn_agent");
  const response = chatCompletionsCodec.parseResponse({ id: "chat-agent", choices: [{ finish_reason: "tool_calls", message: { role: "assistant", tool_calls: [{ id: "call_spawn", type: "function", function: { name: "n14__multi_agent_v2__spawn_agent", arguments: "{}" } }] } }] }, outbound.context);
  assert.deepEqual(response.output[0], {
    id: response.output[0].id,
    type: "function_call",
    call_id: "call_spawn",
    name: "spawn_agent",
    namespace: "multi_agent_v2",
    arguments: "{}",
    encrypted_function_args: [],
    status: "completed",
  });
});

test("adapted delegation calls are explicitly marked as plaintext", () => {
  for (const name of ["spawn_agent", "send_message", "followup_task"]) {
    const delegated = { ...request, tools: [{ type: "namespace", name: "multi_agent_v2", tools: [{ type: "function", name, parameters: { type: "object", properties: {} } }] }] };
    const context = chatCompletionsCodec.createRequest(delegated, connection("chat-completions"), "secret").context;
    const response = chatCompletionsCodec.parseResponse({
      id: `chat-${name}`,
      choices: [{ finish_reason: "tool_calls", message: { role: "assistant", tool_calls: [{ id: `call-${name}`, type: "function", function: { name: `n14__multi_agent_v2__${name}`, arguments: "{}" } }] } }],
    }, context);
    assert.deepEqual(response.output[0].encrypted_function_args, []);
  }
});

test("protocol conversion validates and strips Codex encrypted schema annotations", () => {
  const delegated = {
    ...request,
    tools: [{
      type: "namespace",
      name: "multi_agent_v2",
      tools: [{
        type: "function",
        name: "spawn_agent",
        parameters: {
          type: "object",
          properties: { message: { type: "string", encrypted: true } },
          required: ["message"],
        },
      }],
    }],
  };
  const converted = chatCompletionsCodec.createRequest(delegated, connection("chat-completions"), "secret");
  assert.equal(converted.body.tools[0].function.parameters.properties.message.encrypted, undefined);
  assert.equal(converted.context.tools[0].parameters.properties.message.encrypted, undefined);
  assert.throws(
    () => chatCompletionsCodec.createRequest({ ...delegated, tools: [{ ...delegated.tools[0], tools: [{ ...delegated.tools[0].tools[0], parameters: { type: "object", encrypted: "yes" } }] }] }, connection("chat-completions"), "secret"),
    /encrypted annotation must be boolean/,
  );
});

test("failed or incomplete responses never commit executable tool items", async () => {
  for (const finish_reason of ["length", "content_filter"]) {
    const dangerous = { ...request, tools: [{ type: "function", name: "danger", parameters: { type: "object", properties: {} } }] };
    const context = chatCompletionsCodec.createRequest(dangerous, connection("chat-completions"), "secret").context;
    const response = chatCompletionsCodec.parseResponse({
      id: `chat-${finish_reason}`,
      choices: [{ finish_reason, message: { role: "assistant", tool_calls: [{ id: "call-danger", type: "function", function: { name: "f__danger", arguments: "{}" } }] } }],
    }, context);
    assert.deepEqual(response.output, []);
  }
  const stream = [
    'data: {"id":"filtered","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-danger","function":{"name":"f__danger","arguments":"{}"}}]}}]}',
    'data: {"id":"filtered","choices":[{"delta":{},"finish_reason":"content_filter"}]}',
    "data: [DONE]",
  ].join("\n\n");
  const events = [];
  const context = chatCompletionsCodec.createRequest({ ...request, tools: [{ type: "function", name: "danger", parameters: { type: "object", properties: {} } }] }, connection("chat-completions"), "secret").context;
  for await (const event of chatCompletionsCodec.parseStream(new Response(stream).body, context)) events.push(event);
  assert.equal(events.some((event) => event.type === "response.output_item.done"), false);
  assert.equal(events.at(-1).type, "response.failed");
  assert.deepEqual(events.at(-1).response.output, []);
});

test("adapted codecs reject opaque, standalone, reordered and lossy states", () => {
  const opaque = { ...request, input: [{ type: "message", role: "user", content: [{ type: "encrypted_content", encrypted_content: "opaque" }] }] };
  assert.throws(() => chatCompletionsCodec.createRequest(opaque, connection("chat-completions"), "secret"), /native Responses route/);
  const standalone = { ...request, input: [{ type: "function_call_output", call_id: "orphan", output: "result" }] };
  assert.throws(() => messagesCodec.createRequest(standalone, connection("messages"), "secret"), /standalone tool output/);
  const developerHistory = { ...request, input: [{ type: "message", role: "user", content: "first" }, { type: "message", role: "developer", content: "later" }] };
  assert.throws(() => messagesCodec.createRequest(developerHistory, connection("messages"), "secret"), /cannot preserve/);
  const initialDeveloper = { ...request, input: [{ type: "message", role: "developer", content: "prefix policy" }, { type: "message", role: "user", content: "first" }] };
  assert.match(messagesCodec.createRequest(initialDeveloper, connection("messages"), "secret").body.system, /<developer>\nprefix policy/);
  assert.match(generateContentCodec.createRequest(initialDeveloper, connection("generate-content"), "secret").body.systemInstruction.parts[0].text, /<developer>\nprefix policy/);
  const unknownSchema = { ...request, tools: [{ ...request.tools[0], parameters: { type: "object", unsupportedKeyword: true } }] };
  assert.throws(() => generateContentCodec.createRequest(unknownSchema, connection("generate-content"), "secret"), /unsupported JSON Schema keyword/);
});

test("tool aliases stay within provider limits and restore exact names", () => {
  const namespace = `namespace_${"n".repeat(90)}`;
  const name = `tool_${"x".repeat(90)}`;
  const longRequest = { ...request, tools: [{ type: "namespace", name: namespace, tools: [{ type: "function", name, parameters: { type: "object" } }] }] };
  const outbound = chatCompletionsCodec.createRequest(longRequest, connection("chat-completions"), "secret");
  const alias = outbound.body.tools[0].function.name;
  assert.ok(alias.length <= 64);
  const response = chatCompletionsCodec.parseResponse({ id: "long-tool", choices: [{ finish_reason: "tool_calls", message: { role: "assistant", tool_calls: [{ id: "call-long", type: "function", function: { name: alias, arguments: "{}" } }] } }] }, outbound.context);
  assert.equal(response.output[0].namespace, namespace);
  assert.equal(response.output[0].name, name);
});

test("adapted codecs reject missing call ids and over-limit model items", () => {
  const outbound = chatCompletionsCodec.createRequest(request, connection("chat-completions"), "secret");
  assert.throws(() => chatCompletionsCodec.parseResponse({ id: "missing-id", choices: [{ finish_reason: "tool_calls", message: { role: "assistant", tool_calls: [{ type: "function", function: { name: "f__read_context", arguments: "{\"projectId\":\"p1\"}" } }] } }] }, outbound.context), /tool call id/);
  assert.throws(() => chatCompletionsCodec.createRequest({ ...request, instructions: "界".repeat(12_000) }, connection("chat-completions"), "secret"), /10K-token/);
  assert.throws(() => chatCompletionsCodec.createRequest({ ...request, instructions: "界".repeat(13_334) }, connection("chat-completions"), "secret"), /10K-token/);
});

test("messages codec converts native tool use to canonical Responses items", () => {
  const outbound = messagesCodec.createRequest(request, connection("messages"), "secret");
  assert.equal(outbound.url, "https://api.example.test/v1/messages");
  assert.equal(outbound.body.tools[0].input_schema.type, "object");
  assert.equal(outbound.body.messages[1].content[0].id, "call_1");
  const response = messagesCodec.parseResponse({ id: "msg-1", stop_reason: "tool_use", content: [{ type: "text", text: "Checking." }, { type: "tool_use", id: "call_2", name: "f__read_context", input: { projectId: "p1" } }], usage: { input_tokens: 8, output_tokens: 3 } }, outbound.context);
  assert.deepEqual(response.output.map((item) => item.type), ["message", "function_call"]);
  assert.equal(response.output[1].arguments, "{\"projectId\":\"p1\"}");
});

test("messages codec restores namespaced tool calls", () => {
  const namespaced = { ...request, tools: [{ type: "namespace", name: "mcp__novel_workspace", tools: [{ type: "function", name: "read_context", parameters: { type: "object", properties: {} } }] }] };
  const outbound = messagesCodec.createRequest(namespaced, connection("messages"), "secret");
  const response = messagesCodec.parseResponse({ id: "msg-ns", stop_reason: "tool_use", content: [{ type: "tool_use", id: "call_ns", name: "n20__mcp__novel_workspace__read_context", input: {} }] }, outbound.context);
  assert.equal(response.output[0].namespace, "mcp__novel_workspace");
  assert.equal(response.output[0].name, "read_context");
});

test("generate-content codec maps functions without model-specific rules", () => {
  const outbound = generateContentCodec.createRequest(request, connection("generate-content"), "secret");
  assert.match(outbound.url, /v1beta\/models\/model-generate-content:generateContent\?key=secret$/);
  assert.equal(outbound.body.tools[0].functionDeclarations[0].name, "f__read_context");
  assert.equal(outbound.body.tools[0].functionDeclarations[0].parametersJsonSchema.type, "object");
  const response = generateContentCodec.parseResponse({ responseId: "gen-1", candidates: [{ finishReason: "STOP", content: { parts: [{ functionCall: { id: "call_3", name: "f__read_context", args: { projectId: "p1" } } }] } }], usageMetadata: { promptTokenCount: 7, candidatesTokenCount: 2, totalTokenCount: 9 } }, outbound.context);
  assert.equal(response.output[0].call_id, "call_3");
  assert.equal(response.output[0].name, "read_context");
});

test("generate-content codec correlates namespaced function responses by call id", () => {
  const namespaced = {
    ...request,
    input: [
      request.input[0],
      { type: "function_call", call_id: "call_ns", namespace: "mcp__novel_workspace", name: "read_context", arguments: "{}" },
      { type: "function_call_output", call_id: "call_ns", output: "fixture-context" },
    ],
    tools: [{ type: "namespace", name: "mcp__novel_workspace", tools: [{ type: "function", name: "read_context", parameters: { type: "object" } }] }],
  };
  const outbound = generateContentCodec.createRequest(namespaced, connection("generate-content"), "secret");
  assert.equal(outbound.body.contents[1].parts[0].functionCall.name, "n20__mcp__novel_workspace__read_context");
  assert.equal(outbound.body.contents[2].parts[0].functionResponse.name, "n20__mcp__novel_workspace__read_context");
  assert.equal(outbound.body.contents[2].parts[0].functionResponse.id, "call_ns");
  const response = generateContentCodec.parseResponse({ responseId: "gen-ns", candidates: [{ finishReason: "STOP", content: { parts: [{ functionCall: { id: "call_ns", name: "n20__mcp__novel_workspace__read_context", args: {} } }] } }] }, outbound.context);
  assert.equal(response.output[0].namespace, "mcp__novel_workspace");
  assert.equal(response.output[0].name, "read_context");
});

test("provider-private encrypted reasoning is rejected outside native Responses", () => {
  assert.throws(() => chatCompletionsCodec.createRequest({
    model: "model-a",
    input: [{ type: "message", role: "user", content: [{ type: "encrypted_content", encrypted_content: "private-state" }, { type: "input_text", text: "visible" }] }],
    stream: false,
  }, connection("chat-completions"), "secret"), /native Responses route/);
});

test("tool responses are validated atomically against the request catalog and schema", () => {
  const outbound = chatCompletionsCodec.createRequest(request, connection("chat-completions"), "secret");
  const response = (calls) => ({ id: "validation", choices: [{ finish_reason: "tool_calls", message: { role: "assistant", tool_calls: calls } }] });
  assert.throws(() => chatCompletionsCodec.parseResponse(response([{ id: "unknown", type: "function", function: { name: "f__unknown", arguments: "{}" } }]), outbound.context), /not authorized/);
  assert.throws(() => chatCompletionsCodec.parseResponse(response([{ id: "missing", type: "function", function: { name: "f__read_context", arguments: "{}" } }]), outbound.context), /projectId/);
  assert.throws(() => chatCompletionsCodec.parseResponse(response([{ id: "scalar", type: "function", function: { name: "f__read_context", arguments: "1" } }]), outbound.context), /object/);
  assert.throws(() => chatCompletionsCodec.parseResponse(response([
    { id: "duplicate", type: "function", function: { name: "f__read_context", arguments: "{\"projectId\":\"p1\"}" } },
    { id: "duplicate", type: "function", function: { name: "f__read_context", arguments: "{\"projectId\":\"p1\"}" } },
  ]), outbound.context), /duplicate tool call id/);
});

test("JSON Schema validation uses bounded standards semantics", () => {
  const parse = (schema, value) => {
    const constrained = { ...request, tools: [{ type: "function", name: "validate", parameters: schema }] };
    const outbound = chatCompletionsCodec.createRequest(constrained, connection("chat-completions"), "secret");
    return () => chatCompletionsCodec.parseResponse({
      id: "schema",
      choices: [{ finish_reason: "tool_calls", message: { tool_calls: [{ id: "schema-call", type: "function", function: { name: "f__validate", arguments: JSON.stringify(value) } }] } }],
    }, outbound.context);
  };
  assert.throws(parse({ type: "object", properties: { items: { type: "array", items: false } }, required: ["items"] }, { items: [1] }), /"reason":"constraint".*"schemaPath":"\/properties\/items\/items"/);
  assert.throws(parse({ type: "object", properties: { constructor: { type: "string" } }, required: ["constructor"] }, {}), /required/);
  assert.throws(parse({ type: "object", properties: { value: { type: "string", minLength: 2 } }, required: ["value"] }, { value: "🙂" }), /"reason":"constraint".*"schemaPath":"\/properties\/value\/minLength"/);
  assert.throws(parse({ type: "object", properties: { value: { not: { const: { a: 1, b: 2 } } } } }, { value: { b: 2, a: 1 } }), /"reason":"constraint".*"schemaPath":"\/properties\/value\/not"/);
  assert.throws(() => chatCompletionsCodec.createRequest({ ...request, tools: [{ ...request.tools[0], parameters: { type: "object", properties: { value: { type: "string", pattern: "^(a+)+$" } } } }] }, connection("chat-completions"), "secret"), /unbounded regular expressions/);
});

test("request limits include tools, merged items, and output schemas", () => {
  assert.throws(() => chatCompletionsCodec.createRequest({ ...request, tools: [{ ...request.tools[0], description: "界".repeat(12_000) }] }, connection("chat-completions"), "secret"), /description.*10K-token/);
  assert.throws(() => chatCompletionsCodec.createRequest({
    ...request,
    text: { format: { type: "json_schema", name: "oversized", schema: { type: "object", description: "界".repeat(12_000) } } },
  }, connection("chat-completions"), "secret"), /output schema.*10K-token/);
  const calls = [
    ...[0, 1, 2].map((index) => ({ type: "function_call", call_id: `merge-${index}`, name: "read_context", arguments: '{"projectId":"p1"}' })),
    ...[0, 1, 2].map((index) => ({ type: "function_call_output", call_id: `merge-${index}`, output: "界".repeat(4_000) })),
  ];
  assert.throws(() => messagesCodec.createRequest({ ...request, input: [request.input[0], ...calls] }, connection("messages"), "secret"), /encoded model item.*10K-token/);
});

test("tool choice and opaque history are enforced at commit", () => {
  const none = chatCompletionsCodec.createRequest({ ...request, tool_choice: "none" }, connection("chat-completions"), "secret");
  assert.throws(() => chatCompletionsCodec.parseResponse({ id: "none", choices: [{ finish_reason: "tool_calls", message: { tool_calls: [{ id: "bad", type: "function", function: { name: "f__read_context", arguments: '{"projectId":"p1"}' } }] } }] }, none.context), /tool_choice is none/);
  const required = chatCompletionsCodec.createRequest({ ...request, tool_choice: "required" }, connection("chat-completions"), "secret");
  assert.throws(() => chatCompletionsCodec.parseResponse({ id: "required", choices: [{ finish_reason: "stop", message: { content: "no tool" } }] }, required.context), /required tool call/);
  assert.throws(() => chatCompletionsCodec.createRequest({
    ...request,
    input: [{ type: "function_call", call_id: "opaque", name: "read_context", arguments: '{"projectId":"p1"}', encrypted_function_args: ["ciphertext"] }],
  }, connection("chat-completions"), "secret"), /native Responses route/);
});

test("chat streaming rejects provider errors after an apparent tool finish", async () => {
  const stream = [
    'data: {"id":"late-error","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-late","function":{"name":"f__read_context","arguments":"{\\"projectId\\":\\"p1\\"}"}}]}}]}',
    'data: {"id":"late-error","choices":[{"delta":{},"finish_reason":"tool_calls"}]}',
    'data: {"error":{"message":"provider disconnected"}}',
    "data: [DONE]",
  ].join("\n\n");
  const context = chatCompletionsCodec.createRequest(request, connection("chat-completions"), "secret").context;
  await assert.rejects(async () => {
    for await (const _event of chatCompletionsCodec.parseStream(new Response(stream).body, context)) { /* consume */ }
  }, /chat-completions provider returned an error frame/);
});

test("protocol usage and output limits remain conservative and explicit", () => {
  const chatConnection = defineConnection({ ...connection("chat-completions"), protocolOptions: { maxOutputTokensField: "max_completion_tokens" } });
  const chat = chatCompletionsCodec.createRequest({ ...request, max_output_tokens: 777 }, chatConnection, "secret");
  assert.equal(chat.body.max_completion_tokens, 777);
  assert.equal(chat.body.max_tokens, undefined);
  const messages = messagesCodec.createRequest({ ...request, parallel_tool_calls: false }, connection("messages"), "secret");
  assert.deepEqual(messages.body.tool_choice, { type: "auto", disable_parallel_tool_use: true });
  const parsed = messagesCodec.parseResponse({ id: "cache", stop_reason: "end_turn", content: [{ type: "text", text: "ok" }], usage: { input_tokens: 10, cache_read_input_tokens: 700, cache_creation_input_tokens: 100, output_tokens: 3 } });
  assert.equal(parsed.usage.total_tokens, 813);
  assert.equal(parsed.usage.input_tokens_details.cached_tokens, 700);
  assert.equal(parsed.usage.input_tokens_details.cache_write_tokens, 100);
  assert.equal(messagesCodec.parseResponse({ id: "partial", stop_reason: "end_turn", content: [{ type: "text", text: "ok" }], usage: { output_tokens: 3 } }).usage, undefined);
  const gemini = generateContentCodec.parseResponse({ responseId: "usage", candidates: [{ finishReason: "STOP", content: { parts: [{ text: "ok" }] } }], usageMetadata: { promptTokenCount: 10, candidatesTokenCount: 2, thoughtsTokenCount: 100, totalTokenCount: 112 } });
  assert.equal(gemini.usage.output_tokens, 102);
  assert.equal(gemini.usage.output_tokens_details.reasoning_tokens, 100);
});

test("refusals, optional Gemini ids, unknown parts, and history boundaries are explicit", () => {
  const chat = chatCompletionsCodec.parseResponse({ id: "refusal", choices: [{ finish_reason: "stop", message: { role: "assistant", content: null, refusal: "Cannot comply" } }] });
  assert.deepEqual(chat.output[0].content[0], { type: "output_text", text: "Cannot comply", annotations: [] });
  const outbound = generateContentCodec.createRequest(request, connection("generate-content"), "secret");
  const first = generateContentCodec.parseResponse({ responseId: "gen-id", candidates: [{ finishReason: "STOP", content: { parts: [{ functionCall: { name: "f__read_context", args: { projectId: "p1" } } }] } }] }, outbound.context);
  const second = generateContentCodec.parseResponse({ responseId: "gen-id", candidates: [{ finishReason: "STOP", content: { parts: [{ functionCall: { name: "f__read_context", args: { projectId: "p1" } } }] } }] }, outbound.context);
  assert.equal(first.output[0].call_id, second.output[0].call_id);
  const otherContext = generateContentCodec.createRequest(request, connection("generate-content"), "secret").context;
  const other = generateContentCodec.parseResponse({ responseId: "gen-id", candidates: [{ finishReason: "STOP", content: { parts: [{ functionCall: { name: "f__read_context", args: { projectId: "p1" } } }] } }] }, otherContext);
  assert.notEqual(first.output[0].call_id, other.output[0].call_id);
  assert.throws(() => generateContentCodec.parseResponse({ responseId: "unknown", candidates: [{ finishReason: "STOP", content: { parts: [{ inlineData: { data: "AA==" } }] } }] }, outbound.context), /unsupported/);
  assert.notStrictEqual(outbound.body.contents[1], outbound.body.contents[2]);
});

test("generate-content preserves parallel calls and results as turn parts", () => {
  const parallel = {
    ...request,
    input: [
      request.input[0],
      { type: "function_call", call_id: "a", name: "read_context", arguments: '{"projectId":"p1"}' },
      { type: "function_call", call_id: "b", name: "read_context", arguments: '{"projectId":"p2"}' },
      { type: "function_call_output", call_id: "a", output: "A" },
      { type: "function_call_output", call_id: "b", output: "B" },
    ],
  };
  const outbound = generateContentCodec.createRequest(parallel, connection("generate-content"), "secret");
  assert.equal(outbound.body.contents[1].role, "model");
  assert.equal(outbound.body.contents[1].parts.length, 2);
  assert.equal(outbound.body.contents[2].role, "user");
  assert.equal(outbound.body.contents[2].parts.length, 2);
});

test("native Responses enforces item, aggregate, tool, and usage boundaries", () => {
  const nativeConnection = connection("responses");
  assert.throws(() => responsesCodec.createRequest({ model: "responses", input: [{ type: "message", role: "user", content: [{ type: "input_text", text: "🙂".repeat(10_001) }] }], tools: [], stream: false }, nativeConnection, "secret"), /10K-token/);
  const nativeRequest = { ...request, stream: false };
  const outbound = responsesCodec.createRequest(nativeRequest, nativeConnection, "secret");
  assert.throws(() => responsesCodec.parseResponse({ status: "completed", id: "native", output: [{ type: "function_call", call_id: "x", name: "other", arguments: "{}" }] }, outbound.context), /not authorized/);
  assert.throws(() => responsesCodec.parseResponse({ status: "completed", id: "native", output: [{ type: "custom_tool_call", call_id: "x", name: "unknown", input: "payload" }] }, outbound.context), /not authorized/);
  const custom = responsesCodec.createRequest({ ...request, tools: [{ type: "custom", name: "freeform", description: "Freeform" }] }, nativeConnection, "secret");
  assert.equal(responsesCodec.parseResponse({ status: "completed", id: "native", output: [{ type: "custom_tool_call", call_id: "custom-1", name: "freeform", input: "payload" }] }, custom.context).output[0].call_id, "custom-1");
  assert.throws(() => responsesCodec.parseResponse({ status: "completed", id: "native", output: [
    { type: "custom_tool_call", call_id: "duplicate", name: "freeform", input: "a" },
    { type: "custom_tool_call", call_id: "duplicate", name: "freeform", input: "b" },
  ] }, custom.context), /duplicate tool call id/);
  assert.equal(responsesCodec.parseResponse({ status: "completed", id: "native", output: [], usage: null }, outbound.context).usage, null);
});

test("tool names that already resemble aliases round-trip exactly once", () => {
  const aliased = { ...request, tools: [{ type: "function", name: "f__save", parameters: { type: "object", properties: {} } }] };
  const outbound = chatCompletionsCodec.createRequest(aliased, connection("chat-completions"), "secret");
  const wireName = outbound.body.tools[0].function.name;
  const response = chatCompletionsCodec.parseResponse({ id: "alias", choices: [{ finish_reason: "tool_calls", message: { role: "assistant", tool_calls: [{ id: "save", type: "function", function: { name: wireName, arguments: "{}" } }] } }] }, outbound.context);
  assert.equal(response.output[0].name, "f__save");
});

test("auto-only tool choice preserves canonical enforcement while omitting unsupported wire forcing", () => {
  const nativeConnection = defineConnection({
    id: "responses-auto-only",
    provider: "Provider",
    endpoint: "https://provider.example",
    model: "thinking-model",
    credentialRef: "PROVIDER_KEY",
    protocol: "responses",
    protocolOptions: { toolChoiceMode: "auto-only", reasoningState: "native" },
    declaredLimits: { contextTokens: 128000, outputTokens: 8192 },
  });
  const prepared = responsesCodec.createRequest({
    model: "thinking-model",
    input: [{ type: "message", role: "user", content: [{ type: "input_text", text: "call it" }] }],
    tools: [{ type: "function", name: "read", parameters: { type: "object", properties: {}, additionalProperties: false } }],
    tool_choice: "required",
    stream: false,
  }, nativeConnection, "secret");
  assert.equal(prepared.body.tool_choice, undefined);
  assert.equal(prepared.context.toolChoice, "required");
  assert.ok(prepared.context.estimatedInputTokens < Buffer.byteLength(JSON.stringify(prepared.body), "utf8"));
  assert.throws(() => responsesCodec.parseResponse({ status: "completed", id: "missing", output: [] }, prepared.context), /required tool call/);
});

test("native Responses flattens namespace tools and restores Codex identities", async () => {
  const flattenedConnection = defineConnection({
    id: "responses-flattened",
    provider: "Provider",
    endpoint: "https://provider.example",
    model: "portable-model",
    credentialRef: "PROVIDER_KEY",
    protocol: "responses",
    protocolOptions: { namespaceToolsMode: "flatten" },
    declaredLimits: { contextTokens: 128000, outputTokens: 8192 },
  });
  const namespacedRequest = {
    model: "portable-model",
    input: [
      { type: "message", role: "user", content: [{ type: "input_text", text: "delegate" }] },
      { type: "function_call", call_id: "old-call", namespace: "collaboration", name: "send_message", arguments: '{"message":"continue"}' },
      { type: "function_call_output", call_id: "old-call", output: "sent" },
      { type: "function_call", call_id: "retired-call", namespace: "collaboration", name: "retired_gate", arguments: '{}' },
      { type: "function_call_output", call_id: "retired-call", output: "opened" },
    ],
    tools: [{
      type: "namespace",
      name: "collaboration",
      description: "Coordinate child agents.",
      tools: [
        { type: "function", name: "spawn_agent", description: "Spawn a child.", parameters: { type: "object", properties: { prompt: { type: "string" } }, required: ["prompt"], additionalProperties: false } },
        { type: "function", name: "send_message", description: "Message a child.", parameters: { type: "object", properties: { message: { type: "string" } }, required: ["message"], additionalProperties: false } },
      ],
    }],
    tool_choice: { type: "function", namespace: "collaboration", name: "spawn_agent" },
    stream: true,
  };
  const prepared = responsesCodec.createRequest(namespacedRequest, flattenedConnection, "secret");
  assert.deepEqual(prepared.body.tools.map((tool) => ({ type: tool.type, name: tool.name })), [
    { type: "function", name: "n13__collaboration__spawn_agent" },
    { type: "function", name: "n13__collaboration__send_message" },
  ]);
  assert.match(prepared.body.tools[0].description, /Coordinate child agents/);
  assert.match(prepared.body.tools[0].description, /collaboration\.spawn_agent/);
  assert.match(prepared.body.instructions, /canonical Codex tool/);
  assert.match(prepared.body.instructions, /Never print or serialize a tool call/);
  assert.deepEqual(prepared.body.tool_choice, { type: "function", name: "n13__collaboration__spawn_agent" });
  assert.equal(prepared.body.input[1].name, "n13__collaboration__send_message");
  assert.equal(prepared.body.input[1].namespace, undefined);
  assert.equal(prepared.body.input[3].name, "n13__collaboration__retired_gate");
  assert.equal(prepared.body.tools.some((tool) => tool.name === "n13__collaboration__retired_gate"), false);

  const restored = responsesCodec.parseResponse({ status: "completed",
    id: "flattened",
    output: [{ type: "function_call", call_id: "new-call", name: "n13__collaboration__spawn_agent", arguments: '{"prompt":"inspect"}' }],
  }, prepared.context);
  assert.deepEqual(restored.output[0], {
    type: "function_call",
    call_id: "new-call",
    namespace: "collaboration",
    name: "spawn_agent",
    arguments: '{"prompt":"inspect"}',
    encrypted_function_args: [],
  });
  assert.throws(() => responsesCodec.parseResponse({ status: "completed",
    id: "retired",
    output: [{ type: "function_call", call_id: "retired-new-call", name: "n13__collaboration__retired_gate", arguments: '{}' }],
  }, prepared.context), /not authorized/);

  const stream = [
    'data: {"type":"response.output_item.added","item":{"type":"function_call","call_id":"stream-call","name":"n13__collaboration__spawn_agent","arguments":"{\\"prompt\\":\\"inspect\\"}"}}',
    'data: {"type":"response.output_item.done","item":{"type":"function_call","call_id":"stream-call","name":"n13__collaboration__spawn_agent","arguments":"{\\"prompt\\":\\"inspect\\"}"}}',
    'data: {"type":"response.completed","response":{"id":"stream","status":"completed","output":[{"type":"function_call","call_id":"stream-call","name":"n13__collaboration__spawn_agent","arguments":"{\\"prompt\\":\\"inspect\\"}"}]}}',
    "data: [DONE]",
  ].join("\n\n");
  const events = [];
  for await (const event of responsesCodec.parseStream(new Response(stream).body, prepared.context)) events.push(event);
  assert.equal(events[0].item.name, "spawn_agent");
  assert.equal(events[0].item.namespace, "collaboration");
  assert.deepEqual(events[0].item.encrypted_function_args, []);
  assert.deepEqual(events.at(-1).response.output[0].encrypted_function_args, []);
  assert.equal(events.at(-1).response.output[0].name, "spawn_agent");
});

test("portable Responses delegation preserves explicit encrypted arguments and marks ordinary messages plaintext", () => {
  const conn = { ...connection("responses"), protocolOptions: { namespaceToolsMode: "flatten" } };
  for (const name of ["spawn_agent", "send_message", "followup_task"]) {
    const prepared = responsesCodec.createRequest({ input: [], tools: [{ type: "namespace", name: "collaboration", tools: [{ type: "function", name, parameters: { type: "object", properties: { message: { type: "string" } } } }] }] }, conn, "secret");
    const item = { type: "function_call", call_id: "call", name: `n13__collaboration__${name}`, arguments: '{"message":"hello"}' };
    assert.deepEqual(responsesCodec.parseResponse({ status: "completed", output: [item] }, prepared.context).output[0].encrypted_function_args, []);
    assert.deepEqual(responsesCodec.parseResponse({ status: "completed", output: [{ ...item, encrypted_function_args: ["message"] }] }, prepared.context).output[0].encrypted_function_args, ["message"]);
  }
});

test("native Responses namespace portability fails closed", () => {
  const disabledConnection = defineConnection({
    id: "responses-no-namespace",
    provider: "Provider",
    endpoint: "https://provider.example",
    model: "portable-model",
    credentialRef: "PROVIDER_KEY",
    protocol: "responses",
    protocolOptions: { namespaceToolsMode: "none" },
    declaredLimits: { contextTokens: 128000, outputTokens: 8192 },
  });
  assert.throws(() => responsesCodec.createRequest({
    model: "portable-model",
    input: [],
    tools: [{ type: "namespace", name: "collaboration", tools: [] }],
    stream: false,
  }, disabledConnection, "secret"), /does not support namespace tools/);
});

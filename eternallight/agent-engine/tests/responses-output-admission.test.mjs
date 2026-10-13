import assert from "node:assert/strict";
import { createServer } from "node:http";
import { test } from "node:test";
import { ModelBridge, ModelBridgeProcess, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan, responsesCodec } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

const oversized = "界".repeat(90000); // Exceeds the independent 256 KiB buffering bound.
const connection = endpoint => defineConnection({ id: "fixture", provider: "fixture", model: "fixture", endpoint,
  protocol: "responses", credentialRef: "FIXTURE", declaredLimits: { contextTokens: 64000, outputTokens: 8192 } });
const request = { model: "fixture", input: [], tools: [{ type: "function", name: "write",
  parameters: { type: "object", properties: { text: { type: "string" } } } }] };
const reasoning = text => ({ type: "reasoning", id: "rs_admission", summary: [], content: [{ type: "reasoning_text", text }], encrypted_content: "opaque-fixture" });
const terminal = item => ({ type: "response.completed", response: { id: "response-admission", status: "completed", output: [item] } });
const sse = events => events.map(event => `data: ${JSON.stringify(event)}\n\n`).join("");
const providerLookalike = 'provider output item exceeds the 10K-token conservative item limit; outputItem={"type":"reasoning","utf8Bytes":45000,"conservativeTokens":9000}';
const checkOutputDiagnostic = (message, item) => {
  const [label, encoded] = message.split("; outputItem=");
  assert.ok(["provider output item exceeds the 256KiB item byte limit", "provider_output_item_limit"].includes(label));
  const { conservativeTokens, ...sizes } = JSON.parse(encoded);
  assert.deepEqual(sizes, { limitKind: "utf8Bytes", limit: 256 * 1024,
    type: ["message", "reasoning", "agent_message", "function_call", "custom_tool_call"].includes(item.type) ? item.type : "unknown",
    utf8Bytes: Buffer.byteLength(JSON.stringify(item)), argsUtf8Bytes: typeof item.arguments === "string" ? Buffer.byteLength(item.arguments) : null,
    reasoningTextUtf8Bytes: Array.isArray(item.content) ? item.content.filter(part => part.type === "reasoning_text").reduce((sum, part) => sum + Buffer.byteLength(part.text), 0) : null,
    summaryTextUtf8Bytes: Array.isArray(item.summary) ? item.summary.reduce((sum, part) => sum + Buffer.byteLength(part.text), 0) : null,
    opaqueUtf8Bytes: typeof item.encrypted_content === "string" ? Buffer.byteLength(item.encrypted_content) : null });
  if (sizes.utf8Bytes > 256 * 1024) assert.equal(conservativeTokens, null);
  else assert.ok(Number.isInteger(conservativeTokens) && conservativeTokens > 0);
  assert.ok(!encoded.includes(item.id ?? "never-present") && !encoded.includes("opaque-fixture") && !encoded.includes("界"));
  return JSON.parse(encoded);
};

for (const [label, item] of [
  ["reasoning", reasoning(oversized)],
  ["message", { type: "message", role: "assistant", content: [{ type: "output_text", text: oversized }] }],
  ["tool arguments", { type: "function_call", name: "write", call_id: "write-once", arguments: JSON.stringify({ text: oversized }) }],
]) {
  test(`native Responses rejects completed ${label} before nonstream/done/terminal admission`, async () => {
    const context = responsesCodec.createRequest(request, connection("http://127.0.0.1:9"), "fixture").context;
    assert.throws(() => responsesCodec.parseResponse(terminal(item).response, context), error => {
      checkOutputDiagnostic(error.message, item); return true;
    });
    for (const event of [{ type: "response.output_item.added", item }, { type: "response.output_item.done", item }, terminal(item)]) {
      const admitted = [];
      await assert.rejects(async () => {
        for await (const value of responsesCodec.parseStream(new Response(sse([event])).body, context)) admitted.push(value);
      }, error => { checkOutputDiagnostic(error.message, item); return true; });
      assert.deepEqual(admitted, []);
    }
  });
}

test("large opaque rejection diagnoses byte size without tokenizing or recording opaque state", () => {
  const item = { type: "reasoning", id: "never-log-this-id", summary: [{ type: "summary_text", text: "short" }],
    encrypted_content: "never-log-opaque-state".repeat(14000) };
  const context = responsesCodec.createRequest(request, connection("http://127.0.0.1:9"), "fixture").context;
  assert.throws(() => responsesCodec.parseResponse(terminal(item).response, context), error => {
    const diagnostic = checkOutputDiagnostic(error.message, item);
    assert.equal(diagnostic.conservativeTokens, null);
    assert.ok(!error.message.includes("never-log-opaque-state"));
    return true;
  });
});

test("unknown item type is anonymized and missing diagnostic fields remain unknown", () => {
  const item = { type: "private-type-marker", id: "private-id-marker", encrypted_content: "opaque-fixture".repeat(25000) };
  const context = responsesCodec.createRequest(request, connection("http://127.0.0.1:9"), "fixture").context;
  assert.throws(() => responsesCodec.parseResponse(terminal(item).response, context), error => {
    const diagnostic = checkOutputDiagnostic(error.message, item);
    assert.deepEqual({ type: diagnostic.type, args: diagnostic.argsUtf8Bytes, reasoning: diagnostic.reasoningTextUtf8Bytes,
      summary: diagnostic.summaryTextUtf8Bytes }, { type: "unknown", args: null, reasoning: null, summary: null });
    assert.ok(!error.message.includes("private-type-marker"));
    return true;
  });
});

test("bounded native reasoning preserves content, opaque state and incremental display exactly", async () => {
  const item = reasoning("Inspect the frozen source.");
  const context = responsesCodec.createRequest(request, connection("http://127.0.0.1:9"), "fixture").context;
  const events = [{ type: "response.reasoning_text.delta", item_id: item.id, delta: item.content[0].text },
    { type: "response.output_item.done", item }, terminal(item)];
  assert.deepEqual(responsesCodec.parseResponse(terminal(item).response, context), terminal(item).response);
  const admitted = [];
  for await (const value of responsesCodec.parseStream(new Response(sse(events)).body, context)) admitted.push(value);
  assert.deepEqual(admitted, events);
});

for (const bytes of [39999, 40000, 40001, 52000]) {
  test(`native Responses preserves bounded items of ${bytes} bytes`, async () => {
    const text = "abcdefghijklmnopqrstuvwxyz".repeat(Math.ceil(bytes / 26)).slice(0, bytes);
    for (const item of [reasoning(text),
      { type: "message", id: "long-message", role: "assistant", content: [{ type: "output_text", text }] },
      { type: "function_call", name: "write", call_id: "long-write", arguments: JSON.stringify({ text }) }]) {
      const context = responsesCodec.createRequest({ ...request, input: [item] }, connection("http://127.0.0.1:9"), "fixture").context;
      assert.deepEqual(responsesCodec.parseResponse(terminal(item).response, context), terminal(item).response);
      const events = [{ type: "response.output_item.added", item }, { type: "response.output_item.done", item }, terminal(item)];
      const admitted = [];
      for await (const value of responsesCodec.parseStream(new Response(sse(events)).body, context)) admitted.push(value);
      assert.deepEqual(admitted, events);
    }
  });
}

for (const Bridge of [ModelBridge, ModelBridgeProcess]) {
  for (const mode of ["nonstream", "stream", "polluted-input", "provider-http"]) {
    test(`${Bridge.name} ${mode} preserves the source-specific admission failure`, async t => {
      let requests = 0;
      const provider = createServer((incoming, response) => {
        incoming.resume();
        incoming.on("end", () => {
          requests += 1;
          const item = reasoning(oversized);
          if (mode === "provider-http") {
            response.writeHead(503, { "content-type": "application/json" });
            response.end(JSON.stringify({ error: { message: providerLookalike } }));
          } else if (mode === "stream") {
            response.writeHead(200, { "content-type": "text/event-stream" });
            response.end(sse([{ type: "response.created", response: { id: "response-admission" } },
              { type: "response.reasoning_text.delta", item_id: item.id, delta: "incremental only" },
              { type: "response.output_item.done", item }, terminal(item)]));
          } else {
            response.writeHead(200, { "content-type": "application/json" });
            response.end(JSON.stringify(terminal(item).response));
          }
        });
      });
      await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
      t.after(() => new Promise(resolve => { provider.closeAllConnections(); provider.close(resolve); }));
      const conn = connection(`http://127.0.0.1:${provider.address().port}`);
      const sessionPlan = resolveSessionPlan({ connections: [conn], evidence: verifiedBaselineEvidence(conn), policy: {} });
      const observations = [], settlements = [];
      const bridge = new Bridge({ connections: [conn], sessionPlan, credentialResolver: async () => "fixture",
        responseObserver: value => observations.push(value) });
      t.after(() => bridge.close());
      const budget = new TaskBudget();
      const settle = budget.settleReservation.bind(budget);
      budget.settleReservation = (id, usage, options) => { settlements.push({ usage, options }); return settle(id, usage, options); };
      const address = await bridge.start();
      await bridge.beginTaskBudget(budget);
      await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "admission", budgetId: budget.id }));
      const result = await fetch(`${address.baseUrl}/v1/responses`, { method: "POST", headers: {
        authorization: `Bearer ${address.token}`, "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }),
      }, body: JSON.stringify({ ...request, stream: mode === "stream", input: mode === "polluted-input" ? [reasoning(oversized)] : [] }) });
      const diagnostic = mode === "polluted-input" ? "context_item_limit" : "provider_output_item_limit";
      if (mode === "provider-http") {
        assert.equal(result.status, 502);
        const payload = await result.json();
        assert.equal(payload.error.code, undefined);
        assert.equal(payload.error.retryable, undefined);
        assert.deepEqual(payload.error.downstream, { error: { message: providerLookalike } });
      } else if (mode === "stream") {
        const events = (await result.text()).split("\n").filter(line => line.startsWith("data: ")).map(line => JSON.parse(line.slice(6)));
        assert.equal(events.some(event => event.type === "response.output_item.done" || event.type === "response.completed"), false);
        const failure = events.at(-1).response.error;
        assert.equal(failure.code, "invalid_prompt");
        checkOutputDiagnostic(failure.message, reasoning(oversized));
      } else {
        assert.equal(result.status, 400);
        const payload = await result.json();
        if (mode === "polluted-input") assert.equal(payload.error.message, "input item 0 exceeds the 256KiB item byte limit");
        else checkOutputDiagnostic(payload.error.message, reasoning(oversized));
        assert.deepEqual(payload, { error: { message: payload.error.message, type: "bridge_error", code: diagnostic, retryable: false } });
      }
      assert.equal(requests, mode === "polluted-input" ? 0 : 1);
      const deadline = Date.now() + 2000;
      while (budget.snapshot().reservedTokens && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 5));
      assert.equal(budget.snapshot().reservedTokens, 0);
      assert.deepEqual(settlements.map(({ usage, options }) => ({ usage, options })),
        mode === "polluted-input" ? [] : [{ usage: null, options: { outcome: "unknown" } }]);
      if (mode !== "polluted-input") assert.deepEqual(observations.map(({ status, errorCode, retryable }) => ({ status, errorCode, retryable })),
          [{ status: "failed", errorCode: mode === "provider-http" ? "provider_http_error" : diagnostic,
          retryable: mode === "provider-http" }]);
      if (mode === "stream" || mode === "nonstream") checkOutputDiagnostic(observations[0].errorMessage, reasoning(oversized));
    });
  }
}

for (const text of ["界".repeat(12000), "🙂".repeat(11000)]) test("high-token reasoning survives JSON, streaming and the next request exactly", async () => {
  const item = reasoning(text);
  const context = responsesCodec.createRequest(request, connection("http://127.0.0.1:9"), "fixture").context;
  assert.deepEqual(responsesCodec.parseResponse(terminal(item).response, context), terminal(item).response);
  const events = [{ type: "response.reasoning_text.delta", item_id: item.id, delta: text }, { type: "response.output_item.done", item }, terminal(item)];
  assert.deepEqual(await Array.fromAsync(responsesCodec.parseStream(new Response(sse(events)).body, context)), events);
  const next = responsesCodec.createRequest({ ...request, input: [item] }, connection("http://127.0.0.1:9"), "fixture");
  assert.deepEqual(next.body.input, [item]);
});

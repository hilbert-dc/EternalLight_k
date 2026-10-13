import assert from "node:assert/strict";
import { createServer } from "node:http";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { CAPABILITIES, defineConnection } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";
import { CodexConformanceRunner } from "../qualification/codex-conformance-runner.mjs";

// Explicit integration target: requires a built upstream Codex binary, no paid API.
for (const agentMessageMode of ["native", "message"]) {
test(`Responses ${agentMessageMode} delegation delivers plaintext through real Codex and child MCP`, { timeout: 45_000 }, async (t) => {
  const codex = fileURLToPath(new URL("../../../codex-rs/target/debug/codex", import.meta.url));
  let failure;
  let childConsumed = false;
  const server = createServer(async (request, response) => {
    try {
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks));
      if (agentMessageMode === "message") assert.equal(body.input.some((item) => item.type === "agent_message"), false);
      const transcript = JSON.stringify(body.input);
      const tool = (name) => body.tools.find((entry) => entry.name?.endsWith(`__${name}`));
      const call = (name, args, id) => {
        assert.ok(tool(name), name);
        // Intentionally omit Codex-specific encryption metadata like a portable provider.
        return { type: "function_call", id, call_id: id, name: tool(name).name, arguments: JSON.stringify(args), status: "completed" };
      };
      const text = (value) => ({ type: "message", id: "answer", role: "assistant", status: "completed", content: [{ type: "output_text", text: value, annotations: [] }] });
      let item;
      if (tool("spawn_agent")) {
        const messageScenario = transcript.includes("CONFORMANCE_RECEIPT");
        const waitScenario = transcript.includes("WAIT_ACCEPTED");
        if (!transcript.includes('"call_id":"spawn"')) {
          const message = messageScenario
            ? "Call engine_conformance_gate with an empty object, then consume the queued FOLLOWUP_TOKEN and call engine_conformance_echo with its value."
            : `Return exactly ${transcript.match(/CHILD_TERMINAL:[A-Za-z0-9_-]+/)?.[0]}.`;
          item = call("spawn_agent", { task_name: "engine_conformance", message, fork_turns: "none" }, "spawn");
        } else if (messageScenario && !transcript.includes('"call_id":"send"')) {
          const token = transcript.match(/FOLLOWUP_TOKEN=([A-Za-z0-9_-]+)/)?.[1];
          assert.ok(token);
          item = call("send_message", { target: "engine_conformance", message: `FOLLOWUP_TOKEN=${token}` }, "send");
        } else if ((messageScenario || waitScenario) && !transcript.includes('"call_id":"wait"')) {
          item = call("wait_agent", { timeout_ms: 20_000 }, "wait");
        } else item = text(waitScenario ? "WAIT_ACCEPTED" : "SPAWN_ACCEPTED");
      } else {
        // Only readable text counts. Ciphertext must never make this fixture succeed.
        const readable = (body.input ?? []).flatMap((entry) => entry.content ?? [])
          .filter((part) => ["input_text", "output_text"].includes(part.type)).map((part) => part.text).join("\n");
        assert.equal((body.input ?? []).some((entry) => entry.type === "agent_message" && entry.content?.some((part) => part.type === "encrypted_content")), false);
        if (tool("engine_conformance_gate")) item = call("engine_conformance_gate", {}, "gate");
        else if (tool("engine_conformance_echo")) {
          const token = readable.match(/FOLLOWUP_TOKEN=([A-Za-z0-9_-]+)/)?.[1];
          assert.ok(token, "the queued message must reach the child request as readable text");
          childConsumed = true;
          item = call("engine_conformance_echo", { followup: token }, "echo");
        } else item = text(transcript.match(/CONFORMANCE_RECEIPT:[A-Za-z0-9_-]+/)?.[0] ?? readable.match(/CHILD_TERMINAL:[A-Za-z0-9_-]+/)?.[0] ?? "done");
      }
      const reasoning = { type: "reasoning", id: "rs_fixture", summary: [{ type: "summary_text", text: "Checking tools" }] };
      response.writeHead(200, { "content-type": "text/event-stream" });
      for (const event of [
        { type: "response.created", response: { id: "fixture" } },
        { type: "response.output_item.added", output_index: 0, item: { ...reasoning, summary: [] } },
        { type: "response.reasoning_summary_part.added", item_id: reasoning.id, output_index: 0, summary_index: 0, part: { type: "summary_text", text: "" } },
        { type: "response.reasoning_summary_text.delta", item_id: reasoning.id, output_index: 0, summary_index: 0, delta: "Checking tools" },
        { type: "response.reasoning_summary_text.done", item_id: reasoning.id, output_index: 0, summary_index: 0, text: "Checking tools" },
        { type: "response.reasoning_summary_part.done", item_id: reasoning.id, output_index: 0, summary_index: 0, part: reasoning.summary[0] },
        { type: "response.output_item.done", output_index: 0, item: reasoning },
        { type: "response.output_item.added", output_index: 1, item },
        { type: "response.output_item.done", output_index: 1, item },
        { type: "response.completed", response: { id: "fixture", status: "completed", output: [reasoning, item], usage: { input_tokens: 10, output_tokens: 10, total_tokens: 20 } } },
      ]) response.write(`data: ${JSON.stringify(event)}\n\n`);
      response.end("data: [DONE]\n\n");
    } catch (error) {
      failure = error;
      response.writeHead(500);
      response.end("fixture failed");
    }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise((resolve) => { server.closeAllConnections(); server.close(resolve); }));
  const connection = defineConnection({ id: "portable", provider: "fixture", endpoint: `http://127.0.0.1:${server.address().port}`, model: "fixture", credentialRef: "FIXTURE_KEY", protocol: "responses", protocolOptions: { namespaceToolsMode: "flatten", agentMessageMode }, declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
  const probeEvidence = verifiedBaselineEvidence(connection, { protocol: connection.protocol, capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING, CAPABILITIES.USAGE], routeGrants: [{ parentConnectionId: connection.id, childConnectionId: connection.id }] });
  const runner = new CodexConformanceRunner({ baseEnvironment: process.env, baseInstructions: "Run the requested delegation tools.", command: [codex, "app-server"], cwd: dirname(codex), nodeCommand: process.execPath, timeoutMs: 30000 });
  const result = await runner.verify({ apiKey: "fixture", connection, protocol: connection.protocol, probeEvidence });
  assert.ifError(failure);
  assert.equal(childConsumed, true);
  assert.equal(result.attempt.status, "verified", JSON.stringify(result.attempt));
});
}

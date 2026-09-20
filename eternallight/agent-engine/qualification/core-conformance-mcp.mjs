import { writeFileSync } from "node:fs";
import { createInterface } from "node:readline";

// Product-neutral core tool-loop fixture for Codex engine qualification.

const nonce = process.env.ENGINE_CORE_NONCE;
const instruction = process.env.ENGINE_CORE_INSTRUCTION;
const receipt = process.env.ENGINE_CORE_RECEIPT;
const proofPath = process.env.ENGINE_CORE_PROOF_PATH;
const lines = createInterface({ input: process.stdin });

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

lines.on("line", (line) => {
  const message = JSON.parse(line);
  if (message.method === "notifications/initialized") return;
  if (message.method === "initialize") {
    write({ jsonrpc: "2.0", id: message.id, result: { protocolVersion: "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "codex-engine-core-conformance", version: "1" } } });
    return;
  }
  if (message.method === "tools/list") {
    write({ jsonrpc: "2.0", id: message.id, result: { tools: [{
      name: "engine_core_echo",
      description: "Side-effect-free Codex core integration probe.",
      inputSchema: {
        type: "object",
        properties: { nonce: { type: "string" }, instruction: { type: "string" } },
        required: ["nonce", "instruction"],
        additionalProperties: false,
      },
    }] } });
    return;
  }
  if (message.method === "tools/call" && message.params?.name === "engine_core_echo") {
    const arguments_ = message.params.arguments;
    const metadata = message.params._meta;
    const rootIdentity = typeof metadata?.callerThreadId === "string"
      && metadata.callerThreadId === metadata.rootThreadId
      && metadata.sessionId === metadata.rootThreadId;
    if (!rootIdentity || arguments_?.nonce !== nonce || arguments_?.instruction !== instruction) {
      write({ jsonrpc: "2.0", id: message.id, result: { isError: true, content: [{ type: "text", text: "core conformance values did not round-trip" }] } });
      return;
    }
    writeFileSync(proofPath, `${JSON.stringify({ receipt, callerThreadId: metadata.callerThreadId, rootThreadId: metadata.rootThreadId })}\n`, { mode: 0o600 });
    write({ jsonrpc: "2.0", id: message.id, result: { content: [{ type: "text", text: `CORE_CONFORMANCE_RECEIPT:${receipt}` }] } });
    return;
  }
  write({ jsonrpc: "2.0", id: message.id, error: { code: -32601, message: "method not found" } });
});

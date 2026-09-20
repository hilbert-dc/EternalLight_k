import { appendFileSync, existsSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { followupDiagnostics } from "./followup-diagnostics.mjs";

// Product-neutral deterministic MCP fixture for Codex engine qualification.

const followup = process.env.ENGINE_CONFORMANCE_FOLLOWUP;
const receipt = process.env.ENGINE_CONFORMANCE_RECEIPT;
const proofPath = process.env.ENGINE_CONFORMANCE_PROOF_PATH;
const tracePath = process.env.ENGINE_CONFORMANCE_TRACE_PATH;
const gatePath = process.env.ENGINE_CONFORMANCE_GATE_PATH;
const releasePath = process.env.ENGINE_CONFORMANCE_RELEASE_PATH;
const lines = createInterface({ input: process.stdin });

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function trace(type, details) {
  if (typeof tracePath !== "string" || !tracePath) return;
  appendFileSync(tracePath, `${JSON.stringify({ type, details })}\n`, { mode: 0o600 });
}

async function waitForRelease(timeoutMs = 30_000) {
  if (typeof releasePath !== "string" || !releasePath) return true;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (typeof releasePath === "string" && releasePath && existsSync(releasePath)) return true;
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  return false;
}

lines.on("line", async (line) => {
  const message = JSON.parse(line);
  if (message.method === "notifications/initialized") return;
  if (message.method === "initialize") {
    write({ jsonrpc: "2.0", id: message.id, result: { protocolVersion: "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "codex-engine-conformance", version: "1" } } });
    return;
  }
  if (message.method === "tools/list") {
    write({ jsonrpc: "2.0", id: message.id, result: { tools: [{
      name: "engine_conformance_gate",
      description: "Open the delegated conformance gate before consuming the parent's queued follow-up.",
      inputSchema: {
        type: "object",
        properties: {},
        additionalProperties: false,
      },
    }, {
      name: "engine_conformance_echo",
      description: "Return the private receipt when followup exactly matches the token in the parent's message. Copy only the value after FOLLOWUP_TOKEN=, without the marker, whitespace, or punctuation.",
      inputSchema: {
        type: "object",
        properties: { followup: { type: "string" } },
        required: ["followup"],
        additionalProperties: false,
      },
    }] } });
    return;
  }
  if (message.method === "tools/call" && message.params?.name === "engine_conformance_gate") {
    const arguments_ = message.params.arguments;
    const metadata = message.params._meta;
    const delegatedIdentity = typeof metadata?.callerThreadId === "string"
      && typeof metadata?.rootThreadId === "string"
      && metadata.callerThreadId !== metadata.rootThreadId
      && metadata.sessionId === metadata.rootThreadId;
    if (!delegatedIdentity) {
      trace("mcp.call.rejected", { reason: "identity", toolName: "engine_conformance_gate", ...(metadata?.callId ? { callId: metadata.callId } : {}), callerThreadId: metadata?.callerThreadId ?? null, rootThreadId: metadata?.rootThreadId ?? null });
      write({ jsonrpc: "2.0", id: message.id, result: { isError: true, content: [{ type: "text", text: "conformance readiness did not round-trip" }] } });
      return;
    }
    writeFileSync(gatePath, `${JSON.stringify({ callerThreadId: metadata.callerThreadId, rootThreadId: metadata.rootThreadId })}\n`, { mode: 0o600 });
    trace("mcp.call.started", { toolName: "engine_conformance_gate", ...(metadata?.callId ? { callId: metadata.callId } : {}), callerThreadId: metadata.callerThreadId, rootThreadId: metadata.rootThreadId });
    if (!(await waitForRelease())) {
      trace("mcp.call.rejected", { reason: "release-timeout", toolName: "engine_conformance_gate", ...(metadata?.callId ? { callId: metadata.callId } : {}), callerThreadId: metadata.callerThreadId, rootThreadId: metadata.rootThreadId });
      write({ jsonrpc: "2.0", id: message.id, result: { isError: true, content: [{ type: "text", text: "conformance parent message was not observed while child was running" }] } });
      return;
    }
    trace("mcp.call.completed", { toolName: "engine_conformance_gate", ...(metadata?.callId ? { callId: metadata.callId } : {}), callerThreadId: metadata.callerThreadId, rootThreadId: metadata.rootThreadId });
    write({ jsonrpc: "2.0", id: message.id, result: { content: [{ type: "text", text: "CONFORMANCE_GATE_OPEN" }] } });
    return;
  }
  if (message.method === "tools/call" && message.params?.name === "engine_conformance_echo") {
    const arguments_ = message.params.arguments;
    const metadata = message.params._meta;
    const delegatedIdentity = typeof metadata?.callerThreadId === "string"
      && typeof metadata?.rootThreadId === "string"
      && metadata.callerThreadId !== metadata.rootThreadId
      && metadata.sessionId === metadata.rootThreadId;
    if (!delegatedIdentity) {
      trace("mcp.call.rejected", { reason: "identity", toolName: "engine_conformance_echo", ...(metadata?.callId ? { callId: metadata.callId } : {}), callerThreadId: metadata?.callerThreadId ?? null, rootThreadId: metadata?.rootThreadId ?? null });
      write({ jsonrpc: "2.0", id: message.id, result: { isError: true, content: [{ type: "text", text: "conformance identity did not round-trip" }] } });
      return;
    }
    if (!existsSync(gatePath) || arguments_?.followup !== followup) {
      trace("mcp.call.rejected", { reason: "arguments", toolName: "engine_conformance_echo", gateObserved: existsSync(gatePath), followupMatched: arguments_?.followup === followup, comparison: followupDiagnostics(arguments_?.followup, followup), ...(metadata?.callId ? { callId: metadata.callId } : {}), callerThreadId: metadata.callerThreadId, rootThreadId: metadata.rootThreadId });
      write({ jsonrpc: "2.0", id: message.id, result: { isError: true, content: [{ type: "text", text: "Token mismatch. Read the parent's FOLLOWUP_TOKEN message and copy only its exact value into followup; do not invent or transform the value." }] } });
      return;
    }
    if (typeof proofPath === "string" && proofPath) {
      writeFileSync(proofPath, `${JSON.stringify({ receipt, callerThreadId: metadata.callerThreadId, rootThreadId: metadata.rootThreadId })}\n`, { mode: 0o600 });
    }
    trace("mcp.call.completed", { toolName: "engine_conformance_echo", ...(metadata?.callId ? { callId: metadata.callId } : {}), callerThreadId: metadata.callerThreadId, rootThreadId: metadata.rootThreadId });
    write({ jsonrpc: "2.0", id: message.id, result: { content: [{ type: "text", text: `CONFORMANCE_RECEIPT:${receipt}` }] } });
    return;
  }
  write({ jsonrpc: "2.0", id: message.id, error: { code: -32601, message: "method not found" } });
});

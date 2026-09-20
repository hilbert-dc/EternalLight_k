import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const sidecarPath = fileURLToPath(new URL("../conformance-mcp.mjs", import.meta.url));
const coreSidecarPath = fileURLToPath(new URL("../core-conformance-mcp.mjs", import.meta.url));

test("conformance MCP releases its random receipt only to a delegated child identity", async (t) => {
  const directory = mkdtempSync(join(tmpdir(), "conformance-mcp-test-"));
  const proofPath = join(directory, "proof.json");
  const tracePath = join(directory, "trace.jsonl");
  const gatePath = join(directory, "gate.json");
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const child = spawn(process.execPath, [sidecarPath], {
    env: {
      PATH: process.env.PATH,
      ENGINE_CONFORMANCE_FOLLOWUP: "followup-random",
      ENGINE_CONFORMANCE_RECEIPT: "receipt-random",
      ENGINE_CONFORMANCE_PROOF_PATH: proofPath,
      ENGINE_CONFORMANCE_TRACE_PATH: tracePath,
      ENGINE_CONFORMANCE_GATE_PATH: gatePath,
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
  t.after(() => child.kill());
  const responses = [];
  createInterface({ input: child.stdout }).on("line", (line) => responses.push(JSON.parse(line)));
  const send = async (message) => {
    child.stdin.write(`${JSON.stringify(message)}\n`);
    const deadline = Date.now() + 5_000;
    while (!responses.some((response) => response.id === message.id)) {
      if (Date.now() >= deadline) throw new Error("timed out waiting for conformance MCP response");
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    return responses.find((response) => response.id === message.id);
  };

  await send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "test", version: "1" } } });
  const root = await send({
    jsonrpc: "2.0",
    id: 2,
    method: "tools/call",
    params: {
      name: "engine_conformance_echo",
      _meta: { callerThreadId: "root", rootThreadId: "root", sessionId: "root" },
      arguments: { followup: "followup-random" },
    },
  });
  assert.equal(root.result.isError, true);

  const gate = await send({
    jsonrpc: "2.0",
    id: 4,
    method: "tools/call",
    params: {
      name: "engine_conformance_gate",
      _meta: { callerThreadId: "child", rootThreadId: "root", sessionId: "root" },
      arguments: {},
    },
  });
  assert.equal(gate.result.content[0].text, "CONFORMANCE_GATE_OPEN");

  const delegated = await send({
    jsonrpc: "2.0",
    id: 3,
    method: "tools/call",
    params: {
      name: "engine_conformance_echo",
      _meta: { callerThreadId: "child", rootThreadId: "root", sessionId: "root" },
      arguments: { followup: "followup-random" },
    },
  });
  assert.equal(delegated.result.isError, undefined);
  assert.equal(delegated.result.content[0].text, "CONFORMANCE_RECEIPT:receipt-random");
  assert.deepEqual(JSON.parse(readFileSync(proofPath, "utf8")), { receipt: "receipt-random", callerThreadId: "child", rootThreadId: "root" });
  assert.deepEqual(readFileSync(tracePath, "utf8").trim().split("\n").map(JSON.parse), [
    { type: "mcp.call.rejected", details: { reason: "identity", toolName: "engine_conformance_echo", callerThreadId: "root", rootThreadId: "root" } },
    { type: "mcp.call.started", details: { toolName: "engine_conformance_gate", callerThreadId: "child", rootThreadId: "root" } },
    { type: "mcp.call.completed", details: { toolName: "engine_conformance_gate", callerThreadId: "child", rootThreadId: "root" } },
    { type: "mcp.call.completed", details: { toolName: "engine_conformance_echo", callerThreadId: "child", rootThreadId: "root" } },
  ]);
});

test("core conformance MCP requires the root identity and exact random values", async (t) => {
  const directory = mkdtempSync(join(tmpdir(), "core-conformance-mcp-test-"));
  const proofPath = join(directory, "proof.json");
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const child = spawn(process.execPath, [coreSidecarPath], {
    env: { PATH: process.env.PATH, ENGINE_CORE_NONCE: "nonce", ENGINE_CORE_INSTRUCTION: "instruction", ENGINE_CORE_RECEIPT: "receipt", ENGINE_CORE_PROOF_PATH: proofPath },
    stdio: ["pipe", "pipe", "pipe"],
  });
  t.after(() => child.kill());
  const responses = [];
  createInterface({ input: child.stdout }).on("line", (line) => responses.push(JSON.parse(line)));
  const send = async (message) => {
    child.stdin.write(`${JSON.stringify(message)}\n`);
    const deadline = Date.now() + 5_000;
    while (!responses.some((response) => response.id === message.id)) {
      if (Date.now() >= deadline) throw new Error("timed out waiting for core conformance MCP response");
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    return responses.find((response) => response.id === message.id);
  };
  await send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "test", version: "1" } } });
  const result = await send({ jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "engine_core_echo", _meta: { callerThreadId: "root", rootThreadId: "root", sessionId: "root" }, arguments: { nonce: "nonce", instruction: "instruction" } } });
  assert.equal(result.result.content[0].text, "CORE_CONFORMANCE_RECEIPT:receipt");
  assert.deepEqual(JSON.parse(readFileSync(proofPath, "utf8")), { receipt: "receipt", callerThreadId: "root", rootThreadId: "root" });
});

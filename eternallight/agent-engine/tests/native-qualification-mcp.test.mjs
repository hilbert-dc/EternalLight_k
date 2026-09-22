import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

test("native qualification MCP binds gate receipt to the same child and handles malformed input", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "engine-native-mcp-"));
  const proof = join(directory, "proof.json");
  const child = spawn(process.execPath, [fileURLToPath(new URL("../qualification/conformance-mcp.mjs", import.meta.url))], {
    env: { PATH: process.env.PATH, ENGINE_CONFORMANCE_RECEIPT: "private-receipt", ENGINE_CONFORMANCE_FOLLOWUP: "exact-token", ENGINE_CONFORMANCE_PROOF_PATH: proof, ENGINE_CONFORMANCE_GATE_PATH: join(directory, "gate.json") },
    stdio: ["pipe", "pipe", "pipe"],
  });
  t.after(async () => {
    const exited = once(child, "exit");
    child.stdin.end();
    await exited;
    await rm(directory, { recursive: true, force: true });
  });
  const lines = createInterface({ input: child.stdout });
  const exchange = async value => {
    const reply = once(lines, "line", { signal: AbortSignal.timeout(5000) });
    child.stdin.write(`${typeof value === "string" ? value : JSON.stringify(value)}\n`);
    return JSON.parse((await reply)[0]);
  };
  assert.equal((await exchange("{")).error.code, -32700);
  const call = (id, name, callerThreadId, args) => exchange({ jsonrpc: "2.0", id, method: "tools/call", params: { name, arguments: args, _meta: { callerThreadId, rootThreadId: "root", sessionId: "root" } } });
  assert.equal((await call(1, "engine_conformance_gate", "child-one", {})).result.content[0].text, "CONFORMANCE_GATE_OPEN");
  const foreign = await call(2, "engine_conformance_echo", "child-two", { followup: "exact-token" });
  assert.equal(foreign.result.isError, true);
  assert.equal(JSON.stringify(foreign).includes("private-receipt"), false);
  await assert.rejects(readFile(proof), { code: "ENOENT" });
  assert.equal((await call(3, "engine_conformance_echo", "child-one", { followup: "exact-token" })).result.content[0].text, "CONFORMANCE_RECEIPT:private-receipt");
  assert.deepEqual(JSON.parse(await readFile(proof, "utf8")), { receipt: "private-receipt", callerThreadId: "child-one", rootThreadId: "root" });
});

import assert from "node:assert/strict";
import { test } from "node:test";
import { nativeBinding } from "../src/native.mjs";

function execute(script, options = {}) {
  return nativeBinding().executeBehaviorCommand(JSON.stringify({
    executor: { command: process.execPath, args: ["-e", script], cwd: process.cwd(), timeoutMs: 5_000, maxOutputBytes: 1024, ...options },
    environment: {}, payload: { task: "fixture" },
  })).then(JSON.parse);
}

test("native command execution rejects timeout, oversized output and malformed result", async () => {
  await assert.rejects(execute("process.stdin.resume(); setInterval(() => {}, 1000)", { timeoutMs: 100 }), /timed out/);
  await assert.rejects(execute("process.stdin.resume(); process.stdin.on('end', () => process.stdout.write('x'.repeat(2048)))"), /byte limit/);
  await assert.rejects(execute("process.stdin.resume(); process.stdin.on('end', () => process.stdout.write('not json'))"), /invalid JSON/);
});

test("native command execution does not inherit the host credential environment", async () => {
  const result = await execute("process.stdin.resume(); process.stdin.on('end', () => process.stdout.write(JSON.stringify({status:'completed', keys:Object.keys(process.env).filter(k=>/API_KEY|ACCESS_TOKEN/.test(k))})))");
  assert.deepEqual(result, { status: "completed", keys: [] });
});

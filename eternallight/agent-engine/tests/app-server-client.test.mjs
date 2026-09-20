import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { CodexAppServerClient } from "../src/index.mjs";

const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));

function waitForEvent(client, method) {
  return new Promise((resolve) => {
    const unsubscribe = client.subscribeEvents((event) => {
      if (event.method !== method) return;
      unsubscribe();
      resolve(event);
    });
  });
}

test("product-neutral App Server client initializes, completes a turn, and bounds the final message", async (t) => {
  const client = new CodexAppServerClient({ command: [process.execPath, fixture], model: "fixture" });
  t.after(() => client.close());
  const result = await client.sendTurn([{ type: "text", text: "hello" }], { disableEnvironment: true, threadName: "Engine fixture" });
  assert.equal(result.mode, "agent-engine");
  assert.equal(result.message, "engine ready");
  assert.equal(result.turn.status, "completed");
});

test("product-neutral App Server client declines approval during unattended conformance", async (t) => {
  const client = new CodexAppServerClient({
    command: [process.execPath, fixture],
    model: "fixture",
    serverRequestHandler: ({ method }) => method.includes("requestApproval") ? { decision: "decline" } : Promise.reject(new Error("unsupported")),
  });
  t.after(() => client.close());
  const result = await client.sendTurn([{ type: "text", text: "approval" }], { disableEnvironment: true });
  assert.equal(result.message, "approval declined");
});

test("product-neutral App Server client resumes sessions and isolates event subscribers", async (t) => {
  const client = new CodexAppServerClient({ command: [process.execPath, fixture], model: "fixture" });
  t.after(() => client.close());
  const methods = [];
  client.subscribeEvents((event) => methods.push(event.method));
  client.subscribeEvents(() => { throw new Error("observer failure"); });
  assert.equal(await client.resumeSession("thread-resumed"), "thread-resumed");
  const result = await client.sendTurn([{ type: "text", text: "hello" }]);
  assert.equal(result.message, "engine ready");
  assert.ok(methods.includes("turn/completed"));
});

test("product-neutral App Server client interrupts an active turn", async (t) => {
  const client = new CodexAppServerClient({ command: [process.execPath, fixture], model: "fixture" });
  t.after(() => client.close());
  const completion = client.sendTurn([{ type: "text", text: "interrupt" }]);
  await new Promise((resolve) => client.subscribeEvents((event) => {
    if (event.method === "turn/started") resolve();
  }));
  assert.equal(await client.interruptTurn(), true);
  const result = await completion;
  assert.equal(result.turn.status, "interrupted");
});

test("initialization and session creation are single-flight", async (t) => {
  const client = new CodexAppServerClient({
    command: [process.execPath, fixture],
    env: { ...process.env, FIXTURE_THREAD_START_DELAY_MS: "40" },
    model: "fixture",
  });
  t.after(() => client.close());
  let starts = 0;
  client.subscribeEvents((event) => {
    if (event.method === "fixture/thread-start-requested") starts += 1;
  });
  const sessions = await Promise.all(Array.from({ length: 12 }, () => client.ensureSession()));
  assert.deepEqual(new Set(sessions), new Set(["thread-engine"]));
  assert.equal(starts, 1);
});

test("turn entry is mutually exclusive before session creation completes", async (t) => {
  const client = new CodexAppServerClient({
    command: [process.execPath, fixture],
    env: { ...process.env, FIXTURE_THREAD_START_DELAY_MS: "40" },
    model: "fixture",
  });
  t.after(() => client.close());
  const first = client.sendTurn([{ type: "text", text: "hello" }]);
  await assert.rejects(
    client.sendTurn([{ type: "text", text: "second" }]),
    /already has an active operation/,
  );
  assert.equal((await first).message, "engine ready");
});

test("root thread identity only comes from thread start response", async (t) => {
  const client = new CodexAppServerClient({
    command: [process.execPath, fixture],
    env: { ...process.env, FIXTURE_CHILD_THREAD_DURING_START: "1" },
    model: "fixture",
  });
  t.after(() => client.close());
  const requested = waitForEvent(client, "fixture/turn-start-requested");
  const result = await client.sendTurn([{ type: "text", text: "hello" }]);
  assert.equal((await requested).params.threadId, "thread-engine");
  assert.equal(result.turn.status, "completed");
});

test("failed initialization retires the process and can be retried", async (t) => {
  const directory = mkdtempSync(join(tmpdir(), "agent-engine-init-"));
  const marker = join(directory, "failed-once");
  const client = new CodexAppServerClient({
    command: [process.execPath, fixture],
    env: { ...process.env, FIXTURE_INITIALIZE_FAIL_ONCE_MARKER: marker },
    model: "fixture",
  });
  t.after(async () => {
    await client.close();
    rmSync(directory, { recursive: true, force: true });
  });
  await assert.rejects(client.ensureSession(), /fixture initialization failed/);
  assert.equal(await client.ensureSession(), "thread-engine");
});

test("cancellation before turn start is latched and interrupts the late turn", async (t) => {
  const client = new CodexAppServerClient({ command: [process.execPath, fixture], model: "fixture" });
  t.after(() => client.close());
  const startRequested = waitForEvent(client, "fixture/turn-start-requested");
  const completion = client.sendTurn([{ type: "text", text: "delayed-start" }]);
  await startRequested;
  assert.equal(await client.interruptTurn(), true);
  const result = await completion;
  assert.equal(result.turn.status, "interrupted");
});

test("close waits for the child process to actually exit", async () => {
  const client = new CodexAppServerClient({
    command: [process.execPath, fixture],
    env: { ...process.env, FIXTURE_SIGTERM_DELAY_MS: "80" },
    model: "fixture",
  });
  const started = waitForEvent(client, "fixture/thread-start-requested");
  await client.ensureSession();
  const pid = (await started).params.pid;
  const startedAt = Date.now();
  await client.close({ timeoutMs: 500 });
  // Windows terminates on SIGTERM rather than invoking the JavaScript handler.
  if (process.platform !== "win32") assert.ok(Date.now() - startedAt >= 60);
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
});

test("server request completion after close is safely discarded", async () => {
  const client = new CodexAppServerClient({
    command: [process.execPath, fixture],
    model: "fixture",
    serverRequestHandler: async () => {
      await new Promise((resolve) => setTimeout(resolve, 80));
      return { answers: {} };
    },
  });
  const requested = waitForEvent(client, "item/tool/requestUserInput");
  const completion = client.sendTurn([{ type: "text", text: "slow-host-request" }]);
  const rejected = assert.rejects(completion, /client closed/);
  await requested;
  await client.close();
  await rejected;
  await new Promise((resolve) => setTimeout(resolve, 100));
});

test("oversized JSONL is rejected before an unbounded line can accumulate", async (t) => {
  const client = new CodexAppServerClient({
    command: [process.execPath, fixture],
    env: { ...process.env, FIXTURE_OVERSIZED_JSONL: "1" },
    model: "fixture",
    requestTimeoutMs: 2_000,
  });
  t.after(() => client.close());
  await assert.rejects(client.ensureSession(), /exceeds byte limit/);
});

test("native transport preserves Unicode and CRLF across byte writes", async (t) => {
  const client = new CodexAppServerClient({ command: [process.execPath, fixture], model: "fixture" });
  t.after(() => client.close());
  assert.equal((await client.sendTurn([{ type: "text", text: "fragmented-unicode" }])).message, "故事🌙继续");
});

test("native transport rejects malformed frames and native RPC expires stalled initialization", async (t) => {
  for (const [setting, expected] of [["FIXTURE_INVALID_JSONL", /invalid JSONL/], ["FIXTURE_INITIALIZE_STALL", /request timed out: initialize/]]) {
    const client = new CodexAppServerClient({ command: [process.execPath, fixture], model: "fixture", env: { ...process.env, [setting]: "1" }, requestTimeoutMs: 150 });
    t.after(() => client.close());
    await assert.rejects(client.ensureSession(), expected);
    await client.close();
  }
});

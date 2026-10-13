import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { setImmediate as nextTurn } from "node:timers/promises";
import { test } from "node:test";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { createMcpStartupWaiter } from "./helpers/mcp-startup.mjs";

const serverName = "novel_workspace";
const secret = "sk-private-credential https://user:password@host full stderr";

function startup(status, name = serverName) {
  return { method: "mcpServer/startupStatus/updated", params: { name, status, error: secret } };
}

function setup(t, timeoutMs = 1_000) {
  const child = new EventEmitter();
  child.exitCode = null;
  child.signalCode = null;
  // Adapter owns an error handler; cleanup must preserve it and other listeners.
  const errors = [];
  child.on("error", (error) => errors.push(error));
  child.on("exit", () => {});
  const listeners = { error: child.listeners("error"), exit: child.listeners("exit") };
  const waiter = createMcpStartupWaiter({ serverName, process: child, timeoutMs });
  t.after(waiter.dispose);
  const adapter = new CodexAppServerAdapter({ command: null });
  const events = [];
  adapter.setCallbacks({ onEvent: (event) => { events.push(event); waiter.onEvent(event); } });
  const send = (event) => adapter.handleLine(JSON.stringify(event));
  const assertClean = () => assert.deepEqual(
    { error: child.listeners("error"), exit: child.listeners("exit") }, listeners,
  );
  return { child, waiter, send, events, errors, assertClean };
}

function diagnostic(code, message) {
  return (error) => {
    assert.deepEqual({ code: error.code, message: error.message, cause: error.cause }, { code, message, cause: undefined });
    assert.ok(!error.stack.includes(secret));
    assert.ok(!JSON.stringify(error).includes(secret));
    return true;
  };
}

test("only explicit ready for the configured server resolves through Adapter onEvent", async (t) => {
  const { waiter, send, events, assertClean } = setup(t);
  let settled = false;
  waiter.ready.then(() => { settled = true; });
  for (const event of [
    startup("ready", "other_server"), startup("failed", "other_server"),
    startup("ready", "novel_workspace_extra"), startup("starting"), startup("cancelled"),
    { method: "agent/ready", params: { name: serverName, status: "ready" } },
    { method: "mcpServer/startupStatus/updated", params: { status: "ready" } },
    { method: "mcpServer/startupStatus/updated", params: { name: serverName } },
  ]) send(event);
  waiter.onEvent(null);
  await nextTurn();
  assert.equal(settled, false);
  send(startup("ready"));
  assert.equal(await waiter.ready, undefined);
  assert.equal(events.length, 9);
  assertClean();
});

test("matching failed rejects immediately without copying the remote error", async (t) => {
  const { child, waiter, send, assertClean } = setup(t);
  send(startup("failed"));
  // Listener removal is synchronous, before any rejection handler runs.
  assertClean();
  send(startup("ready"));
  child.emit("exit", 0, null);
  await assert.rejects(waiter.ready, diagnostic("MCP_STARTUP_FAILED", `MCP server "${serverName}": startup failed before ready`));
});

test("process error rejects immediately and preserves the Adapter error listener", async (t) => {
  const { child, waiter, errors, assertClean } = setup(t);
  const error = new Error(secret);
  child.emit("error", error);
  assertClean();
  await assert.rejects(waiter.ready, diagnostic("MCP_PROCESS_ERROR", `MCP server "${serverName}": adapter process error before ready`));
  child.emit("error", error);
  assert.deepEqual(errors, [error, error]);
});

for (const [code, signal] of [[0, null], [1, null], [null, "SIGTERM"]]) {
  test(`process exit (${code}, ${signal}) rejects before ready`, async (t) => {
    const { child, waiter, assertClean } = setup(t);
    child.emit("exit", code, signal);
    assertClean();
    await assert.rejects(waiter.ready, diagnostic("MCP_PROCESS_EXIT", `MCP server "${serverName}": adapter process exited before ready`));
  });
}

test("already exited processes reject and clean up on construction", async () => {
  for (const state of [{ exitCode: 0, signalCode: null }, { exitCode: null, signalCode: "SIGTERM" }]) {
    const child = Object.assign(new EventEmitter(), state);
    const waiter = createMcpStartupWaiter({ serverName, process: child, timeoutMs: 1_000 });
    await assert.rejects(waiter.ready, { code: "MCP_PROCESS_EXIT" });
    assert.deepEqual(child.eventNames(), []);
  }
});

test("timeout stays bounded despite nonmatching events and stderr", async (t) => {
  const { waiter, send, assertClean } = setup(t, 20);
  send({ method: "agent/stderr", params: { text: secret } });
  send({ method: "agent/text", params: { text: secret } });
  send(startup("ready", "other_server"));
  send(startup("starting"));
  await assert.rejects(waiter.ready, diagnostic("MCP_STARTUP_TIMEOUT", `MCP server "${serverName}": timed out after 20 ms waiting for ready`));
  assertClean();
  send(startup("ready"));
  await assert.rejects(waiter.ready, { code: "MCP_STARTUP_TIMEOUT" });
});

test("ready clears the timer and stays resolved after late failures and dispose", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const { child, waiter, send, assertClean } = setup(t, 20);
  send(startup("ready"));
  assertClean();
  send(startup("failed"));
  child.emit("error", new Error(secret));
  child.emit("exit", 1, null);
  waiter.dispose();
  t.mock.timers.tick(20);
  assert.equal(await waiter.ready, undefined);
  assertClean();
});

test("dispose rejects pending waits, clears the timer, and is idempotent", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const { waiter, send, assertClean } = setup(t, 20);
  waiter.dispose();
  waiter.dispose();
  assertClean();
  t.mock.timers.tick(20);
  send(startup("ready"));
  await assert.rejects(waiter.ready, diagnostic("MCP_STARTUP_DISPOSED", `MCP server "${serverName}": startup wait disposed before ready`));
});

test("every terminal path clears its own deadline timer", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const schedule = t.mock.method(globalThis, "setTimeout");
  const cancel = t.mock.method(globalThis, "clearTimeout");
  for (const [trigger, code] of [
    [(fixture) => fixture.send(startup("ready")), null],
    [(fixture) => fixture.send(startup("failed")), "MCP_STARTUP_FAILED"],
    [(fixture) => fixture.child.emit("error", new Error(secret)), "MCP_PROCESS_ERROR"],
    [(fixture) => fixture.child.emit("exit", 1, null), "MCP_PROCESS_EXIT"],
    [(fixture) => fixture.waiter.dispose(), "MCP_STARTUP_DISPOSED"],
  ]) {
    const fixture = setup(t, 20);
    const timer = schedule.mock.calls.at(-1).result;
    trigger(fixture);
    assert.deepEqual(cancel.mock.calls.at(-1).arguments, [timer]);
    fixture.assertClean();
    if (code) await assert.rejects(fixture.waiter.ready, { code });
    else assert.equal(await fixture.waiter.ready, undefined);
  }
});

test("early rejections remain awaitable after a turn without unhandledRejection", async (t) => {
  const unhandled = [];
  const onUnhandled = (reason) => unhandled.push(reason);
  process.on("unhandledRejection", onUnhandled);
  t.after(() => process.off("unhandledRejection", onUnhandled));
  for (const [trigger, code] of [
    [(fixture) => fixture.send(startup("failed")), "MCP_STARTUP_FAILED"],
    [(fixture) => fixture.child.emit("error", new Error(secret)), "MCP_PROCESS_ERROR"],
    [(fixture) => fixture.child.emit("exit", 1, null), "MCP_PROCESS_EXIT"],
    [(fixture) => fixture.waiter.dispose(), "MCP_STARTUP_DISPOSED"],
  ]) {
    const fixture = setup(t);
    trigger(fixture);
    await nextTurn();
    await assert.rejects(fixture.waiter.ready, { code });
    fixture.assertClean();
  }
  const fixture = setup(t, 10);
  // Hold off awaiting ready until a later timer turn has observed its timeout.
  await new Promise((resolve) => setTimeout(resolve, 30));
  await assert.rejects(fixture.waiter.ready, { code: "MCP_STARTUP_TIMEOUT" });
  fixture.assertClean();
  assert.deepEqual(unhandled, []);
});

test("invalid input is rejected without echoing values or registering listeners", () => {
  const child = new EventEmitter();
  for (const name of [undefined, "", "x".repeat(65), secret]) {
    assert.throws(() => createMcpStartupWaiter({ serverName: name, process: child, timeoutMs: 20 }), {
      name: "TypeError", message: "MCP serverName must be a bounded server identifier",
    });
  }
  for (const timeoutMs of [undefined, 0, -1, 1.5, Infinity, NaN, "20", 2_147_483_648]) {
    assert.throws(() => createMcpStartupWaiter({ serverName, process: child, timeoutMs }), {
      name: "TypeError", message: "MCP timeoutMs must be a positive timer-safe integer",
    });
  }
  assert.throws(() => createMcpStartupWaiter({ serverName, timeoutMs: 20 }), {
    name: "TypeError", message: "MCP startup requires the adapter child process",
  });
  assert.deepEqual(child.eventNames(), []);
});

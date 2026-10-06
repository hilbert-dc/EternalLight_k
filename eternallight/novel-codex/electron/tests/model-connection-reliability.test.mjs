import assert from "node:assert/strict";
import test from "node:test";
import { ModelConnectionGate } from "../model-connection-gate.mjs";
import { checkCodexConnectionAccess, inspectCodexConnection } from "../codex-runtime.mjs";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

test("connection operations and model executions exclude each other and release failures", async () => {
  let executing = false, release;
  const gate = new ModelConnectionGate(() => executing);
  const active = gate.run(() => new Promise(resolve => { release = resolve; }));
  assert.throws(() => gate.assertExecutionAllowed(), /正在验证或切换/);
  await assert.rejects(gate.run(() => assert.fail("duplicate verification")), /正在验证或切换/);
  release(); await active;
  executing = true;
  await assert.rejects(gate.run(() => assert.fail("verification during execution")), /已有创作任务/);
  executing = false;
  await assert.rejects(gate.run(() => { throw Error("controlled failure"); }), /controlled failure/);
  gate.assertExecutionAllowed();
});

test("production execution, restore, verification and configuration entries respect the connection gate", async () => {
  const fixture = new URL("./fixture-app-server.mjs", import.meta.url).pathname;
  const { handlers, adapter, modelConnectionGate } = await mainHandlers(async () => assert.fail("blocked entry reached Runtime"), `${process.execPath} ${fixture}`);
  let release;
  const active = modelConnectionGate.run(() => new Promise(resolve => { release = resolve; }));
  try {
    for (const [channel, args] of [
      ["agent:verify-connection", ["deepseek", "deepseek-flash"]],
      ["agent:verify-connection-edge", ["deepseek", "deepseek-flash", "deepseek", "deepseek-flash"]],
      ["agent:configure", ["deepseek", "deepseek-flash"]],
      ["agent:resume-thread", ["project", "thread"]], ["agent:reconnect-thread", ["project", "thread", null]],
      ["agent:send-prompt", ["project", "分析故事"]], ["agent:generate-planning", ["project", "generateBrief", "故事"]],
      ["agent:generate-work", ["project", "故事", 1]], ["agent:resume-work-task", ["project", "work-fixture"]],
    ]) await assert.rejects(handlers.get(channel)(rendererEvent(), ...args), /模型连接正在验证或切换/, channel);
  } finally { release(); await active; adapter.close(); }
});

for (const phase of ["headers", "body"]) test(`account verification aborts its whole ${phase} wait without invalidating authentication`, async t => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  let signal, bodyStarted = false;
  const { connection } = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
  const request = checkCodexConnectionAccess({ apiKey: "fixture-secret", connection, fetchImpl: async (_url, options) => {
    signal = options.signal;
    if (phase === "headers") return new Promise(() => {});
    return { ok: true, json: () => { bodyStarted = true; return new Promise(() => {}); } };
  } });
  const rejected = assert.rejects(request, error => /超过 55 秒/.test(error.message) && error.statusCode === undefined);
  await Promise.resolve(); await Promise.resolve();
  if (phase === "body") assert.equal(bodyStarted, true);
  t.mock.timers.tick(55_000);
  await rejected;
  assert.equal(signal.aborted, true);
});

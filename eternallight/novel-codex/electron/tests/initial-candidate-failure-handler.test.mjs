import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";
import { coreTurnFailure } from "../core-turn-failure.mjs";

const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
// Production main/Adapter, controlled Runtime responses. The companion Runtime
// integration exercises actual admission; these cases isolate Host provenance.
for (const boundary of ["known", "coreTerminal", "foreignWork", "foreignTask", "cancelled", "uncreated"]) {
  test(`initial worker failure: ${boundary} preserves original authority`, async () => {
    const fixture = runtimeFixture(), calls = [];
    let issued, turns = 0, activeAdapter;
    const event = rendererEvent();
    const original = Object.assign(new Error("invalid_tool_arguments"), {
      code: boundary === "cancelled" ? "TASK_CANCELLED" : "invalid_tool_arguments",
    });
    const main = await mainHandlers(async (path, body, options) => {
      calls.push({ path, body });
      if (path.endsWith("/tasks") && body && boundary === "uncreated") throw original;
      if (issued && path.endsWith(`/tasks/${issued.id}`)) {
        if (options.method === "PATCH") {
          issued.status = body.status;
          issued.error = body.error;
          const work = [...fixture.workTasks.values()][0];
          work.status = "blocked"; work.steps[0].status = "blocked";
          return { ...issued };
        }
        return { ...issued, ...(boundary === "foreignTask" ? { threadId: "unrelated-thread" } : {}) };
      }
      if (issued && path.endsWith("/work-task") && boundary === "foreignWork" && issued.status === "failed") {
        return { ...[...fixture.workTasks.values()][0], id: "work-foreign" };
      }
      if (path.endsWith("/decision")) {
        const work = [...fixture.workTasks.values()][0];
        return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "executionBlocked" };
      }
      const value = await fixture.request(path, body);
      if (path.endsWith("/tasks") && body) {
        issued = value;
        [...fixture.workTasks.values()][0].steps[0].operation = body.operation;
      }
      return value;
    }, command, { onPrepareDomain(domain) {
      activeAdapter = domain.codexAdapter;
      const request = activeAdapter.request;
      activeAdapter.request = async function(method, params) {
        if (method === "turn/start") {
          turns++;
          if (boundary === "coreTerminal") await request.call(this, "fixture/incomplete-next-turn", {});
          if (boundary === "cancelled") await main.handlers.get("agent:interrupt")(event, main.executionTargets.get(event));
          if (boundary !== "coreTerminal") throw original;
        }
        return request.call(this, method, params);
      };
    } });
    try {
      const run = main.handlers.get("agent:generate-planning")(event, "project", "generateBrief", "修订故事方向");
      if (boundary === "coreTerminal") {
        await assert.rejects(run, error => {
          const failure = coreTurnFailure(error);
          assert.equal(failure.code, "CORE_TURN_FAILED");
          assert.equal(failure.threadId, issued.threadId);
          assert.equal(failure.turnId, "turn-test");
          assert.equal(failure.automaticContinuation, "stopped");
          return true;
        });
        assert.equal(issued.status, "failed");
        assert.match(issued.error, /^core_turn_failed; response incomplete/);
        assert.equal(calls.filter(call => call.path.endsWith("/decision")).length, 0);
      } else if (boundary === "known") {
        const result = await run;
        assert.equal(result.executionFailure.sourceTaskId, issued.id);
        assert.equal(result.executionFailure.workTaskId, [...fixture.workTasks.keys()][0]);
        assert.deepEqual(JSON.parse(JSON.stringify(result.candidateResults)), []);
        assert.equal(result.originalFailedTaskId, undefined);
        assert.equal(calls.filter(call => call.path.endsWith("/decision")).length, 1);
      } else {
        await assert.rejects(run, error => {
          if (boundary === "cancelled") return error.code === "TASK_CANCELLED";
          if (boundary === "foreignTask") {
            assert.equal(error.cause?.message, original.message);
            assert.equal(error.message, `${original.message}；原任务终止状态尚待核对：中断任务身份无法核对`);
            assert.equal(calls.filter(call => call.path.endsWith(`/tasks/${issued.id}`) && call.body).length, 0,
              "an unverified foreign task must never receive a terminal write");
            return true;
          }
          return error.message === original.message;
        });
        assert.equal(calls.filter(call => call.path.endsWith("/decision")).length, 0);
      }
      assert.equal(turns, boundary === "uncreated" ? 0 : 1);
      assert.equal(calls.filter(call => call.path.endsWith("/tasks") && call.body).length, 1);
      assert.equal(calls.filter(call => call.path.endsWith("/budget-authorization") && call.body).length, 0);
    } finally { activeAdapter?.close(); main.adapter.close(); main.chapterStructureConsumer.stop(); }
  });
}

import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";

async function loadAgentEvents() {
  const directory = await mkdtemp(join(tmpdir(), "novel-codex-agent-events-"));
  const output = join(directory, "agent-events.mjs");
  await build({
    entryPoints: [new URL("../../src/agent-events.ts", import.meta.url).pathname],
    outfile: output,
    bundle: true,
    format: "esm",
    platform: "node",
  });
  return { directory, module: await import(`${pathToFileURL(output).href}?test=${Date.now()}`) };
}

test("renders native Codex delegation as semantic author-facing progress", async () => {
  const loaded = await loadAgentEvents();
  try {
    let run = loaded.module.createAgentRun("联合审阅这一章");
    run = loaded.module.reduceAgentEvent(run, { method: "thread/started", params: { thread: { id: "root-thread" } } });
    run = loaded.module.reduceAgentEvent(run, { method: "turn/started", params: { threadId: "root-thread", turn: { id: "root-turn" } } });
    run = loaded.module.reduceAgentEvent(run, {
      method: "item/started",
      params: {
        threadId: "root-thread",
        turnId: "root-turn",
        item: {
          id: "delegation-1",
          type: "collabAgentToolCall",
          tool: "spawnAgent",
          status: "inProgress",
          receiverThreadIds: ["child-1", "child-2"],
          prompt: "分别检查结构与人物一致性",
          agentsStates: {
            "child-1": { status: "running" },
            "child-2": { status: "pendingInit" },
          },
        },
      },
    });
    const delegation = run.steps.find((step) => step.id === "delegation-1");
    assert.deepEqual(delegation, {
      id: "delegation-1",
      kind: "interaction",
      title: "委派小说审阅",
      detail: "2 位子 Agent\n分析中 1 · 准备中 1\n任务：分别检查结构与人物一致性",
      status: "running",
    });

    const unchanged = loaded.module.reduceAgentEvent(run, {
      method: "thread/status/changed",
      params: { threadId: "child-1", status: { type: "idle" } },
    });
    assert.deepEqual(unchanged, run);

    run = loaded.module.reduceAgentEvent(run, {
      method: "item/completed",
      params: {
        threadId: "root-thread",
        turnId: "root-turn",
        item: {
          id: "delegation-1",
          type: "collabAgentToolCall",
          tool: "spawnAgent",
          status: "completed",
          receiverThreadIds: ["child-1", "child-2"],
          agentsStates: {
            "child-1": { status: "completed" },
            "child-2": { status: "completed" },
          },
        },
      },
    });
    assert.equal(run.steps.find((step) => step.id === "delegation-1")?.status, "completed");
    assert.match(run.steps.find((step) => step.id === "delegation-1")?.detail ?? "", /已完成 2/);
  } finally {
    await rm(loaded.directory, { recursive: true, force: true });
  }
});

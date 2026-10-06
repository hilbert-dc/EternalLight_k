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
    entryPoints: { "agent-events": new URL("../../src/agent-events.ts", import.meta.url).pathname,
      "agent-error-description": new URL("../../src/agent-error-description.ts", import.meta.url).pathname },
    outdir: directory,
    outExtension: { ".js": ".mjs" },
    bundle: true,
    format: "esm",
    platform: "node",
  });
  return { directory, module: await import(`${pathToFileURL(output).href}?test=${Date.now()}`),
    description: await import(pathToFileURL(join(directory, "agent-error-description.mjs")).href) };
}

test("author execution history retains bounded public process without private reasoning or deleting earlier messages", async () => {
  const loaded = await loadAgentEvents();
  try {
    const { createAgentRun, summarizeAgentRun } = loaded.module;
    const steps = [{ id: "private", sourceType: "reasoning", kind: "system", title: "正在分析", text: "PRIVATE_REASONING", detail: "OPAQUE", status: "completed" },
      { id: "saved", kind: "tool", title: "保存正文", status: "completed" },
      { id: "stopped", kind: "tool", title: "查询故事设定", status: "cancelled", text: "已停止" }];
    const run = { ...createAgentRun("作者指令"), state: "cancelled", threadId: "thread", turnId: "turn", startedAt: 1000, completedAt: 17000, steps };
    const summary = summarizeAgentRun(run, 17000);
    assert.deepEqual(summary.displayExecution, { steps: steps.slice(1), startedAt: 1000, completedAt: 17000, totalPublicSteps: 2, detailsTruncated: false });
    assert.ok(!JSON.stringify(summary).includes("PRIVATE_REASONING") && !JSON.stringify(summary).includes("OPAQUE"));
    const many = Array.from({ length: 120 }, (_, index) => ({ id: `step-${index}`, kind: "tool", title: "查询故事设定", status: "completed", detail: "界".repeat(4000) }));
    const large = summarizeAgentRun({ ...run, steps: many }, 17000).displayExecution;
    assert.equal(large.totalPublicSteps, 120); assert.equal(large.detailsTruncated, true);
    assert.ok(large.steps.length < 96 && Buffer.byteLength(JSON.stringify(large)) <= 24 * 1024);
    const countOnly = summarizeAgentRun({ ...run, steps: many.map(({ detail, ...step }) => step) }, 17000).displayExecution;
    assert.equal(countOnly.steps.length, 96);
    const history = Array.from({ length: 14 }, (_, index) => ({ ...summary, id: `history-${index}`, prompt: `指令${index}` }));
    const next = createAgentRun("下一轮", history);
    assert.equal(next.history.length, 14);
    assert.deepEqual(next.history.map(entry => ({ prompt: entry.prompt, kept: Boolean(entry.displayExecution) })), history.map((entry, index) => ({ prompt: entry.prompt, kept: index < 10 })));
    assert.equal(history[13].displayExecution, summary.displayExecution);
  } finally { await rm(loaded.directory, { recursive: true, force: true }); }
});

test("host workflow binds active attempt identity without accepting late or foreign scope", async () => {
  const loaded = await loadAgentEvents();
  try {
    const { createAgentRun, reduceAgentEvent: reduce, summarizeAgentRun } = loaded.module;
    const initial = { ...createAgentRun("作者指令"), threadId: "thread" };
    const workflow = (task, params = {}) => ({ method: "agent/workflow", params: { ...params, workflow: { task } } });
    const identity = { id: "attempt", threadId: "thread" };
    const bound = reduce(initial, workflow(identity));
    assert.deepEqual({ taskId: bound.taskId, threadId: bound.threadId, turnId: bound.turnId },
      { taskId: "attempt", threadId: "thread", turnId: null });
    for (const task of [{ ...identity, id: "different" }, { ...identity, threadId: "foreign" },
      { ...identity, id: "x".repeat(161) }, { ...identity, threadId: "x".repeat(161) },
      { ...identity, id: " " }, { id: "attempt" }]) assert.strictEqual(reduce(bound, workflow(task)), bound);
    assert.strictEqual(reduce(initial, workflow(identity, { threadId: "foreign" })), initial);
    const started = reduce(bound, { method: "turn/started", params: { threadId: "thread", turn: { id: "turn" } } });
    const cancelled = reduce(started, { method: "turn/completed", params: { threadId: "thread", turn: { id: "turn", status: "interrupted" } } });
    assert.deepEqual({ taskId: summarizeAgentRun(cancelled, 1).taskId, threadId: cancelled.threadId, turnId: cancelled.turnId },
      { taskId: "attempt", threadId: "thread", turnId: "turn" });
    assert.strictEqual(reduce(cancelled, workflow({ id: "new", threadId: "thread" })), cancelled);
    assert.strictEqual(reduce(createAgentRun(), workflow(identity)).state, "idle");
  } finally { await rm(loaded.directory, { recursive: true, force: true }); }
});

test("private reasoning remains fixed activity across started, delta, completed and terminal replay", async () => {
  const loaded = await loadAgentEvents();
  try {
    const { createAgentRun, reduceAgentEvent: reduce } = loaded.module;
    let run = reduce(createAgentRun("分析本章"), { method: "thread/started", params: { thread: { id: "thread" } } });
    run = reduce(run, { method: "turn/started", params: { threadId: "thread", turn: { id: "turn" } } });
    const event = (method, params, turnId = "turn") => ({ method, params: { threadId: "thread", turnId, ...params } });
    const secret = "PRIVATE_RAW_REASONING";
    const item = { id: "analysis", type: "reasoning", text: secret, summary: secret,
      encrypted_content: secret, content: [{ text: secret }], command: secret };
    run = reduce(run, event("item/started", { item }));
    for (const method of ["item/reasoning/textDelta", "item/reasoning/summaryTextDelta", "item/reasoning/summaryPartAdded"]) {
      run = reduce(run, event(method, { itemId: "analysis", delta: secret, text: secret }));
    }
    run = reduce(run, event("item/outputDelta", { itemId: "analysis", delta: secret }));
    assert.deepEqual(run.steps.find(step => step.id === "analysis"), {
      id: "analysis", kind: "system", title: "正在分析", sourceType: "reasoning", status: "running",
    });
    assert.equal(JSON.stringify(run).includes(secret), false);
    assert.strictEqual(reduce(run, event("item/reasoning/textDelta", { itemId: "foreign", delta: secret }, "old-turn")), run);
    run = reduce(run, event("item/completed", { item: { ...item, status: "completed" } }));
    assert.strictEqual(reduce(run, event("item/reasoning/textDelta", { itemId: "analysis", delta: secret })), run);
    for (const [id, phase, text] of [["progress", "commentary", "已读取本章"], ["reply", "final_answer", "本章已保存"]]) {
      run = reduce(run, event("item/completed", { item: { id, type: "agentMessage", phase, text } }));
    }
    run = reduce(run, event("turn/completed", { turn: { id: "turn", status: "completed", items: [item] } }));
    assert.deepEqual({ answer: run.answer, commentary: run.steps.find(step => step.id === "message:progress")?.text,
      reasoning: run.steps.find(step => step.id === "analysis") }, {
      answer: "本章已保存", commentary: "已读取本章",
      reasoning: { id: "analysis", kind: "system", title: "正在分析", sourceType: "reasoning", status: "completed" },
    });
    assert.equal(JSON.stringify(run).includes(secret), false);
    assert.strictEqual(reduce(run, event("item/reasoning/textDelta", { itemId: "analysis", delta: secret })), run);
  } finally { await rm(loaded.directory, { recursive: true, force: true }); }
});

test("current MCP and dynamic tool events retain their tool titles across start and completion", async () => {
  const loaded = await loadAgentEvents();
  try {
    const { createAgentRun, reduceAgentEvent: reduce } = loaded.module;
    let run = reduce(createAgentRun("查询依据后保存"), { method: "thread/started", params: { thread: { id: "thread" } } });
    run = reduce(run, { method: "turn/started", params: { threadId: "thread", turn: { id: "turn" } } });
    for (const item of [
      { id: "saved", type: "mcpToolCall", server: "novel_workspace", tool: "save_draft_checkpoint", arguments: {} },
      { id: "query", type: "dynamicToolCall", namespace: null, tool: "query_story", arguments: {} },
    ]) {
      const event = (method, status) => ({ method, params: { threadId: "thread", turnId: "turn", item: { ...item, status } } });
      run = reduce(run, event("item/started", "inProgress"));
      const started = run.steps.find(step => step.id === item.id);
      run = reduce(run, event("item/completed", "completed"));
      assert.deepEqual(run.steps.find(step => step.id === item.id), { ...started, status: "completed" });
    }
    assert.deepEqual(run.steps.filter(step => ["saved", "query"].includes(step.id)), [
      { id: "saved", kind: "tool", title: "保存章节", detail: undefined, status: "completed" },
      { id: "query", kind: "tool", title: "查询故事证据", detail: undefined, status: "completed" },
    ]);
  } finally { await rm(loaded.directory, { recursive: true, force: true }); }
});

test("stopping preserves saved tool success and marks unfinished work stopped, not successful or failed", async () => {
  const loaded = await loadAgentEvents();
  try {
    const { createAgentRun, reduceAgentEvent: reduce } = loaded.module;
    let run = reduce(createAgentRun("保存后分析本章"), { method: "thread/started", params: { thread: { id: "thread" } } });
    run = reduce(run, { method: "turn/started", params: { threadId: "thread", turn: { id: "turn" } } });
    const event = (method, params) => ({ method, params: { threadId: "thread", turnId: "turn", ...params } });
    run = reduce(run, event("item/completed", { item: { id: "saved", type: "mcpToolCall", server: "novel_workspace", tool: "save_draft_checkpoint" } }));
    const saved = run.steps.find(step => step.id === "saved");
    run = reduce(run, event("item/started", { item: { id: "unfinished", type: "mcpToolCall", server: "novel_workspace", tool: "publish_knowledge_batch" } }));
    run = reduce(run, event("turn/completed", { turn: { id: "turn", status: "interrupted" } }));
    assert.deepEqual({ state: run.state, saved: run.steps.find(step => step.id === "saved"), unfinished: run.steps.find(step => step.id === "unfinished") }, {
      state: "cancelled", saved,
      unfinished: { id: "unfinished", kind: "tool", title: "保存故事设定分析", detail: undefined, status: "cancelled" },
    });
    assert.deepEqual(Object.fromEntries(run.phases.map(phase => [phase.id, phase.status])), {
      context: "skipped", planning: "completed", tools: "skipped", response: "pending",
    });
    for (const late of [event("item/completed", { item: { id: "unfinished", type: "mcpToolCall" } }),
      event("item/started", { item: { id: "unfinished", type: "mcpToolCall" } }),
      event("item/tool/requestUserInput", { questions: [{ question: "迟到问题" }] }),
      event("thread/status/changed", { status: { type: "active", activeFlags: ["waitingOnUserInput"] } }),
      event("turn/completed", { turn: { id: "turn", status: "completed" } })]) {
      assert.strictEqual(reduce(run, late), run);
    }
  } finally { await rm(loaded.directory, { recursive: true, force: true }); }
});

test("elapsed execution includes session startup and retains its submission timestamp", async (t) => {
  const loaded = await loadAgentEvents();
  let now = 1_800_000_000_000;
  t.mock.method(Date, "now", () => now);
  try {
    const { createAgentRun, reduceAgentEvent: reduce } = loaded.module;
    assert.equal(createAgentRun().startedAt, null);
    let run = createAgentRun("分析当前作品");
    const submittedAt = now;
    now += 90_000;
    run = reduce(run, { method: "thread/started", params: { thread: { id: "thread" } } });
    run = reduce(run, { method: "turn/started", params: { threadId: "thread", turn: { id: "turn" } } });
    now += 7_000;
    run = reduce(run, { method: "turn/completed", params: { threadId: "thread",
      turn: { id: "turn", status: "completed", items: [] } } });
    assert.deepEqual({ startedAt: run.startedAt, elapsed: run.completedAt - run.startedAt },
      { startedAt: submittedAt, elapsed: 97_000 });
    now += 1_000;
    assert.equal(createAgentRun("重试讨论", run.history).startedAt, now);
  } finally {
    t.mock.restoreAll();
    await rm(loaded.directory, { recursive: true, force: true });
  }
});

test("background recovery failures do not invent an author turn before the next successful generation", async () => {
  const loaded = await loadAgentEvents();
  try {
    const { createAgentRun, reduceAgentEvent: reduce, summarizeAgentRun } = loaded.module;
    const error = "provider_output_item_limit: 11003 tokens";
    const recovery = reduce(createAgentRun(), { method: "agent/error", params: { message: error } });
    assert.equal(recovery.error, error, "Recovery evidence remains in the current run");
    assert.equal(summarizeAgentRun(recovery, 100), null, "An unsubmitted recovery must not become an author turn");
    const failed = reduce(createAgentRun("修改第一章的结尾"), { method: "agent/error", params: { message: error } });
    const earlier = summarizeAgentRun(failed, 200);
    assert.deepEqual({ prompt: earlier.prompt, state: earlier.state, summary: earlier.summary }, {
      prompt: "修改第一章的结尾", state: "error", summary: "本轮未完成，可检查错误后重试。",
    });
    let next = createAgentRun("生成第二章", [earlier]);
    next = reduce(next, { method: "thread/started", params: { thread: { id: "new-thread" } } });
    next = reduce(next, { method: "turn/started", params: { threadId: "new-thread", turn: { id: "new-turn" } } });
    next = reduce(next, { method: "turn/completed", params: { threadId: "new-thread", turn: { id: "new-turn", status: "completed",
      items: [{ id: "reply", type: "agentMessage", phase: "final_answer", text: "第二章已保存" }] } } });
    assert.deepEqual({ state: next.state, error: next.error, answer: next.answer, summary: next.summary, history: next.history }, {
      state: "completed", error: null, answer: "第二章已保存", summary: "创作引擎已完成本轮创作。", history: [earlier],
    });
  } finally { await rm(loaded.directory, { recursive: true, force: true }); }
});

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
      collaborators: [
        { id: "child-1", status: "running", task: "分别检查结构与人物一致性" },
        { id: "child-2", status: "pendingInit", task: "分别检查结构与人物一致性" },
      ],
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
            "child-1": { status: "completed", message: "第三章人物尚未得知真相，需要补充信息来源。" },
            "child-2": { status: "errored", message: "预算耗尽，结构分析未完成。" },
          },
        },
      },
    });
    assert.equal(run.steps.find((step) => step.id === "delegation-1")?.status, "completed");
    const detail = run.steps.find((step) => step.id === "delegation-1")?.detail ?? "";
    assert.match(detail, /已完成 1 · 失败 1/);
    assert.match(detail, /child-1 · 已完成：第三章人物尚未得知真相/);
    assert.match(detail, /child-2 · 失败：预算耗尽/);
  } finally {
    await rm(loaded.directory, { recursive: true, force: true });
  }
});

test("keeps current-protocol replies and lifecycle events within their owning turn", async (t) => {
  const loaded = await loadAgentEvents();
  const { createAgentRun, reduceAgentEvent: reduce } = loaded.module;
  const event = (method, params = {}, turnId = "current-turn", threadId = "root-thread") => ({
    method, params: { threadId, turnId, ...params },
  });
  const start = () => reduce({ ...createAgentRun("分析当前作品"), threadId: "root-thread" }, {
    method: "turn/started", params: { threadId: "root-thread", turn: { id: "current-turn" } },
  });
  const message = (id, text, phase) => ({ id, type: "agentMessage", text, phase });
  const completed = (items = [], status = "completed", id = "current-turn") => ({
    method: "turn/completed", params: { threadId: "root-thread", turn: { id, status, items, error: { message: "本回合失败" } } },
  });
  try {
    await t.test("completion selects the last explicit final reply, not the first message or last commentary", () => {
      const run = reduce(start(), completed([
        message("progress", "读取完毕，继续分析。", "commentary"),
        message("unphased", "未标注阶段的答复", null),
        message("final-1", "初步结论", "final_answer"),
        message("final-2", "最终结论", "final_answer"),
        message("tail", "收尾进度", "commentary"),
        message("null-tail", "没有明确最终阶段", null),
      ]));
      assert.deepEqual({ state: run.state, answer: run.answer, reply: run.steps.find(step => step.id === "agent-message") }, {
        state: "completed", answer: "最终结论",
        reply: { id: "agent-message", kind: "assistant", title: "Agent 回复", text: "最终结论", status: "completed", messagePhase: "final_answer" },
      });
    });

    await t.test("streams are per item, preserve whitespace, and commentary cannot overwrite a final", () => {
      let run = start();
      run = reduce(run, event("item/started", { item: message("progress", "", "commentary") }));
      run = reduce(run, event("item/agentMessage/delta", { itemId: "progress", delta: "正在分析" }));
      assert.equal(run.answer, "");
      run = reduce(run, event("item/started", { item: message("reply", "", "final_answer") }));
      for (const delta of ["第一段", "\n\n", "第二段", " ", "结论"]) {
        run = reduce(run, event("item/agentMessage/delta", { itemId: "reply", delta }));
      }
      assert.equal(run.answer, "第一段\n\n第二段 结论");
      run = reduce(run, event("item/completed", { item: message("reply", "正式答复", "final_answer") }));
      const finalStep = run.steps.find(step => step.id === "agent-message");
      run = reduce(run, event("item/completed", { item: message("progress", "分析完成", "commentary") }));
      run = reduce(run, event("item/agentMessage/delta", { itemId: "late-message", delta: "迟到的未标注片段" }));
      assert.equal(run.answer, "正式答复");
      assert.deepEqual(run.steps.find(step => step.id === "agent-message"), finalStep);
      assert.strictEqual(reduce(run, event("item/agentMessage/delta", { itemId: "reply", delta: "重复片段" })), run);
      assert.equal(reduce(run, completed()).answer, "正式答复");
    });

    await t.test("null-phase public replies work, empty finals remain authoritative, streams alone are not final evidence", () => {
      assert.equal(reduce(start(), completed([message("reply", "当前协议允许的普通回复", null)])).answer, "当前协议允许的普通回复");
      assert.equal(reduce(start(), completed([message("old", "旧的普通回复", null), message("reply", "", "final_answer")])).answer, "");
      let run = reduce(start(), event("item/agentMessage/delta", { itemId: "stream", delta: "尚未确认的流" }));
      run = reduce(run, completed());
      assert.equal(run.answer, "");
      assert.equal(run.steps.find(step => step.id === "agent-message"), undefined);
      run = reduce(start(), completed([message("comment", "仅进度", "commentary"), message("bad-phase", "无效阶段", "unknown")]));
      assert.equal(run.answer, "");
    });

    await t.test("phase arriving after deltas removes provisional commentary from the answer", () => {
      let run = reduce(start(), event("item/agentMessage/delta", { itemId: "unclassified", delta: "仅是进度" }));
      run = reduce(run, event("item/completed", { item: message("unclassified", "仅是进度", "commentary") }));
      assert.equal(run.answer, "");
      assert.equal(run.steps.find(step => step.id === "agent-message"), undefined);
      assert.equal(run.steps.find(step => step.id === "message:unclassified")?.text, "仅是进度");
    });

    await t.test("old same-thread events cannot alter any part of a running or waiting new turn", () => {
      const events = [
        event("turn/started", { turn: { id: "old-turn" } }, "old-turn"),
        ...["completed", "failed", "interrupted"].map(status => completed([message("old-reply", "旧回答", "final_answer")], status, "old-turn")),
        event("error", { error: { message: "旧错误" }, willRetry: false }, "old-turn"),
        event("error", { error: { message: "旧重试" }, willRetry: true }, "old-turn"),
        event("agent/error", { message: "旧宿主错误" }, "old-turn"),
        event("item/started", { item: message("old-reply", "旧回答", "commentary") }, "old-turn"),
        event("item/completed", { item: message("old-reply", "旧回答", "final_answer") }, "old-turn"),
        event("item/completed", { item: { id: "old-write", type: "fileChange", changes: [{ path: "chapter.md", diff: "+旧内容" }] } }, "old-turn"),
        event("item/agentMessage/delta", { itemId: "reply", delta: "旧流" }, "old-turn"),
        event("item/commandExecution/outputDelta", { itemId: "tool", delta: "旧输出" }, "old-turn"),
        event("item/tool/requestUserInput", { questions: [{ question: "旧问题" }] }, "old-turn"),
        event("turn/plan/updated", { plan: [{ step: "旧计划", status: "completed" }] }, "old-turn"),
        event("turn/diff/updated", { diff: "+旧变更" }, "old-turn"),
        event("thread/tokenUsage/updated", { tokenUsage: { total: { totalTokens: 123 } } }, "old-turn"),
        event("model/rerouted", { toModel: "old-model" }, "old-turn"),
        ...["hook/started", "hook/completed", "agent/review", "agent/workflow", "agent/phase", "agent/steer", "agent/compactionStarted", "agent/compactionCompleted", "agent/softwarePermissionDenied", "serverRequest/resolved"]
          .map(method => event(method, { id: "response", prompt: "旧指令", requestId: 42 }, "old-turn")),
      ];
      const running = reduce(start(), event("item/completed", { item: message("reply", "当前结论", "final_answer") }));
      const waiting = reduce(running, { ...event("item/tool/requestUserInput", { questions: [{ question: "当前问题" }] }), id: 42 });
      for (const initial of [running, waiting]) {
        for (const stale of events) assert.strictEqual(reduce(initial, stale), initial, `${initial.state}: ${stale.method}`);
      }
      assert.equal(reduce(running, completed()).state, "completed");
    });

    await t.test("pre-start native events, wrong threads, conflicting identifiers, and missing native ownership are ignored", () => {
      const beforeStart = { ...createAgentRun("下一轮"), threadId: "root-thread" };
      assert.strictEqual(reduce(beforeStart, completed()), beforeStart);
      assert.strictEqual(reduce(beforeStart, event("error", { error: { message: "迟到" } })), beforeStart);
      const run = start();
      assert.strictEqual(reduce(run, event("item/completed", { item: message("child", "子线程", "final_answer") }, "current-turn", "child-thread")), run);
      assert.strictEqual(reduce(run, event("turn/completed", { turn: { id: "old-turn", status: "completed" } })), run);
      assert.strictEqual(reduce(run, { method: "item/completed", params: { threadId: "root-thread", item: message("no-turn", "无归属", "final_answer") } }), run);
      assert.strictEqual(reduce(run, { method: "item/completed", params: { turnId: "current-turn", item: message("no-thread", "无归属", "final_answer") } }), run);
      assert.strictEqual(reduce(run, completed([], "inProgress")), run);
    });

    await t.test("current host preparation and phase events remain valid without native turn IDs", () => {
      let run = { ...createAgentRun("准备新任务"), threadId: "root-thread" };
      run = reduce(run, { method: "agent/workflow", params: { workflow: { task: { id: "task-current", threadId: "root-thread" }, context: { purpose: "writer", facts: [{}] } } } });
      assert.deepEqual(run.steps.find(step => step.id === "workflow:prepared"), {
        id: "workflow:prepared", kind: "system", title: "准备创作上下文", detail: "writer · 1 条时态事实 · 任务 task-current", status: "completed",
      });
      run = reduce(run, { method: "agent/phase", params: { id: "context", status: "completed", detail: "宿主准备完成" } });
      assert.deepEqual(run.phases[0], { id: "context", label: "读取上下文", status: "completed", detail: "宿主准备完成" });
      assert.equal(run.turnId, null);
      run = reduce(run, { method: "turn/started", params: { threadId: "root-thread", turn: { id: "current-turn" } } });
      run = reduce(run, { method: "agent/phase/started", params: { threadId: "root-thread", id: "planning" } });
      assert.equal(run.phases[1].status, "running");
      assert.equal(run.steps.find(step => step.id === "workflow:prepared")?.status, "completed");
    });

    await t.test("thread idle and duplicate starts cannot prematurely complete or reset the run", () => {
      const run = reduce(start(), event("turn/plan/updated", { plan: [{ step: "当前计划", status: "inProgress" }] }));
      assert.strictEqual(reduce(run, { method: "thread/status/changed", params: { threadId: "root-thread", status: { type: "idle" } } }), run);
      assert.strictEqual(reduce(run, { method: "turn/started", params: { threadId: "root-thread", turn: { id: "current-turn" } } }), run);
    });

    await t.test("completed item progress is idempotent and cannot reactivate a finished tool", () => {
      const item = { id: "compact", type: "contextCompaction", status: "completed" };
      const run = reduce(start(), event("item/completed", { item }));
      assert.equal(run.contextCompactions, 1);
      for (const duplicate of [event("item/completed", { item }), event("item/started", { item }),
        event("item/commandExecution/outputDelta", { itemId: "compact", delta: "迟到输出" })]) {
        assert.strictEqual(reduce(run, duplicate), run);
      }
    });

    await t.test("terminal turns do not resurrect on delayed items, starts, errors or duplicate completion", () => {
      for (const status of ["completed", "failed", "interrupted"]) {
        const run = reduce(start(), completed([message("reply", "已确认", "final_answer")], status));
        for (const delayed of [
          event("item/agentMessage/delta", { itemId: "reply", delta: "迟到" }),
          event("item/completed", { item: message("reply", "迟到替换", "final_answer") }),
          event("error", { error: { message: "迟到错误" } }),
          event("agent/error", { message: "迟到宿主错误" }),
          completed(), event("turn/started", { turn: { id: "current-turn" } }),
        ]) assert.strictEqual(reduce(run, delayed), run, `${status}: ${delayed.method}`);
      }
    });

    await t.test("transient current-turn errors keep execution active and final failure remains visible", () => {
      let run = reduce(start(), event("error", { error: { message: "Reconnecting..." }, willRetry: true }));
      assert.deepEqual({ state: run.state, error: run.error, completedAt: run.completedAt, connection: run.connectionState }, {
        state: "running", error: null, completedAt: null, connection: "ready",
      });
      run = reduce(run, completed([message("reply", "恢复后完成", "final_answer")]));
      assert.deepEqual({ state: run.state, answer: run.answer, connection: run.connectionState }, { state: "completed", answer: "恢复后完成", connection: "ready" });
      assert.equal(reduce(start(), completed([], "failed")).error, "本回合失败");
    });

    await t.test("task failure retains the explicit connection state and readable error keeps its raw evidence", () => {
      const raw = 'Error invoking resumeWorkTask: {"code":"provider_output_item_limit","itemTokens":11003}';
      for (const connectionState of ["ready", "offline", "connecting", "reconnecting", "error"]) {
        const connected = reduce(start(), { method: "agent/connection", params: { state: connectionState } });
        const failed = reduce(connected, event("agent/error", { message: raw }));
        assert.deepEqual({ state: failed.state, connection: failed.connectionState, error: failed.error,
          detail: failed.steps.at(-1).detail, description: loaded.description.describeAgentError(failed.error) }, {
          state: "error", connection: connectionState, error: raw, detail: raw,
          description: "模型返回的一条结果超过可处理的长度上限，本次任务未完成。",
        });
      }
      const rawOther = "Error invoking resumeWorkTask: 当前作品版本已变化";
      assert.equal(loaded.description.describeAgentError(rawOther), rawOther);
      assert.equal(reduce(start(), completed([], "failed")).connectionState, "ready");
    });

    await t.test("resolved requests belong to a pending question, support numeric IDs and do not unblock another question", () => {
      let run = start();
      for (const id of [41, 42]) run = reduce(run, { ...event("item/tool/requestUserInput", { questions: [{ question: `问题 ${id}` }] }), id });
      assert.strictEqual(reduce(run, { method: "serverRequest/resolved", params: { threadId: "root-thread", requestId: 7 } }), run);
      run = reduce(run, event("item/completed", { item: { id: "background-tool", type: "mcpToolCall", status: "completed" } }));
      run = reduce(run, event("item/commandExecution/outputDelta", { itemId: "background-tool", delta: "后台进度" }));
      run = reduce(run, event("item/agentMessage/delta", { itemId: "reply", delta: "当前进度" }));
      assert.equal(run.state, "waiting", "in-flight item progress cannot resolve an unanswered question");
      run = reduce(run, { method: "serverRequest/resolved", params: { threadId: "root-thread", requestId: 41 } });
      assert.equal(run.state, "waiting");
      assert.equal(run.steps.find(step => step.id === "user-input:41")?.status, "completed");
      run = reduce(run, { method: "serverRequest/resolved", params: { threadId: "root-thread", requestId: 42 } });
      assert.equal(run.state, "running");
      assert.strictEqual(reduce(run, { method: "serverRequest/resolved", params: { threadId: "root-thread", requestId: 42 } }), run);
    });

    await t.test("message identity is independent of the renderer answer alias", () => {
      let run = reduce(start(), event("item/started", { item: message("agent-message", "", "final_answer") }));
      run = reduce(run, event("item/agentMessage/delta", { itemId: "agent-message", delta: "有效回复" }));
      assert.equal(run.answer, "有效回复");
      run = reduce(run, event("item/completed", { item: message("agent-message", "最终回复", "final_answer") }));
      assert.equal(reduce(run, completed()).answer, "最终回复");
    });

    await t.test("explicit review starts a clean new turn and old replies cannot leak into it", () => {
      const first = reduce(start(), completed([message("old-reply", "先前回复", "final_answer")]));
      let run = reduce(first, { method: "agent/review", params: { threadId: "root-thread" } });
      run = reduce(run, { method: "turn/started", params: { threadId: "root-thread", turn: { id: "review-turn" } } });
      assert.equal(run.turnId, "review-turn");
      assert.equal(run.answer, "");
      assert.strictEqual(reduce(run, completed()), run);
      run = reduce(run, { method: "turn/completed", params: { threadId: "root-thread", turn: { id: "review-turn", status: "completed", items: [message("review-reply", "本轮审阅结果", "final_answer")] } } });
      assert.equal(run.answer, "本轮审阅结果");
      run = reduce(run, { method: "agent/review", params: { threadId: "root-thread", delivery: "inline", target: "uncommittedChanges" } });
      assert.deepEqual({ state: run.state, turnId: run.turnId, answer: run.answer, review: run.review }, {
        state: "completed", turnId: "review-turn", answer: "本轮审阅结果", review: "completed",
      });
      assert.strictEqual(reduce(run, event("agent/steer", { prompt: "迟到追加" }, "review-turn")), run);
    });
  } finally {
    await rm(loaded.directory, { recursive: true, force: true });
  }
});

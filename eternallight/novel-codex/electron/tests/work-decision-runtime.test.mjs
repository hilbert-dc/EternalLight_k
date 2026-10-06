import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

// Production main handler/coordinator + real Native authority and persistent
// budgets. Adapter JSON-RPC responses and tool decisions are synthetic; this is
// distinct from draft-correction-engine's real Core/bridge/MCP provider test.
test("production candidate chain freezes new inputs and repairs missing delivery review in the same lease", { timeout: 120000 }, async t => {
  const root = await mkdtemp(join(tmpdir(), "novel-work-decision-"));
  const authorToken = "coordinator-author-token-123456789", candidateToken = "coordinator-candidate-token-123456789";
  const suffix = process.platform === "win32" ? ".exe" : "";
  const binary = fileURLToPath(new URL(`../../native/target/debug/novel-codex-story-runtime${suffix}`, import.meta.url));
  const runtime = spawn(binary, ["--port", "0", "--db", join(root, "story.sqlite3"),
    "--engine-budget-db", join(root, "engine.sqlite3"), "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
  runtime.stderr.resume();
  let main;
  t.after(async () => {
    main?.chapterStructureConsumer.stop(); main?.adapter.close();
    if (runtime.exitCode === null && runtime.signalCode === null) {
      const stopped = new Promise(resolve => runtime.once("exit", resolve)); runtime.kill(); await stopped;
    }
    await rm(root, { recursive: true, force: true });
  });
  const origin = await new Promise((resolve, reject) => {
    const lines = createInterface({ input: runtime.stdout });
    lines.on("line", line => { if (line.startsWith("Story Runtime listening on ")) resolve(line.slice("Story Runtime listening on ".length)); });
    runtime.once("error", reject); runtime.once("exit", () => reject(new Error("Runtime exited before listening")));
    runtime.stdin.end(`${JSON.stringify({ authorToken, candidateToken })}\n`);
  });
  const request = async (path, body, method = body ? "POST" : "GET", capability = null) => {
    const response = await fetch(`${origin}${path}`, { method, headers: { "content-type": "application/json",
      authorization: `Bearer ${capability ? candidateToken : authorToken}`,
      ...(capability ? { "x-novel-task-capability": capability } : {}) },
      ...(body ? { body: JSON.stringify(body) } : {}) });
    const value = await response.json(); assert.ok(response.ok, `${method} ${path}: ${response.status} ${JSON.stringify(value)}`); return value;
  };
  const { project } = await request("/api/projects", { title: "父对话协调验收" });
  const base = `/api/projects/${project.id}`;
  const original = await request(`${base}/artifacts/brief`, { expectedRevision: 0, expectedCandidateRevisionId: null,
    idempotencyKey: "initial-brief", payload: { premise: "旧创作方向" } });
  const calls = [], created = [], turns = [], owners = [], charges = [];
  const state = { agentStorage: { budgetDb: join(root, "engine.sqlite3") }, confirmations: [], budgets: [], leases: [] };
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  main = await mainHandlers(async (path, body, options, url) => {
    const method = options.method ?? "GET";
    calls.push({ path, method, body });
    const value = await request(path + url.search, body, method, options.headers?.["x-novel-task-capability"] ?? null);
    if (path === `${base}/tasks` && body) {
      created.push(value); owners.push(await request(`${base}/tasks/${value.id}/work-task`));
    }
    if (path.endsWith("/provider-usage") && body) charges.push(value);
    return value;
  }, command, state);
  const rpc = main.adapter.request;
  let saved, outline;
  main.adapter.request = async function(method, params) {
    if (method === "turn/start") {
      const task = created.at(-1);
      const capability = this.env.NOVEL_CODEX_TASK_CAPABILITY;
      assert.equal(task.threadId, params.threadId);
      turns.push({ operation: task.operation, threadId: params.threadId, input: params.input });
      if (turns.length === 1) {
        assert.equal(task.operation, "advisory");
        await request(`${base}/tasks/${task.id}/actions`, { taskId: task.id, threadId: params.threadId,
          expectedRevision: 0, expectedIntentRevision: 0, idempotencyKey: "requested-brief",
          operation: "generateBrief", target: { type: "artifact" }, instruction: "只把创作方向改为新的港口悬疑故事，保存候选不采用" }, "POST", capability);
        await request(`${base}/tasks/${task.id}/actions`, { taskId: task.id, threadId: params.threadId,
          expectedRevision: 0, expectedIntentRevision: 0, idempotencyKey: "requested-outline",
          operation: "generateOutline", target: { type: "artifact" }, instruction: "根据新的港口悬疑方向生成大纲候选，不采用" }, "POST", capability);
      } else if (turns.length === 2) {
        assert.equal(task.operation, "generateBrief");
        assert.equal(task.inputManifest.expectedCandidateRevisionId, original.revisionId);
        saved = await request(`${base}/artifacts/brief`, { taskId: task.id, threadId: params.threadId,
          expectedRevision: 0, expectedCandidateRevisionId: original.revisionId,
          idempotencyKey: "correct-brief", payload: { premise: "新的港口悬疑故事" } }, "POST", capability);
      } else if (turns.length === 3) {
        assert.equal(task.operation, "generateOutline");
        const frozen = await request(`${base}/context?purpose=planner&budget=10000&byteBudget=60000&projectRevision=0&taskId=${task.id}&threadId=${encodeURIComponent(params.threadId)}`, null, "GET", capability);
        const source = frozen.taskSources.entries.find(item => item.sectionPath === "/narrativeGoals/brief");
        assert.ok(source, "successor must expose the saved brief in its frozen source directory");
        const page = await request(`${base}/tasks/${task.id}/task-source?threadId=${encodeURIComponent(params.threadId)}&expectedRevision=0&sourceId=${source.sourceId}`, null, "GET", capability);
        assert.equal(page.nextCursor, null);
        assert.equal(JSON.parse(page.payloadPart).sourceRevisionId, saved.revisionId);
        assert.equal(JSON.parse(page.payloadPart).premise, "新的港口悬疑故事");
        outline = await request(`${base}/artifacts/outline`, { taskId: task.id, threadId: params.threadId,
          expectedRevision: 0, expectedCandidateRevisionId: null,
          idempotencyKey: "new-outline", payload: { nodes: [{ clientKey: "book", kind: "book", position: 0, title: "港口失忆谜案", purpose: "追查失忆", summary: "港口失忆谜案大纲", dependencyKeys: [], certainty: "tentative", protection: "free" }] } }, "POST", capability);
      } else {
        assert.ok([4, 5].includes(turns.length), "one bounded protocol correction only");
        assert.equal(task.operation, "advisory");
        assert.equal(task.target.workDecision.originTaskId, created[0].id);
        const status = await request(`${base}/tasks/${task.id}/work-decision?threadId=${encodeURIComponent(task.threadId)}&expectedRevision=${task.inputRevision}`, null, "GET", capability);
        assert.equal(status.workTaskId, owners[0].id);
        assert.equal(status.rootThreadId, turns[0].threadId);
        assert.equal(status.canFinish, true);
        assert.ok(JSON.stringify(status).includes(saved.revisionId), "the saved child receipt must be available to its parent");
        if (turns.length === 4) return rpc.call(this, method, params); // natural reply, omitted complete_task
        assert.match(params.input.map(item => item.text ?? "").join(""), /尚未通过最终交付审核/);
        // The synthetic model, not the Host transport, submits its own bound
        // delivery assessment through the real Candidate capability endpoint.
        const completed = await request(`${base}/tasks/${task.id}/complete`, {
          threadId: task.threadId, expectedRevision: task.inputRevision, summary: "已保存创作方向候选，尚未正式采用。",
          deliveryReview: {
            expectedWorkVersion: status.version, expectedSourceDigest: status.sourceDigest, goalStatus: "fulfilled",
            goalAssessment: "创作方向已改为新的港口悬疑故事，候选保存符合作者范围。",
            checkedScope: "核对本轮简报候选的实际保存回执与作者要求；未进行正文或全书逻辑检查。",
            unresolved: [], authorDecisions: [], evidence: [{ taskId: created[1].id, revisionId: saved.revisionId }, { taskId: created[2].id, revisionId: outline.revisionId }],
          },
        }, "POST", capability);
        assert.equal(completed.status, "completed");
        const input = params.input.map(item => item.text ?? "").join("");
        assert.match(input, /read_work_status/);
      }
    }
    return rpc.call(this, method, params);
  };
  const result = await main.handlers.get("agent:send-prompt")(rendererEvent(), project.id,
    "请将创作方向调整为新的港口悬疑故事，保存方向和大纲候选，不采用。", null, { view: "brief" });
  const taskStates = await Promise.all(created.map(task => request(`${base}/tasks/${task.id}`)));
  assert.equal(result.executionFailure, undefined, JSON.stringify({ failure: result.executionFailure,
    turns: turns.map(turn => turn.operation), tasks: taskStates.map(task => ({ operation: task.operation, status: task.status, error: task.error })) }));
  assert.ok(!calls.some(call => call.body?.deliveryReview), "Host must never synthesize the model delivery review");
  assert.equal((await request(`${base}/tasks/${created.at(-1).id}`)).status, "completed");
  assert.deepEqual(turns.map(turn => turn.operation), ["advisory", "generateBrief", "generateOutline", "advisory", "advisory"]);
  assert.equal(turns[0].threadId, turns[3].threadId);
  assert.equal(new Set(owners.map(work => work.id)).size, 1);
  assert.equal(calls.filter(call => call.path === `${base}/tasks` && call.body.initialBudget).length, 1);
  assert.ok(!calls.some(call => call.path.endsWith("/budget-authorization") && call.method === "POST"));
  assert.equal(state.budgets.length, 4); assert.equal(state.confirmations.length, 0);
  assert.equal(result.candidateResults.length, 2); assert.equal(result.candidateResults[0].taskId, created[1].id);
  const workspace = await request(`${base}/creative-workspace`);
  assert.equal(workspace.artifacts.brief.candidate.id, saved.revisionId);
  assert.deepEqual(workspace.artifacts.brief.candidate.payload, { premise: "新的港口悬疑故事" });
  assert.equal(workspace.artifacts.brief.accepted, null); assert.equal(workspace.revision, 0);
  assert.equal(charges.length, 4); assert.equal(charges.at(-1).charges.length, 4);
  assert.deepEqual(charges.at(-1).charges.map(charge => ({ taskId: charge.taskId, requests: charge.requests,
    tokens: charge.tokens, unknownTokens: charge.unknownTokens })),
    created.map(task => ({ taskId: task.id, requests: 0, tokens: 0, unknownTokens: 0 })));
  assert.equal((await request(`${base}/tasks/${created[0].id}/final-reply`)).binding.text, "hello");
  assert.equal((await request(`${base}/tasks/${created[3].id}/final-reply`)).binding.text, "hello");
});

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
test("initial persisted worker failure reaches a Native decision and corrects in its original Work", { timeout: 120000 }, async t => {
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
    const value = await request(path + url.search, body, method);
    if (path === `${base}/tasks` && body) {
      created.push(value); owners.push(await request(`${base}/tasks/${value.id}/work-task`));
    }
    if (path.endsWith("/provider-usage") && body) charges.push(value);
    return value;
  }, command, state);
  const rpc = main.adapter.request;
  let saved;
  main.adapter.request = async function(method, params) {
    if (method === "turn/start") {
      const task = created.at(-1), capability = this.env.NOVEL_CODEX_TASK_CAPABILITY;
      turns.push({ operation: task.operation, threadId: params.threadId });
      if (turns.length === 1) {
        assert.equal(task.operation, "generateBrief");
        throw Object.assign(new Error("invalid_tool_arguments"), { code: "invalid_tool_arguments" });
      }
      if (turns.length === 2) {
        assert.equal(task.operation, "advisory");
        const status = await request(`${base}/tasks/${task.id}/work-decision?threadId=${encodeURIComponent(task.threadId)}&expectedRevision=${task.inputRevision}`, null, "GET", capability);
        const action = status.readyActions.find(item => item.operation === "generateBrief");
        assert.ok(action, JSON.stringify(status));
        await request(`${base}/tasks/${task.id}/work-decision/actions`, {
          taskId: task.id, threadId: task.threadId, expectedRevision: task.inputRevision,
          expectedWorkVersion: status.version, expectedSourceDigest: status.sourceDigest,
          actionKey: action.actionKey, instruction: "修正格式并保存新的港口悬疑创作方向", idempotencyKey: "correct-first-failure",
        }, "POST", capability);
      } else if (turns.length === 3) {
        assert.equal(task.operation, "generateBrief");
        saved = await request(`${base}/artifacts/brief`, { taskId: task.id, threadId: task.threadId,
          expectedRevision: task.inputRevision, expectedCandidateRevisionId: original.revisionId,
          idempotencyKey: "corrected-save", payload: { premise: "新的港口悬疑故事" } }, "POST", capability);
      } else {
        assert.equal(turns.length, 4);
        assert.equal(task.operation, "advisory");
        const status = await request(`${base}/tasks/${task.id}/work-decision?threadId=${encodeURIComponent(task.threadId)}&expectedRevision=${task.inputRevision}`, null, "GET", capability);
        assert.equal(status.canFinish, true);
        assert.ok(JSON.stringify(status).includes(saved.revisionId));
        // The synthetic model, not the Host transport, submits its own bound
        // delivery assessment through the real Candidate capability endpoint.
        const completed = await request(`${base}/tasks/${task.id}/complete`, {
          threadId: task.threadId, expectedRevision: task.inputRevision, summary: "已保存创作方向候选，尚未正式采用。",
          deliveryReview: {
            expectedWorkVersion: status.version, expectedSourceDigest: status.sourceDigest, goalStatus: "fulfilled",
            goalAssessment: "创作方向已改为新的港口悬疑故事，候选保存符合作者范围。",
            checkedScope: "核对本轮简报候选的实际保存回执与作者要求；未进行正文或全书逻辑检查。",
            unresolved: [], authorDecisions: [], evidence: [{ taskId: created.at(-2).id, revisionId: saved.revisionId }],
          },
        }, "POST", capability);
        assert.equal(completed.status, "completed");
      }
    }
    return rpc.call(this, method, params);
  };
  const result = await main.handlers.get("agent:generate-planning")(rendererEvent(), project.id,
    "generateBrief", "请将创作方向调整为新的港口悬疑故事，只保存不采用。");
  assert.equal(result.executionFailure, undefined, JSON.stringify(result.executionFailure));
  assert.ok(!calls.some(call => call.body?.deliveryReview), "Host must never synthesize the model delivery review");
  assert.equal((await request(`${base}/tasks/${created.at(-1).id}`)).status, "completed");
  assert.deepEqual(turns.map(turn => turn.operation), ["generateBrief", "advisory", "generateBrief", "advisory"]);
  assert.equal(turns[0].threadId, turns[1].threadId);
  assert.equal(turns[0].threadId, turns[3].threadId);
  assert.equal(new Set(owners.map(work => work.id)).size, 1);
  assert.equal(calls.filter(call => call.path === `${base}/tasks` && call.body.initialBudget).length, 1);
  assert.equal(calls.filter(call => call.path === `${base}/artifacts/brief` && call.method === "POST").length, 0,
    "Candidate writes bypass the Host Author transport and use the real capability route");
  assert.equal((await request(`${base}/tasks/${created[0].id}`)).status, "failed");
  const work = await request(`${base}/tasks/${created[0].id}/work-task`);
  assert.equal(work.status, "completed");
  assert.ok(work.steps.some(step => step.currentAttemptId === created[0].id && step.status === "superseded"));
  assert.equal((await request(`${base}/creative-workspace`)).artifacts.brief.candidate.id, saved.revisionId);
  assert.equal(charges.length, 4);
  assert.deepEqual(charges.at(-1).charges.map(charge => ({ taskId: charge.taskId, requests: charge.requests,
    tokens: charge.tokens, unknownTokens: charge.unknownTokens })),
    created.map(task => ({ taskId: task.id, requests: 0, tokens: 0, unknownTokens: 0 })));
});

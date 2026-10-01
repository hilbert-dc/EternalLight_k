import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import {
  CAPABILITIES, ModelBridge, PersistentTaskBudget, compileCodexConfig, compileCodexModelCatalog,
  createExecutionLease, defineConnection, resolveSessionPlan,
} from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { persistTaskFinalReply } from "../agent-final-reply.mjs";
import { taskProviderBudgetOptions } from "../task-provider-budget.mjs";
import { taskAuthorPrompt } from "../workflow-handoff.mjs";
import { advisoryTaskTarget, preflightRevisionRepair, REVISION_REPAIR_INSTRUCTIONS } from "../revision-repair-handlers.mjs";
import { createMcpStartupWaiter } from "./helpers/mcp-startup.mjs";

const suffix = process.platform === "win32" ? ".exe" : "";
const nativeBinary = (name) => fileURLToPath(new URL(`../../native/target/debug/${name}${suffix}`, import.meta.url));
const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/codex${suffix}`, import.meta.url));
const authorToken = "scoped-engine-author-token-123456789";
const candidateToken = "scoped-engine-candidate-token-123456";
const capability = "scoped-engine-capability-123456789";
const sourceEvidence = (body, quote) => {
  const start = Array.from(body.slice(0, body.indexOf(quote))).length;
  return { quote, start, end: start + Array.from(quote).length };
};

function sendTool(response, name, args, sequence) {
  response.writeHead(200, { "content-type": "text/event-stream" });
  response.write(`data: ${JSON.stringify({ id: `scoped-${sequence}`, choices: [{ delta: { tool_calls: [{ index: 0, id: `scoped-call-${sequence}`, function: { name, arguments: JSON.stringify(args) } }] } }] })}\n\n`);
  response.end(`data: ${JSON.stringify({ id: `scoped-${sequence}`, choices: [{ delta: {}, finish_reason: "tool_calls" }] })}\n\ndata: [DONE]\n\n`);
}

test("real Codex and Novel MCP use a finite author scope to revise, remodel, and continue through GenerationRun", { timeout: 90_000 }, async (t) => {
  const startedAt = Date.now();
  let stage = "launch Runtime";
  const phases = [];
  const enterStage = (label) => {
    stage = label;
    phases.push({ elapsedMs: Date.now() - startedAt, stage: label });
    if (phases.length > 32) phases.shift();
  };
  const root = await mkdtemp(join(tmpdir(), "novel-scoped-engine-"));
  let runtime, downstream;
  const adapters = [];
  const bridges = [];
  const budgets = new Map();
  t.after(async () => {
    t.diagnostic(`journey last stage: ${stage}; elapsed: ${Date.now() - startedAt}ms`);
    t.diagnostic(`journey recent phases: ${JSON.stringify(phases)}`);
    for (const adapter of adapters.reverse()) await adapter.close();
    for (const bridge of bridges.reverse()) await bridge.close();
    for (const budget of budgets.values()) budget.close();
    if (runtime && runtime.exitCode === null && runtime.signalCode === null) {
      const stopped = new Promise((resolve) => runtime.once("exit", resolve));
      runtime.kill(); await stopped;
    }
    if (downstream?.listening) await new Promise((resolve) => { downstream.closeAllConnections(); downstream.close(resolve); });
    await rm(root, { recursive: true, force: true });
  });
  runtime = spawn(nativeBinary("novel-codex-story-runtime"), ["--port", "0", "--db", join(root, "story.sqlite3"), "--engine-budget-db", join(root, "engine-budget.sqlite3"), "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
  const origin = await new Promise((resolve, reject) => {
    const lines = createInterface({ input: runtime.stdout });
    lines.on("line", (line) => { if (line.startsWith("Story Runtime listening on ")) resolve(line.slice("Story Runtime listening on ".length)); });
    runtime.once("error", reject);
    runtime.once("exit", () => reject(new Error("Story Runtime exited before listening")));
    runtime.stdin.end(`${JSON.stringify({ authorToken, candidateToken })}\n`);
  });
  const request = async (method, path, payload, actor = authorToken, expectedStatus = null) => {
    enterStage(`${method} ${path}`);
    const response = await fetch(`${origin}${path}`, { method, headers: {
      authorization: `Bearer ${actor}`, "content-type": "application/json",
      ...(actor === candidateToken ? { "x-novel-task-capability": capability } : {}),
    }, ...(payload === undefined ? {} : { body: JSON.stringify(payload) }) });
    const value = await response.json();
    if (expectedStatus === null) assert.ok(response.ok, `${method} ${path}: ${response.status} ${JSON.stringify(value)}`);
    else assert.equal(response.status, expectedStatus, JSON.stringify(value));
    return value;
  };
  const authorRuntimeRequest = (path, options = {}) => request(options.method ?? "GET", path,
    options.body === undefined ? undefined : JSON.parse(options.body), authorToken);
  const { project } = await request("POST", "/api/projects", { title: "港城局部改稿回环" });
  const base = `/api/projects/${project.id}`;
  const originalPayload = { chapterId: "chapter-one", chapterNumber: 1, title: "密函",
    body: "林夕走进港口。\n\n---\n\n她烧掉密函。\n\n---\n\n白塔熄灭。",
    sceneGroups: [
      { id: "opening", sceneId: "plan-opening", body: "林夕走进港口。", locked: false },
      { id: "letter", sceneId: "plan-letter", body: "她烧掉密函。", locked: false },
      { id: "ending", sceneId: null, body: "白塔熄灭。", locked: true },
    ] };
  const original = await request("POST", `${base}/drafts/checkpoints`, {
    expectedRevision: 0, expectedCandidateRevisionId: null, idempotencyKey: "original", payload: originalPayload,
  });
  const originalWorkspace = await request("GET", `${base}/creative-workspace`);
  const target = { type: "chapter", chapterId: "chapter-one", chapterNumber: 1, readingChapter: 1, sceneOrder: 1,
    scope: { type: "selectedScenes", sourceRevisionId: original.revisionId, groupIds: ["letter"] },
    desiredOutcomes: [{ id: "retain-letter", criterion: "密函仍由林夕保管，改为主动藏信", groupIds: ["letter"] }] };
  const revisedPayload = { ...originalPayload,
    body: "林夕走进港口。\n\n---\n\n她将密函藏进衣襟。\n\n---\n\n白塔熄灭。",
    sceneGroups: originalPayload.sceneGroups.map((group) => group.id === "letter" ? { ...group, body: "她将密函藏进衣襟。" } : group) };
  const repairedScene = "她将密函藏进衣襟，决定亲自留下证词。";
  const repairedPayload = { ...revisedPayload,
    body: "林夕走进港口。\n\n---\n\n" + repairedScene + "\n\n---\n\n白塔熄灭。",
    sceneGroups: revisedPayload.sceneGroups.map(group => group.id === "letter" ? { ...group, body: repairedScene } : group) };
  const repairGoals = [{ id: "testify-motive", criterion: "藏信场景明确林夕主动留下证词的动机", groupIds: ["letter"] }];
  let repairTarget;
  let repairAction;
  let frozenRepairInput;
  const nextPayload = { chapterNumber: 2, title: "未寄出的信",
    body: "林夕摸到衣襟里的密函，决定亲自为未来留下证词。",
    sceneGroups: [{ id: "opening-two", sceneId: "scene-two", body: "林夕摸到衣襟里的密函，决定亲自为未来留下证词。", locked: false }] };
  const grants = new Map();
  const taskRequest = async (operation, threadId, taskTarget, authorIntent = "只把密函场景的烧信改成藏信，保留其他场景和锁定正文", inputRevision = 0) => {
    const workspace = await request("GET", `${base}/creative-workspace`);
    const derived = ["runId", "resumeStepId", "requestedActionId", "requestedByTaskId", "sourceTaskId"].some(field => Object.hasOwn(taskTarget, field));
    const task = await request("POST", `${base}/tasks`, {
      operation, target: taskTarget, inputRevision, threadId, authorIntent,
      capabilityHash: createHash("sha256").update(capability).digest("hex"),
      capabilityExpiresAt: Math.floor(Date.now() / 1000) + 600,
      ...(!derived ? { initialBudget: {
        authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: workspace.headSnapshotId,
        expectedSourceDigest: workspace.sourceDigest, maxRequests: 16, maxTokens: 1_000_000,
        deadlineAt: Math.floor(Date.now() / 1000) + 600,
      } } : {}),
    });
    const grant = await request("GET", `${base}/tasks/${task.id}/provider-budget`);
    grants.set(task.id, grant);
    budgets.set(task.id, new PersistentTaskBudget({ databasePath: join(root, "engine-budget.sqlite3"), runId: task.id,
      ...taskProviderBudgetOptions(grant, { taskId: task.id, workTaskId: grant.workTaskId, deadlineAt: grant.deadlineAt }) }));
    return task;
  };
  const settle = async task => {
    budgets.get(task.id).complete();
    const work = grants.get(task.id).workTaskId;
    return request("POST", `${base}/work-tasks/${work}/provider-usage`, { taskId: task.id });
  };
  const sourceBatch = await request("POST", `${base}/modeling-batches`, {
    objectId: "draft-chapter-one", sourceRevisionId: original.revisionId, expectedRevision: 0, extractorVersion: "scoped-engine-test",
  });
  const oldTask = await taskRequest("modelKnowledge", "old-model", { batchId: sourceBatch.id });
  const evidence = sourceEvidence(originalPayload.body, "她烧掉密函。");
  await request("POST", `${base}/modeling-batches/${sourceBatch.id}/publish`, {
    taskId: oldTask.id, threadId: "old-model", expectedRevision: 0,
    entities: [{ id: "letter", kind: "item", displayName: "密函", aliases: [], evidence }],
    assertions: [{ kind: "assertion", subjectId: "letter", predicate: "action", value: "burned", evidence }],
  }, candidateToken);
  await request("POST", `${base}/tasks/${oldTask.id}/complete`, { threadId: "old-model", summary: "旧来源建模" }, candidateToken);
  await settle(oldTask);
  const statePath = `${base}/knowledge/state?perspective=world&entityId=letter&predicate=action`;
  const before = await request("GET", statePath);
  assert.equal(before.entries[0].assertion.value, "burned");
  assert.equal(before.coverage.complete, true);

  const active = { kind: null, binding: null, stage: 0, calls: [] };
  const providerRequests = [];
  const finalReplies = new Map();
  const rereadFinalReplies = async () => {
    for (const { task, receipt } of finalReplies.values()) {
      assert.deepEqual(await request("GET", `${base}/tasks/${task.id}/final-reply`), receipt,
        "later candidate, knowledge, budget and delivery changes must preserve the historical reply");
      const savedTask = await request("GET", `${base}/tasks/${task.id}`);
      assert.equal(savedTask.threadId, receipt.binding.threadId);
      assert.equal(savedTask.turnId, receipt.binding.turnId);
    }
  };
  const appendedText = "密函仍由林夕保管，不改锁定结尾🌙";
  let action;
  let downstreamFailure;
  downstream = createServer(async (incoming, response) => {
    try {
      const chunks = []; for await (const chunk of incoming) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      providerRequests.push(active.kind);
      if (active.kind === "discussion") {
        assert.ok(JSON.stringify(body.messages).includes(appendedText),
          "the real advisory turn must receive the delivered author intent");
      }
      const tool = (name) => body.tools?.find((entry) => entry.function?.name === name || entry.function?.name?.endsWith(`__${name}`))?.function?.name;
      if (active.stage > 0) {
        const previous = body.messages.findLast((message) => message.role === "tool");
        assert.ok(previous, `${active.kind} tool result must reach the model`);
        const output = String(previous.content);
        assert.ok(output.includes("Output:"), output.slice(0, 500));
        const value = JSON.parse(output.slice(output.indexOf("Output:") + 7).trim());
        if (active.stage === 1) {
          const context = JSON.stringify(value);
          const expectedBody = ["writer", "discussion"].includes(active.kind) ? "她烧掉密函。"
            : active.kind === "model-next" ? nextPayload.body
              : ["repair-model", "repair-outcome", "planner", "continuation", "assessment", "extraction"].includes(active.kind)
                ? repairedScene : "她将密函藏进衣襟。";
          assert.ok(context.includes(expectedBody), active.kind + " must receive its actual source body");
          if (active.kind === "planner") {
            assert.ok(context.includes(active.sourceRevisionId), "the rolling planner must read the revised source");
            assert.ok(context.includes('"hidden"'), "the rolling planner must read remodeled knowledge");
            assert.ok(!context.includes('"burned"'), "the rolling planner must not use stale knowledge");
            assert.ok(context.includes('"testify"'), "the rolling planner must use the second revision's motive knowledge");
          }
          if (active.kind === "continuation") {
            assert.ok(context.includes(active.sourceRevisionId), "the next writer must read the revised source");
            assert.ok(context.includes('"hidden"'), "the next writer must receive remodeled knowledge");
            assert.ok(!context.includes('"burned"'), "stale knowledge cannot be treated as current context");
            assert.ok(!context.includes("她烧掉密函。"), "the replaced draft must not reenter the writer context");
            assert.ok(!context.includes("她将密函藏进衣襟。"), "the first revision must not replace the author's repaired source");
            assert.ok(context.includes('"testify"'), "the next writer must receive the remodeled motive");
          }
          if (active.kind === "assessment") {
            assert.ok(context.includes(active.chapterId), "the assessment must read its bound chapter");
            assert.ok(context.includes(active.batchId), "the assessment must read the new knowledge batch");
          }
          if (active.kind === "revision-outcome") {
            assert.ok(context.includes("她烧掉密函。"), "comparison must include the frozen original");
            assert.ok(context.includes(target.desiredOutcomes[0].criterion), "assessment must receive approved goals");
            assert.ok(context.includes(active.target.writerTaskId), "assessment must bind the actual writer");
          }
          if (["repair-discussion", "repair-proposal"].includes(active.kind)) {
            const input = value.operationContext.revisionRepairInput;
            assert.equal(input.authority, "candidateAnalysis");
            assert.deepEqual(input.source, active.target.revisionRepairSource);
            assert.equal(input.authorDecision.choice, "repairDraft");
            assert.deepEqual(input.desiredOutcomes, target.desiredOutcomes);
            assert.equal(value.operationContext.inputManifest?.revisionRepairInput, undefined,
              "the mandatory pack must not be duplicated in the manifest");
          }
          if (["repair-writer", "repair-model", "repair-outcome"].includes(active.kind)) {
            assert.deepEqual(value.operationContext.revisionRepairInput, frozenRepairInput,
              "own writes must preserve historical repair evidence without reactivating the old report");
          }
          if (active.kind === "repair-outcome") {
            const input = value.operationContext.revisionOutcomeInput;
            assert.deepEqual(input.desiredOutcomes, repairGoals, "evaluate the newly approved goals, not inherited old criteria");
            assert.equal(input.beforeScenes[0].body, "她将密函藏进衣襟。");
            assert.equal(input.afterScenes[0].body, repairedScene);
          }
        } else if (active.stage === 2) {
          if (active.kind === "repair-proposal") {
            assert.equal(value.status, "proposed");
            assert.deepEqual(value.target.scope, repairTarget.scope);
            assert.deepEqual(value.target.desiredOutcomes, repairGoals);
            repairAction = value;
          } else assert.ok(value.revisionId || value.status === "ready", JSON.stringify(value));
        } else {
          assert.equal(value.deliveryStatus, "ready_for_review");
        }
      }
      const stages = ["advisory", "discussion", "repair-discussion"].includes(active.kind)
        ? [["read_context", { ...active.binding, purpose: "planner", budget: 24, byteBudget: 60000 }]]
        : active.kind === "repair-proposal"
        ? [
            ["read_context", { ...active.binding, purpose: "planner", budget: 24, byteBudget: 60000 }],
            ["request_candidate_task", { ...active.binding, expectedIntentRevision: 0, idempotencyKey: "repair-motive-proposal",
              operation: "generateChapter", target: repairTarget, instruction: "只调整密函场景，让藏信与主动留下证词的动机相连" }],
          ]
        : active.kind === "repair-outcome"
        ? [
            ["read_context", { ...active.binding, purpose: "critic", budget: 24, byteBudget: 60000 }],
            ["submit_revision_outcome", { ...active.binding, writerTaskId: active.target.writerTaskId,
              expectedCandidateRevisionId: null, idempotencyKey: "chapter-one-repair-outcome",
              payload: { writerTaskId: active.target.writerTaskId, chapterId: active.target.chapterId,
                originalRevisionId: active.target.originalRevisionId, draftRevisionId: active.target.draftRevisionId,
                knowledgeBatchId: active.target.knowledgeBatchId,
                outcomes: [{ id: "testify-motive", status: "supported", reason: "新稿明确藏信是为了亲自留下证词",
                  evidence: [{ groupId: "letter", quote: "决定亲自留下证词" }] }] } }],
            ["complete_task", { ...active.binding, summary: "再次改稿的批准目标已提交候选评估" }],
          ]
        : active.kind === "revision-outcome"
        ? [
            ["read_context", { ...active.binding, purpose: "critic", budget: 24, byteBudget: 60000 }],
            ["submit_revision_outcome", { ...active.binding, writerTaskId: active.target.writerTaskId,
              expectedCandidateRevisionId: null, idempotencyKey: "chapter-one-outcome",
              payload: { writerTaskId: active.target.writerTaskId, chapterId: active.target.chapterId,
                originalRevisionId: active.target.originalRevisionId, draftRevisionId: active.target.draftRevisionId,
                knowledgeBatchId: active.target.knowledgeBatchId,
                outcomes: [{ id: "retain-letter", status: "supported", reason: "新稿明确林夕主动藏信",
                  evidence: [{ groupId: "letter", quote: "她将密函藏进衣襟。" }] }] } }],
            ["complete_task", { ...active.binding, summary: "改稿预期已提交候选评估" }],
          ]
        : active.kind === "extraction"
        ? [
            ["read_context", { ...active.binding, purpose: "critic", budget: 24, byteBudget: 60000 }],
            ["submit_commitment_extraction", { ...active.binding, chapterId: active.chapterId,
              expectedCandidateRevisionId: null, idempotencyKey: "chapter-two-extraction",
              payload: { chapterId: active.chapterId, sourceRevisionId: active.sourceRevisionId,
                examinedRanges: [{ start: 0, end: Array.from(nextPayload.body).length }], candidates: [] } }],
            ["complete_task", { ...active.binding, summary: "第二章已逐字检查叙事承诺" }],
          ]
        : active.kind === "assessment"
          ? [
              ["read_context", { ...active.binding, purpose: "critic", budget: 24, byteBudget: 60000 }],
              ["submit_scene_assessment", { ...active.binding, chapterId: active.chapterId,
                expectedCandidateRevisionId: null, idempotencyKey: "chapter-two-assessment",
                payload: { chapterId: active.chapterId, planRevisionId: active.target.planRevisionId,
                  draftRevisionId: active.target.draftRevisionId, knowledgeBatchId: active.target.knowledgeBatchId,
                  inputSnapshotId: active.target.inputSnapshotId, intentGeneration: active.target.intentGeneration,
                  assessments: [{ sceneId: "scene-two", verdict: "realized", rationale: "正文保留了密函",
                    recommendation: "continue", effectAssessments: [{ effectId: "effect-two", verdict: "realized",
                      rationale: "第二章正文证据表明林夕继续保管密函",
                      actualEventRefs: [{ modelingBatchId: active.target.knowledgeBatchId, assertionIndex: 1 }],
                      commitmentAssessments: [] }] }] } }],
              ["complete_task", { ...active.binding, summary: "第二章场景目标已评估" }],
            ]
          : active.kind === "planner"
        ? [
            ["read_context", { ...active.binding, purpose: "planner", budget: 24, byteBudget: 60000 }],
            ["save_chapter_plans", { ...active.binding, expectedCandidateRevisionId: null, idempotencyKey: "rolling-plan-two",
              payload: { chapters: [{ chapterPlanId: "plan-two", chapterNumber: 2, goal: "循藏信线索追查密函",
                scenes: [{ id: "scene-two", goal: "林夕确认密函仍在衣襟里", beats: [{ id: "beat-two", description: "林夕决定继续藏信" }],
                  expectedEffects: [{ id: "effect-two", dimension: "knowledge", description: "密函仍由林夕保管", commitmentRefs: [] }] }] }] } }],
            ["complete_task", { ...active.binding, summary: "沿修订正文规划第二章" }],
          ]
        : ["writer", "repair-writer"].includes(active.kind)
        ? [
            ["read_context", { ...active.binding, purpose: "writer", readingChapter: 1, sceneOrder: 1, budget: 24, byteBudget: 60000 }],
            ["save_draft_checkpoint", { ...active.binding,
              expectedCandidateRevisionId: active.kind === "writer" ? original.revisionId : repairTarget.scope.sourceRevisionId,
              idempotencyKey: active.kind + "-revision", payload: active.kind === "writer" ? revisedPayload : repairedPayload }],
            ["complete_task", { ...active.binding, summary: "只修改密函场景" }],
          ]
        : active.kind === "continuation"
          ? [
              ["read_context", { ...active.binding, purpose: "writer", readingChapter: 2, sceneOrder: 1, budget: 24, byteBudget: 60000 }],
              ["save_draft_checkpoint", { ...active.binding, expectedCandidateRevisionId: null, idempotencyKey: "next-chapter",
                payload: { ...nextPayload, chapterId: active.chapterId } }],
              ["complete_task", { ...active.binding, summary: "沿藏信新稿续写第二章" }],
            ]
          : [
            ["read_context", { ...active.binding, purpose: "critic", budget: 24, byteBudget: 60000 }],
            ["publish_knowledge_batch", { ...active.binding, batchId: active.batchId, entities: [
              { id: "letter", kind: "item", displayName: "密函",
                evidence: { quote: active.kind === "model-next" ? "密函" : active.kind === "repair-model" ? repairedScene : "她将密函藏进衣襟。" } },
              ...(active.kind === "model-next" ? [
                { id: "linxi", kind: "character", displayName: "林夕", aliases: [], evidence: sourceEvidence(nextPayload.body, "林夕") },
                { id: "keep-letter", kind: "event", displayName: "继续保管密函", aliases: [],
                  evidence: sourceEvidence(nextPayload.body, "林夕摸到衣襟里的密函") },
              ] : []),
            ], assertions: [
              { kind: "assertion", subjectId: "letter", predicate: "action", value: "hidden",
                evidence: sourceEvidence(active.kind === "model-next" ? nextPayload.body : active.kind === "repair-model" ? repairedPayload.body : revisedPayload.body,
                  active.kind === "model-next" ? "决定亲自为未来留下证词" : active.kind === "repair-model" ? repairedScene : "她将密函藏进衣襟。") },
              ...(active.kind === "repair-model" ? [
                { kind: "assertion", subjectId: "letter", predicate: "motive", value: "testify",
                  evidence: sourceEvidence(repairedPayload.body, "决定亲自留下证词") },
              ] : []),
              ...(active.kind === "model-next" ? [
                { kind: "event", subjectId: "keep-letter", predicate: "retainsLetter", storyTime: 2,
                  value: { participants: [{ role: "actor", entityId: "linxi" }], preconditions: [], effects: [], untilExclusive: null },
                  evidence: sourceEvidence(nextPayload.body, "林夕摸到衣襟里的密函") },
              ] : []),
            ] }],
            ["complete_task", { ...active.binding, summary: "按新正文更新候选知识" }],
          ];
      if (active.stage === stages.length) {
        enterStage(`Codex ${active.kind}: returning final response`);
        response.writeHead(200, { "content-type": "text/event-stream" });
        response.end(`data: ${JSON.stringify({ id: `scoped-final-${active.kind}`, choices: [{ delta: { content: active.finalText }, finish_reason: "stop" }] })}\n\ndata: [DONE]\n\n`);
        return;
      }
      const [name, args] = stages[active.stage];
      const wireName = tool(name);
      assert.ok(wireName, `${active.kind} must expose ${name}`);
      active.calls.push(name);
      active.stage += 1;
      enterStage(`Codex ${active.kind}: awaiting ${name} result`);
      sendTool(response, wireName, args, `${active.kind}-${active.stage}`);
    } catch (error) {
      downstreamFailure = error;
      response.writeHead(500, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: error.message } }));
    }
  });
  await new Promise((resolve, reject) => { downstream.once("error", reject); downstream.listen(0, "127.0.0.1", resolve); });
  const connection = defineConnection({ id: "scoped-model", provider: "fixture", endpoint: `http://127.0.0.1:${downstream.address().port}`,
    model: "fixture-scoped", credentialRef: "FIXTURE_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
  const plan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection, { protocol: connection.protocol,
    capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING] }),
    policy: { defaultConnectionId: connection.id, allowedMcpServerIds: ["novel_workspace"] } });
  async function runTask(kind, operation, taskTarget, batchId, authorIntent, sourceRevisionId) {
    const bridge = new ModelBridge({ sessionPlan: plan, connections: [connection], credentialResolver: async () => "fixture-key", token: `scoped-${kind}-bridge` });
    bridges.push(bridge);
    enterStage(`Codex ${kind}: starting model bridge`);
    const address = await bridge.start();
    const home = join(root, `codex-${kind}`);
    await mkdir(join(home, "sqlite"), { recursive: true, mode: 0o700 });
    const catalog = join(home, "catalog.json");
    await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(plan, { baseInstructions: "只调用授权的小说工作区工具。" })), { mode: 0o600 });
    await writeFile(join(home, "config.toml"), compileCodexConfig({ sessionPlan: plan, modelCatalogPath: catalog,
      bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
      mcpServers: [{ id: "novel_workspace", command: nativeBinary("novel-workspace-mcp"), args: [],
        envVars: ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"], approvalMode: "approve" }],
    }), { mode: 0o600 });
    const adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: root,
      env: { PATH: process.env.PATH, CODEX_HOME: home, CODEX_SQLITE_HOME: join(home, "sqlite"),
        AGENT_ENGINE_BRIDGE_TOKEN: address.token, NOVEL_CODEX_RUNTIME_URL: origin,
        NOVEL_CODEX_CANDIDATE_TOKEN: candidateToken, NOVEL_CODEX_TASK_CAPABILITY: capability, NOVEL_CODEX_OPERATION: operation },
      model: connection.id, provider: connection.provider });
    adapters.push(adapter);
    let startup;
    const completedMessages = [];
    adapter.setCallbacks({ onEvent: (event) => {
      startup?.onEvent(event);
      if (event.method === "item/completed" && event.params?.item?.type === "agentMessage") {
        completedMessages.push(event.params);
      }
      if (event.method === "mcpServer/startupStatus/updated" && event.params?.name === "novel_workspace") {
        const status = event.params?.status;
        if (["ready", "failed", "starting"].includes(status)) enterStage(`Codex ${kind}: MCP ${status}`);
      }
    } });
    adapter.startProcess();
    startup = createMcpStartupWaiter({ serverName: "novel_workspace", process: adapter.process, timeoutMs: 30_000 });
    t.after(startup.dispose);
    enterStage(`Codex ${kind}: starting isolated session`);
    await adapter.ensureSession();
    enterStage(`Codex ${kind}: waiting for MCP readiness`);
    await startup.ready;
    const workspaceRevision = (await request("GET", `${base}/creative-workspace`)).revision;
    const task = await taskRequest(operation, adapter.threadId, taskTarget, authorIntent, workspaceRevision);
    if (kind === "discussion") {
      const intentPath = `${base}/tasks/${task.id}/intents`;
      const pendingIntent = await request("POST", intentPath, { expectedIntentRevision: 0, authorText: appendedText });
      assert.notEqual(pendingIntent.authorMessageId, task.inputManifest.authorMessageId);
      const deliveredIntent = await request("POST", `${intentPath}/1/delivered`, {});
      assert.equal(deliveredIntent.authorMessageId, pendingIntent.authorMessageId);
      assert.deepEqual((await request("GET", intentPath)).intents, [deliveredIntent]);
    }
    if (taskTarget?.runId) {
      const runSource = await request("GET", `${base}/generation-runs/${taskTarget.runId}`);
      assert.equal(task.inputManifest.authorMessageId, runSource.authorMessageId);
      assert.equal(task.inputManifest.generationAuthorIntentBinding.runId, runSource.id);
      assert.equal(task.inputManifest.generationAuthorIntentBinding.intentGeneration, taskTarget.intentGeneration);
      assert.equal(task.inputManifest.generationAuthorIntentBinding.chapterNumber, 2);
      assert.equal(task.inputManifest.authorIntent, `[要求版本 0] ${runSource.intentRevisions[0].instruction}`);
    }
    const budget = budgets.get(task.id);
    bridge.beginTaskBudget(budget);
    bridge.beginExecutionLease(createExecutionLease({ sessionPlan: plan, runId: task.id, budgetId: budget.id }));
    active.kind = kind;
    active.binding = { taskId: task.id, projectId: project.id, threadId: adapter.threadId, expectedRevision: workspaceRevision };
    active.batchId = batchId;
    active.sourceRevisionId = sourceRevisionId;
    active.chapterId = taskTarget?.chapterId;
    active.target = taskTarget;
    active.stage = 0;
    active.calls = [];
    active.finalText = `## ${kind} 最终回复\n\n候选改稿和来源知识已处理。\n保留密函与锁定结尾🌙`;
    const instruction = kind === "discussion" ? "分析密函场景的局部修改，遵守作者追加要求。"
      : kind === "advisory" ? "分析改稿和当前来源知识。"
      : ["repair-discussion", "repair-proposal"].includes(kind) ? REVISION_REPAIR_INSTRUCTIONS
      : operation === "generateChapter" && kind !== "continuation" ? "只改密函场景，保留其余正文；严格按新批准目标执行。"
      : operation === "assessRevisionOutcome" ? "按本次批准目标比较限定场景原稿与新稿，提交有原文证据的候选判断。"
      : kind === "planner" ? "沿新稿与当前知识规划第二章。"
        : kind === "continuation" ? "沿新稿与当前知识续写第二章。" : "按当前稿建模。";
    let result;
    enterStage(`Codex ${kind}: waiting for model turn`);
    try { result = await adapter.sendTurn([{ type: "text", text: `${taskAuthorPrompt(task)}\n${instruction}${kind === "discussion" ? `\n作者追加要求：\n${appendedText}` : ""}` }], { disableEnvironment: true }); }
    catch (error) { if (downstreamFailure) throw new Error(`controlled ${kind} model failed: ${downstreamFailure.stack}`, { cause: error }); throw error; }
    assert.ifError(downstreamFailure);
    enterStage(`Codex ${kind}: validating turn delivery`);
    assert.equal(result.turn.status, "completed");
    assert.equal(result.message, active.finalText);
    assert.ok(result.finalReply?.itemId, `${kind} must expose an actual completed public final message`);
    const publicMessage = completedMessages.find(({ threadId, turnId, item }) => threadId === task.threadId
      && turnId === result.turn.id && item.id === result.finalReply.itemId
      && [null, undefined, "final_answer"].includes(item.phase));
    assert.ok(publicMessage, `${kind} reply must originate from this turn's public item/completed event`);
    assert.equal(publicMessage.item.text, active.finalText);
    assert.deepEqual(result.finalReply, { itemId: publicMessage.item.id, text: active.finalText,
      truncated: false, phase: publicMessage.item.phase ?? null });
    const outputTool = operation === "modelKnowledge" ? "publish_knowledge_batch" : kind === "planner" ? "save_chapter_plans"
      : operation === "assessRevisionOutcome" ? "submit_revision_outcome"
      : kind === "extraction" ? "submit_commitment_extraction" : kind === "assessment" ? "submit_scene_assessment"
        : "save_draft_checkpoint";
    assert.deepEqual(active.calls, kind === "repair-proposal" ? ["read_context", "request_candidate_task"]
      : operation === "advisory" ? ["read_context"] : ["read_context", outputTool, "complete_task"]);
    if (kind === "discussion") {
      // Keep the journey's existing scoped-action fixture; the reply itself
      // must come from the real read-only advisory turn above.
      action = await request("POST", `${base}/tasks/${task.id}/actions`, {
        taskId: task.id, threadId: task.threadId, expectedRevision: workspaceRevision, expectedIntentRevision: 1,
        idempotencyKey: "selected-letter", operation: "generateChapter", target, instruction: "只把密函场景改为藏信",
      }, candidateToken);
    }
    const providerRequestCount = providerRequests.length;
    const taskPath = `${base}/tasks/${task.id}`;
    assert.equal(await request("GET", `${taskPath}/final-reply`), null,
      "model completion and structural artifacts cannot manufacture a host final-reply receipt");
    const boundTask = await request("POST", `${taskPath}/turn`, { threadId: task.threadId, turnId: result.turn.id });
    assert.equal(boundTask.turnId, result.turn.id);
    const receipt = await persistTaskFinalReply(authorRuntimeRequest, project.id, task, result);
    assert.deepEqual(receipt.binding, { threadId: task.threadId, turnId: result.turn.id,
      intentRevision: kind === "discussion" ? 1 : 0, itemId: result.finalReply.itemId,
      text: active.finalText, truncated: false });
    assert.deepEqual({ taskId: receipt.taskId, projectId: receipt.projectId, state: receipt.state },
      { taskId: task.id, projectId: project.id, state: "recorded" });
    assert.match(receipt.sourceDigest, /^[a-f0-9]{64}$/);
    assert.match(receipt.receiptDigest, /^[a-f0-9]{64}$/);
    assert.ok(Number.isSafeInteger(receipt.recordedAt) && receipt.recordedAt > 0);
    assert.deepEqual(await request("GET", `${taskPath}/final-reply`), receipt);
    assert.deepEqual(await persistTaskFinalReply(authorRuntimeRequest, project.id, task, result), receipt,
      "exact host replay must return the same durable receipt without another provider request");
    assert.equal(providerRequests.length, providerRequestCount, "binding and receipt replay cannot incur model usage");
    finalReplies.set(kind, { task, receipt });
    // Advisory is read-only: completion is the trusted host's responsibility,
    // just as in production runAdvisoryAgentTask, not an extra model tool grant.
    if (operation === "advisory") await request("POST", `${base}/tasks/${task.id}/complete`, { threadId: adapter.threadId, summary: result.message });
    if (taskTarget?.runId) {
      budget.complete();
      const runPath = `${base}/generation-runs/${taskTarget.runId}`;
      const charged = await request("POST", `${runPath}/provider-usage`, { taskId: task.id });
      await request("POST", `${runPath}/advance-task`, { expectedVersion: charged.version, taskId: task.id });
    } else {
      await settle(task);
    }
    return task;
  }
  const discussion = await runTask("discussion", "advisory", { type: "project" });
  const authorHistory = `${discussion.inputManifest.authorIntent}\n\n作者追加要求：\n${appendedText}`;
  const scopePath = `${base}/tasks/${discussion.id}/execution-scope`;
  const scopeRequest = { actionIds: [action.id], maxActions: 1, maxTaskAttempts: 8, maxAdvisoryRounds: 1, expectedIntentRevision: 1 };
  const preview = await request("POST", `${scopePath}/preview`, scopeRequest);
  const confirmation = { request: scopeRequest, previewDigest: preview.previewDigest };
  const permit = await request("POST", scopePath, confirmation);
  assert.deepEqual(await request("POST", scopePath, confirmation), permit);
  const permission = await request("GET", `${base}/tasks/${discussion.id}/actions/${action.id}/permission`);
  assert.equal(permission.permission.kind, "finiteScope");
  assert.equal(permission.permission.scopePermitId, permit.id);
  assert.equal(permission.permission.evidence.authorMessageId, discussion.inputManifest.authorMessageId);
  assert.equal(permission.permission.evidence.authorIntent, authorHistory);
  const writer = await runTask("writer", "generateChapter", { ...target, requestedByTaskId: discussion.id, requestedActionId: action.id });
  assert.equal(writer.inputManifest.authorIntent, authorHistory);
  assert.equal(grants.get(writer.id).workTaskId, preview.workTaskId, "scope execution must consume the original budget owner");
  const changed = await request("GET", `${base}/creative-workspace`);
  assert.equal(changed.revision, originalWorkspace.revision, "candidate edits do not advance the formal project revision");
  assert.notEqual(changed.headSnapshotId, originalWorkspace.headSnapshotId);
  assert.notEqual(changed.sourceDigest, originalWorkspace.sourceDigest);
  const draft = changed.chapters[0].candidate;
  assert.equal(draft.body, revisedPayload.body);
  assert.deepEqual(draft.sceneGroups.filter((group) => group.id !== "letter"), originalPayload.sceneGroups.filter((group) => group.id !== "letter"));
  assert.equal(changed.chapters[0].accepted, null);
  const invalidated = await request("GET", statePath);
  assert.deepEqual(invalidated.entries, []);
  assert.equal(invalidated.coverage.complete, false);
  await rereadFinalReplies();
  const followup = await request("GET", `${base}/tasks/${writer.id}/next-step`);
  assert.deepEqual(followup.action, { kind: "modelKnowledge", chapterId: "chapter-one", sourceRevisionId: draft.id, sourceTaskId: writer.id });
  const batch = await request("POST", `${base}/modeling-batches`, {
    objectId: `draft-${followup.action.chapterId}`, sourceRevisionId: followup.action.sourceRevisionId,
    expectedRevision: changed.revision, extractorVersion: "scoped-engine-test",
  });
  await runTask("model", "modelKnowledge", { batchId: batch.id, sourceTaskId: followup.action.sourceTaskId }, batch.id);
  assert.deepEqual((await request("GET", `${base}/tasks/${writer.id}/next-step`)).action, null);
  const updated = await request("GET", statePath);
  assert.equal(updated.entries.length, 1);
  assert.equal(updated.entries[0].sourceRevisionId, draft.id);
  assert.equal(updated.entries[0].assertion.value, "hidden");
  assert.equal(updated.coverage.complete, true);
  const workPath = `${base}/work-tasks/${preview.workTaskId}`;
  const outcomeStep = (await request("GET", workPath)).steps.find(step => step.operation === "assessRevisionOutcome" && step.status === "queued");
  assert.ok(outcomeStep, "model completion must atomically queue the approved outcome assessment");
  assert.equal(outcomeStep.target.writerTaskId, writer.id);
  const outcomeTask = await runTask("revision-outcome", "assessRevisionOutcome", { ...outcomeStep.target, resumeStepId: outcomeStep.id });
  assert.equal(grants.get(outcomeTask.id).workTaskId, preview.workTaskId);
  const outcome = await request("GET", `${base}/revision-outcomes/${writer.id}`);
  assert.equal(outcome.authority, "candidateAnalysis");
  assert.equal(outcome.state, "current");
  assert.deepEqual(outcome.assessment.outcomes, [{ id: "retain-letter", status: "supported", reason: "新稿明确林夕主动藏信",
    evidence: [{ groupId: "letter", ...sourceEvidence("她将密函藏进衣襟。", "她将密函藏进衣襟。") }] }]);
  const continuationWork = await request("POST", `${workPath}/execution-scope/continue`);
  const analysisStep = continuationWork.steps.find(step => step.operation === "advisory" && step.status === "queued");
  assert.ok(analysisStep, "settled scoped outputs must enable their authorized followup analysis");
  assert.deepEqual(await request("POST", `${workPath}/execution-scope/continue`), continuationWork);
  const analysis = await runTask("advisory", "advisory", { ...analysisStep.target, resumeStepId: analysisStep.id }, undefined, "客户端不能替换原作者要求");
  assert.equal(analysis.inputManifest.authorIntent, authorHistory);
  assert.equal(grants.get(analysis.id).workTaskId, preview.workTaskId);
  assert.ok(grants.get(analysis.id).maxRequests < grants.get(writer.id).maxRequests, "followup consumes remaining requests, not a fresh allowance");
  const finishedWork = await request("GET", workPath);
  assert.equal(finishedWork.status, "completed");
  assert.equal(finishedWork.deliveryStatus, "ready_for_review");
  assert.deepEqual(finishedWork.deliveryReport.pendingActionIds, []);
  assert.deepEqual(finishedWork.deliveryReport.unsettledAttemptIds, []);
  assert.ok(finishedWork.deliveryReport.evidence.some(item => item.kind === "knowledgeBatch" && item.state === "recorded"));
  assert.ok(finishedWork.deliveryReport.evidence.some(item => item.kind === "draft" && item.referenceId === draft.id && item.state === "recorded"));
  assert.deepEqual(await request("POST", `${workPath}/execution-scope/continue`), finishedWork, "advisory round allowance cannot be replenished by replay");
  assert.ok(finishedWork.deliveryReport.evidence.some(item => item.kind === "revisionOutcome" && item.state === "recorded"));
  assert.equal(providerRequests.length, 16, "assessment must consume the original author-confirmed allowance");
  await rereadFinalReplies();
  const readyWorkspace = await request("GET", `${base}/creative-workspace`);
  const repairDecision = await request("POST", `${base}/revision-outcomes/${writer.id}/decisions`, {
    decisionId: `repair-decision-${randomUUID()}`, expectedAssessmentRevisionId: outcome.revisionId,
    expectedDecisionId: null, choice: "repairDraft", note: "请先讨论动机，不自动改稿",
  });
  const repairSource = { writerTaskId: writer.id, assessmentRevisionId: outcome.revisionId, decisionId: repairDecision.id };
  await preflightRevisionRepair({ runtimeRequest: authorRuntimeRequest, projectId: project.id, source: repairSource });
  const repair = await runTask("repair-discussion", "advisory", advisoryTaskTarget({ revisionRepairSource: repairSource }),
    undefined, "先只分析藏信后的动机问题，不要改稿。");
  assert.notEqual(grants.get(repair.id).workTaskId, preview.workTaskId);
  assert.notEqual(repair.inputManifest.authorMessageId, discussion.inputManifest.authorMessageId);
  assert.equal(repair.inputManifest.authorIntent, "先只分析藏信后的动机问题，不要改稿。");
  assert.deepEqual((await request("GET", `${base}/tasks/${repair.id}/actions`)).actions, []);
  const afterRepair = await request("GET", `${base}/creative-workspace`);
  assert.deepEqual(afterRepair.chapters, readyWorkspace.chapters, "repair discussion alone must not write fiction");
  assert.equal(afterRepair.sourceDigest, readyWorkspace.sourceDigest);
  assert.equal((await request("GET", `${base}/revision-outcomes/${writer.id}`)).state, "current");
  repairTarget = { ...target, scope: { ...target.scope, sourceRevisionId: draft.id }, desiredOutcomes: repairGoals };
  const proposal = await runTask("repair-proposal", "advisory", advisoryTaskTarget({ revisionRepairSource: repairSource }),
    undefined, "先分析藏信的动机，并提出只修改密函场景的候选方案；未经批准不要改稿。");
  frozenRepairInput = proposal.inputManifest.revisionRepairInput;
  const repairWorkPath = `${base}/work-tasks/${grants.get(proposal.id).workTaskId}`;
  assert.notEqual(grants.get(proposal.id).workTaskId, preview.workTaskId);
  assert.notEqual(grants.get(proposal.id).workTaskId, grants.get(repair.id).workTaskId);
  const proposalWork = await request("GET", repairWorkPath);
  assert.deepEqual(proposalWork.deliveryReport.pendingActionIds, [repairAction.id]);
  assert.deepEqual((await request("GET", `${base}/creative-workspace`)).chapters, readyWorkspace.chapters);
  const repairActionPath = `${base}/tasks/${proposal.id}/actions/${repairAction.id}`;
  assert.deepEqual(await request("GET", `${repairActionPath}/permission`), { permission: null });
  await request("POST", `${repairActionPath}/approve`, { expectedIntentRevision: 0 }, candidateToken, 401);
  const repairClaim = { ...repairAction.target, requestedByTaskId: proposal.id, requestedActionId: repairAction.id };
  await request("POST", `${base}/tasks`, { operation: "generateChapter", target: repairClaim,
    inputRevision: readyWorkspace.revision, threadId: "unauthorized-repair",
    authorIntent: proposal.inputManifest.authorIntent,
    capabilityHash: createHash("sha256").update(capability).digest("hex"),
    capabilityExpiresAt: Math.floor(Date.now() / 1000) + 600 }, authorToken, 400);
  assert.deepEqual(await request("GET", repairWorkPath), proposalWork, "rejected approval/claim cannot consume attempts or budget");
  const approval = await request("POST", `${repairActionPath}/approve`, { expectedIntentRevision: 0 });
  assert.equal(approval.status, "queued");
  const repairPermission = await request("GET", `${repairActionPath}/permission`);
  assert.equal(repairPermission.permission.kind, "exactAction");
  assert.deepEqual(repairPermission.permission.evidence.target, repairAction.target);
  const repairedWriter = await runTask("repair-writer", "generateChapter", repairClaim);
  assert.equal(repairedWriter.inputManifest.authorIntent, proposal.inputManifest.authorIntent);
  assert.equal(grants.get(repairedWriter.id).workTaskId, grants.get(proposal.id).workTaskId);
  assert.ok(grants.get(repairedWriter.id).maxRequests < grants.get(proposal.id).maxRequests);
  const repairedWorkspace = await request("GET", `${base}/creative-workspace`);
  const repairedDraft = repairedWorkspace.chapters[0].candidate;
  assert.notEqual(repairedDraft.id, draft.id);
  assert.equal(repairedDraft.body, repairedPayload.body);
  assert.deepEqual(repairedDraft.sceneGroups.filter(group => group.id !== "letter"),
    originalPayload.sceneGroups.filter(group => group.id !== "letter"));
  assert.equal(repairedWorkspace.chapters[0].accepted, null);
  const staleOutcome = await request("GET", `${base}/revision-outcomes/${writer.id}`);
  assert.equal(staleOutcome.state, "stale");
  assert.deepEqual(staleOutcome.assessment, outcome.assessment, "historical report cannot be rewritten by later repair");
  assert.deepEqual(staleOutcome.decisions, [repairDecision]);
  await assert.rejects(preflightRevisionRepair({ runtimeRequest: authorRuntimeRequest, projectId: project.id, source: repairSource }));
  const repairNext = await request("GET", `${base}/tasks/${repairedWriter.id}/next-step`);
  assert.deepEqual(repairNext.action, { kind: "modelKnowledge", chapterId: "chapter-one",
    sourceRevisionId: repairedDraft.id, sourceTaskId: repairedWriter.id });
  const repairBatch = await request("POST", `${base}/modeling-batches`, {
    objectId: "draft-chapter-one", sourceRevisionId: repairedDraft.id,
    expectedRevision: repairedWorkspace.revision, extractorVersion: "scoped-repair-engine-test",
  });
  const repairModel = await runTask("repair-model", "modelKnowledge",
    { batchId: repairBatch.id, sourceTaskId: repairedWriter.id }, repairBatch.id);
  const repairOutcomeStep = (await request("GET", repairWorkPath)).steps
    .find(step => step.operation === "assessRevisionOutcome" && step.status === "queued");
  assert.ok(repairOutcomeStep, "own draft staleness must not block the repair's next assessment");
  const repairAssessment = await runTask("repair-outcome", "assessRevisionOutcome",
    { ...repairOutcomeStep.target, resumeStepId: repairOutcomeStep.id });
  for (const task of [repairModel, repairAssessment]) {
    assert.equal(grants.get(task.id).workTaskId, grants.get(proposal.id).workTaskId);
    assert.equal(grants.get(task.id).deadlineAt, grants.get(proposal.id).deadlineAt);
  }
  const repairedOutcome = await request("GET", `${base}/revision-outcomes/${repairedWriter.id}`);
  assert.equal(repairedOutcome.state, "current");
  assert.equal(repairedOutcome.assessment.originalRevisionId, draft.id);
  assert.equal(repairedOutcome.assessment.draftRevisionId, repairedDraft.id);
  assert.equal(repairedOutcome.assessment.knowledgeBatchId, repairBatch.id);
  assert.equal(repairedOutcome.assessment.outcomes[0].id, repairGoals[0].id);
  const repairedWork = await request("GET", repairWorkPath);
  assert.equal(repairedWork.deliveryStatus, "ready_for_review");
  assert.deepEqual(repairedWork.deliveryReport.pendingActionIds, []);
  assert.deepEqual(repairedWork.deliveryReport.unsettledAttemptIds, []);
  assert.equal(providerRequests.filter(kind => ["repair-proposal", "repair-writer", "repair-model", "repair-outcome"].includes(kind)).length,
    15, "the entire second repair must stay within its one 16-request root allowance");
  await rereadFinalReplies();
  const decisionRequest = { decisionId: `revision-decision-${randomUUID()}`, expectedAssessmentRevisionId: repairedOutcome.revisionId,
    expectedDecisionId: null, choice: "acceptRevision", note: "沿明确证词动机的新稿继续，不等于确认 Canon" };
  const decisionPath = `${base}/revision-outcomes/${repairedWriter.id}/decisions`;
  const decision = await request("POST", decisionPath, decisionRequest);
  assert.deepEqual(await request("POST", decisionPath, decisionRequest), decision);
  const revisionPredecessor = { writerTaskId: repairedWriter.id, assessmentRevisionId: repairedOutcome.revisionId, decisionId: decision.id };
  const runRequest = {
    expectedProjectRevision: repairedWorkspace.revision, expectedBaseSnapshotId: repairedWorkspace.headSnapshotId,
    expectedSourceDigest: repairedWorkspace.sourceDigest,
    authorIntent: "沿藏信和主动留下证词的新稿继续写第二章，不恢复烧信设定",
    revisionPredecessor,
    targetChapters: 1, maxTaskAttempts: 8, maxProviderRequests: 24, maxProviderTokens: 1_000_000,
    deadlineAt: Math.floor(Date.now() / 1000) + 600,
  };
  await request("POST", `${base}/generation-runs`, runRequest, authorToken, 409);
  const continuationSource = await request("GET", `${base}/creative-workspace`);
  assert.equal(continuationSource.chapters[0].candidate.id, repairedDraft.id);
  assert.notEqual(continuationSource.sourceDigest, repairedWorkspace.sourceDigest,
    "knowledge publication must invalidate the pre-modeling continuation source");
  const run = await request("POST", `${base}/generation-runs`, { ...runRequest,
    expectedProjectRevision: continuationSource.revision, expectedBaseSnapshotId: continuationSource.headSnapshotId,
    expectedSourceDigest: continuationSource.sourceDigest,
  });
  assert.equal(run.startChapterNumber, 2);
  assert.deepEqual(run.revisionPredecessor, revisionPredecessor);
  assert.match(run.authorMessageId, /^author-message-[a-f0-9]{32}$/);
  assert.notEqual(run.authorMessageId, discussion.inputManifest.authorMessageId,
    "a provenance edge must not inherit the previous author's execution authority");
  const runPath = `${base}/generation-runs/${run.id}`;
  const planning = await request("POST", `${runPath}/next`, {});
  assert.equal(planning.action.operation, "generatePlan");
  assert.equal(planning.action.target.planFromChapter, 2);
  await runTask("planner", "generatePlan", planning.action.target, null, "客户端不可替换：恢复烧信设定", repairedDraft.id);
  const materialization = await request("POST", `${runPath}/next`, {});
  assert.equal(materialization.action.kind, "materializeChapter");
  await request("POST", `${runPath}/materialize`, { expectedVersion: materialization.run.version });
  const writing = await request("POST", `${runPath}/next`, {});
  assert.equal(writing.action.operation, "generateChapter");
  assert.equal(writing.action.target.chapterNumber, 2);
  const beforeContinuation = await request("GET", `${base}/creative-workspace`);
  assert.equal(beforeContinuation.chapters[1].candidate, null, "materialization must not insert a placeholder draft");
  const next = await runTask("continuation", "generateChapter", writing.action.target,
    null, writing.action.authorIntent, repairedDraft.id);
  const workspace = await request("GET", `${base}/creative-workspace`);
  assert.equal(workspace.chapters[0].candidate.id, repairedDraft.id);
  assert.equal(workspace.chapters[0].candidate.body, repairedPayload.body);
  assert.equal(workspace.chapters[1].candidate.body, nextPayload.body);
  assert.equal(workspace.chapters[1].accepted, null);
  assert.equal((await request("GET", `${base}/tasks/${next.id}/next-step`)).action, null,
    "the GenerationRun owns the next model task rather than a free-standing task handoff");
  const modeling = await request("POST", `${runPath}/next`, {});
  assert.equal(modeling.action.operation, "modelKnowledge");
  assert.equal(modeling.action.target.chapterId, writing.action.target.chapterId);
  const nextBatch = await request("POST", `${base}/modeling-batches`, {
    objectId: `draft-${modeling.action.target.chapterId}`,
    sourceRevisionId: workspace.chapters[1].candidate.id,
    expectedRevision: workspace.revision, extractorVersion: "scoped-engine-test",
  });
  await runTask("model-next", "modelKnowledge", { ...modeling.action.target, batchId: nextBatch.id },
    nextBatch.id, modeling.action.authorIntent);
  const nextState = await request("GET", statePath);
  assert.ok(nextState.entries.some(entry => entry.sourceRevisionId === workspace.chapters[1].candidate.id
    && entry.assertion.value === "hidden"), "the second chapter must produce its own current knowledge");
  assert.ok(nextState.entries.every(entry => entry.assertion.value !== "burned"));
  assert.equal(nextState.coverage.complete, true);
  const extraction = await request("POST", `${runPath}/next`, {});
  assert.equal(extraction.action.operation, "extractCommitments");
  assert.equal(extraction.action.target.sourceRevisionId, workspace.chapters[1].candidate.id);
  await runTask("extraction", "extractCommitments", extraction.action.target,
    null, extraction.action.authorIntent, workspace.chapters[1].candidate.id);
  const assessment = await request("POST", `${runPath}/next`, {});
  assert.equal(assessment.action.operation, "assessSceneOutcome");
  assert.equal(assessment.action.target.knowledgeBatchId, nextBatch.id);
  await runTask("assessment", "assessSceneOutcome", assessment.action.target,
    nextBatch.id, assessment.action.authorIntent);
  const check = await request("POST", `${runPath}/next`, {});
  assert.equal(check.action.kind, "checkKnowledge");
  const completed = await request("POST", `${runPath}/check`, { expectedVersion: check.run.version });
  assert.equal(completed.status, "completed");
  assert.equal(completed.completedChapters, 2);
  assert.equal(completed.chapterDeliveries.length, 1);
  const delivery = completed.chapterDeliveries[0];
  assert.equal(delivery.chapterId, writing.action.target.chapterId);
  assert.equal(delivery.chapterNumber, 2);
  assert.equal(delivery.draftRevisionId, workspace.chapters[1].candidate.id);
  assert.equal(delivery.knowledgeBatchId, nextBatch.id);
  assert.equal(delivery.checkedRunVersion, check.run.version);
  assert.equal(delivery.sourceDigest, check.run.sourceDigest);
  assert.equal(delivery.feedbackDecisionId, null);
  assert.deepEqual((await request("GET", runPath)).chapterDeliveries, completed.chapterDeliveries);
  const generationWork = await request("GET", `${base}/work-tasks/work-${run.id}`);
  assert.equal(generationWork.deliveryStatus, "ready_for_review");
  assert.deepEqual(generationWork.deliveryReport.unsettledAttemptIds, []);
  assert.deepEqual([...finalReplies.keys()], ["discussion", "writer", "model", "revision-outcome", "advisory", "repair-discussion",
    "repair-proposal", "repair-writer", "repair-model", "repair-outcome", "planner",
    "continuation", "model-next", "extraction", "assessment"]);
  await rereadFinalReplies();
  assert.equal(providerRequests.length, 53, "historical receipt reads must not add provider usage");
});

import { nativeTestBinary } from "./helpers/native-binaries.mjs";
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
import { currentFixtureEvidence, updateFixtureDeliveryPlan } from "./fixtures/model-plan-current-contract.mjs";
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
import { NOVEL_TOOL_OUTPUT_POLICY } from "../codex-runtime.mjs";

const suffix = process.platform === "win32" ? ".exe" : "";
const nativeBinary = (name) => nativeTestBinary(name);
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
  response.end(`data: ${JSON.stringify({ id: `scoped-${sequence}`, choices: [{ delta: {}, finish_reason: "tool_calls" }], usage: { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 } })}\n\ndata: [DONE]\n\n`);
}

test("real Codex and Novel MCP use a finite author scope to revise, remodel, and continue through GenerationRun", { timeout: 180_000 }, async (t) => {
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
    const raw = await response.text();
    assert.match(response.headers.get("content-type") ?? "", /^application\/json\b/, `${method} ${path}: HTTP ${response.status}: ${raw.slice(0, 2000)}`);
    const value = JSON.parse(raw);
    if (expectedStatus === null) assert.ok(response.ok, `${method} ${path}: ${response.status} ${JSON.stringify(value)}`);
    else assert.equal(response.status, expectedStatus, JSON.stringify(value));
    return value;
  };
  const authorRuntimeRequest = (path, options = {}) => request(options.method ?? "GET", path,
    options.body === undefined ? undefined : JSON.parse(options.body), authorToken);
  const { project } = await request("POST", "/api/projects", { title: "港城局部改稿回环", logline: "实际 Core 和 MCP 的限定改稿与续章验收", medium: "novel", idempotencyKey: randomUUID() });
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
    chapterAcceptance: { requirements: [{ id: "letter-action", text: "林夕主动藏信，密函仍由林夕保管" }, { id: "protected-scenes", text: "其余场景与锁定结尾保持原文" }],
      length: { requirement: "本次局部修改无篇幅限制", minCharacters: null, maxCharacters: null } },
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
        expectedSourceDigest: workspace.sourceDigest, maxRequests: 256, maxTokens: 1_000_000,
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
      // The decision model reads the frozen author request and actual bound
      // current prose/report before chapter closure or final delivery. A status
      // summary, source catalogue or host-side assertion cannot stand in for it.
      const readingDecisionSource = active.decisionReadPhase !== null && !active.decisionReadDone;
      const boundResult = () => {
        const previous = body.messages.findLast(message => message.role === "tool");
        const output = String(previous?.content);
        assert.ok(output.includes("Output:"), `${active.kind} bound read result must reach the model`);
        return JSON.parse(output.slice(output.indexOf("Output:") + 7).trim());
      };
      const evidenceTool = (name, args, phase) => {
        active.decisionReadPhase = phase;
        const wireName = tool(name);
        assert.ok(wireName, `${active.kind} must expose ${name}`);
        sendTool(response, wireName, { ...active.binding, ...args }, `${active.kind}-evidence-${++active.evidenceSequence}`);
      };
      if (readingDecisionSource) {
        const value = boundResult();
        if (active.decisionReadPhase === "context") {
          active.decisionReadQueue = value.taskSources.entries.filter(entry => entry.sectionPath === "/authorRequirementsSource"
            || (active.chooseOperation === "finishChapter" && entry.sectionPath === "/relevantText"));
          assert.ok(active.decisionReadQueue.some(entry => entry.sectionPath === "/authorRequirementsSource"), "final decision must read the complete frozen author request");
          if (active.chooseOperation === "finishChapter") assert.ok(active.decisionReadQueue.some(entry => entry.sectionPath === "/relevantText"), "current saved chapter must be in this task's catalogue");
          active.decisionReadText = "";
        } else if (active.decisionReadPhase === "source") {
          const entry = active.decisionReadQueue[0];
          assert.ok(entry);
          assert.equal(value.sourceId, entry.sourceId);
          assert.equal(value.contentHash, entry.contentHash);
          assert.equal(value.byteRange.start, Buffer.byteLength(active.decisionReadText));
          active.decisionReadText += value.payloadPart;
          if (value.nextCursor) {
            evidenceTool("read_task_source", { sourceId: entry.sourceId, cursor: value.nextCursor }, "source");
            return;
          }
          assert.equal(value.byteRange.total, Buffer.byteLength(active.decisionReadText));
          assert.equal(createHash("sha256").update(active.decisionReadText).digest("hex"), entry.contentHash);
          if (entry.sectionPath === "/relevantText") assert.ok(active.decisionReadText.includes(active.chapterReviewRevision), "model must read the action's actual saved draft");
          JSON.parse(active.decisionReadText);
          active.decisionReadQueue.shift(); active.decisionReadText = "";
        } else if (active.decisionReadPhase === "report") {
          const assessment = active.chapterAssessment;
          assert.equal(value.state, "current");
          assert.equal(value.revisionId, assessment.reportRevisionId);
          assert.ok(value.assessment.sourceId);
          active.chapterReport = value; active.chapterReportFields = new Map(); active.chapterReportOffset = 0;
          evidenceTool("read_source_content", { sourceId: value.assessment.sourceId }, "report-content");
          return;
        } else if (active.decisionReadPhase === "report-content") {
          assert.equal(value.sourceId, active.chapterReport.assessment.sourceId);
          assert.equal(value.contentRead, true);
          assert.equal(value.coverage.fromEntry, active.chapterReportOffset);
          active.chapterReportOffset = value.coverage.toEntry;
          for (const field of value.entries) active.chapterReportFields.set(field.path,
            Object.hasOwn(field, "value") ? field.value : (active.chapterReportFields.get(field.path) ?? "") + field.text);
          if (value.nextCursor) {
            evidenceTool("read_source_content", { sourceId: value.sourceId, cursor: value.nextCursor }, "report-content");
            return;
          }
          assert.equal(value.coverage.toEntry, value.coverage.totalEntries);
          assert.equal(active.chapterReportFields.get("/draftRevisionId"), active.chapterReviewRevision);
          const reportOutcomes = [...active.chapterReportFields].filter(([path]) => /^\/outcomes\/\d+\/id$/.test(path));
          assert.deepEqual(active.chapterAssessment.checkCounts, {
            total: reportOutcomes.length, supported: reportOutcomes.length, unmet: 0, insufficientEvidence: 0,
          }, "the current bounded status must count the actual independently saved report outcomes");
          active.chapterReview = {
            sourceRevisionId: active.chapterReviewRevision, assessmentRevisionId: active.chapterReport.revisionId,
            checks: active.workStatus.chapterAcceptance.requirements.map(criterion => {
              const field = reportOutcomes.find(([, value]) => value === criterion.id);
              assert.ok(field, `current report must cover ${criterion.id}`);
              const prefix = field[0].slice(0, -3);
              assert.equal(active.chapterReportFields.get(`${prefix}/status`), "supported");
              const issueId = active.chapterReportFields.get(`${prefix}/issue/issueId`) ?? null;
              return { criterionId: criterion.id, issueId };
            }), length: active.workStatus.chapterAcceptance.length,
          };
          active.decisionReadDone = true;
        }
        if (!active.decisionReadDone) {
          if (active.decisionReadQueue.length) {
            evidenceTool("read_task_source", { sourceId: active.decisionReadQueue[0].sourceId }, "source");
            return;
          }
          if (active.chooseOperation === "finishChapter") {
            active.chapterAssessment = active.workStatus.requirements.map(requirement => requirement.assessment)
              .find(assessment => assessment?.draftRevisionId === active.chapterReviewRevision);
            assert.ok(active.chapterAssessment?.reportRevisionId, "finish uses the current saved draft's actual independent report");
            assert.equal(active.chapterAssessment.current, true); assert.equal(active.chapterAssessment.verdict, "passed");
            evidenceTool("read_revision_outcome", { writerTaskId: active.chapterAssessment.writerTaskId }, "report");
            return;
          }
          active.decisionReadDone = true;
        }
      }
      if (active.stage > 0 && !readingDecisionSource) {
        const previous = body.messages.findLast((message) => message.role === "tool");
        assert.ok(previous, `${active.kind} tool result must reach the model`);
        const output = String(previous.content);
        assert.ok(output.includes("Output:"), output.slice(0, 500));
        let value = JSON.parse(output.slice(output.indexOf("Output:") + 7).trim());
        if (active.kind.startsWith("decision-")) {
          if (active.stage === 1 || active.stage === 3) {
            if (active.workStatus) {
              assert.equal(value.version, active.workStatus.version);
              assert.equal(value.sourceDigest, active.workStatus.sourceDigest);
              active.workStatus.receipts.items.push(...value.receipts.items);
              active.workStatus.readyActions.push(...value.readyActions);
              active.workStatus.requirements.push(...value.requirements);
            } else active.workStatus = value;
            if (value.pagination.nextCursor) {
              active.calls.push("read_work_status");
              sendTool(response, tool("read_work_status"), { ...active.binding, cursor: value.pagination.nextCursor }, `${active.kind}-status-${active.calls.length}`);
              return;
            }
            value = active.workStatus;
            if (!active.planSatisfied
              && ((!active.chooseOperation && active.stage === 1)
                || (active.chooseOperation === "finishChapter" && active.stage === 3))) {
              await updateFixtureDeliveryPlan((method, path, body) => request(method, path, body, candidateToken), base, active.fixtureTask,
                { status: "satisfied", evidence: currentFixtureEvidence(value, await request("GET", `${base}/creative-workspace`)) });
              active.planSatisfied = true;
              // Persisted plan satisfaction affects completion readiness. Read
              // the real current status before asserting or reporting closure.
              active.workStatus = null;
              active.calls.push("read_work_status");
              sendTool(response, tool("read_work_status"), { ...active.binding }, `${active.kind}-status-after-plan`);
              return;
            }
            if (active.stage === 3) assert.equal(value.finalDeliveryReviewRequired, true);
            else if (active.chooseOperation) assert.ok(value.readyActions.some(action => action.operation === active.chooseOperation),
              `the agent must select an available ${active.chooseOperation} action: ${JSON.stringify(value)}`);
            else assert.equal(value.canFinish, true, "the agent may finish only after persisted evidence satisfies this work");
          }
          else if (active.stage === 2 && active.chooseOperation) {
            active.selectedStep = value.step ?? null;
            if (active.chooseOperation === "finishChapter") active.workStatus = null;
          }
        } else if (active.stage === 1) {
          if (!active.sourceQueue) {
            assert.equal(value.taskSources.readTool, "read_source_content");
            assert.equal(value.taskSources.textDelivered, false);
            for (const entry of value.taskSources.entries) {
              const reference = entry.sectionPath.slice(1).split("/").reduce((part, key) => part[key], value);
              if (entry.sectionPath === "/taskWorkspace") {
                assert.equal(reference.sourceId, entry.sourceId); assert.equal(reference.textDelivered, false);
                assert.equal(reference.view, "workspace");
                assert.match(reference.instruction, /read_source_content\(view=workspace\)/);
              } else assert.deepEqual(reference, { sourceId: entry.sourceId, textDelivered: false });
            }
            active.context = value;
            active.sourceQueue = [...value.taskSources.entries];
            active.sourceText = "";
          } else {
            const source = active.sourceQueue[0];
            assert.equal(value.sourceId, source.sourceId);
            assert.equal(value.contentHash, source.contentHash);
            assert.equal(value.sectionPath, source.sectionPath);
            assert.equal(value.byteRange.start, Buffer.byteLength(active.sourceText));
            assert.ok(Buffer.byteLength(value.payloadPart) > 0 && Buffer.byteLength(value.payloadPart) <= 2000);
            assert.ok(Buffer.byteLength(JSON.stringify(value)) <= 6500);
            active.sourceText += value.payloadPart;
            assert.equal(value.byteRange.end, Buffer.byteLength(active.sourceText));
            if (!value.nextCursor) {
              const entry = active.sourceQueue.shift();
              assert.equal(value.endOfInputFromCursor, true);
              assert.equal(value.byteRange.total, Buffer.byteLength(active.sourceText));
              assert.equal(entry.payloadBytes, Buffer.byteLength(active.sourceText));
              assert.equal(createHash("sha256").update(active.sourceText).digest("hex"), entry.contentHash);
              const parts = entry.sectionPath.slice(1).split("/");
              let container = active.context;
              for (const part of parts.slice(0, -1)) container = container[part];
              container[parts.at(-1)] = JSON.parse(active.sourceText);
              active.sourceText = "";
            }
          }
          if (active.sourceQueue.length) {
            active.calls.push("read_task_source");
            sendTool(response, tool("read_task_source"), { ...active.binding,
              sourceId: active.sourceQueue[0].sourceId, ...(value.nextCursor ? { cursor: value.nextCursor } : {}) }, `${active.kind}-page-${active.calls.length}`);
            return;
          }
          value = active.context;
        }
        if (active.stage === 1 && !active.kind.startsWith("decision-")) {
          const context = JSON.stringify(value);
          const expectedBody = ["writer", "discussion"].includes(active.kind) ? "她烧掉密函。"
            : ["model-next", "next-revision-outcome"].includes(active.kind) ? nextPayload.body
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
            active.outcomeInput = input;
            assert.deepEqual(input.desiredOutcomes.filter(item => !target.chapterAcceptance.requirements.some(criterion => criterion.id === item.id)), repairGoals,
              "evaluate the newly approved additional goals alongside all frozen chapter standards");
            assert.equal(input.beforeScenes.find(scene => scene.groupId === "letter").body, "她将密函藏进衣襟。");
            assert.equal(input.afterScenes.find(scene => scene.groupId === "letter").body, repairedScene);
          }
          if (active.kind === "revision-outcome" || active.kind === "next-revision-outcome") {
            active.outcomeInput = value.operationContext.revisionOutcomeInput;
          }
        } else if (active.kind.startsWith("decision-")) {
          // The actual MCP decision receipt was retained above.
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
      if (active.kind.startsWith("decision-") && active.stage === 1 && !active.decisionReadDone
        && (active.chooseOperation === "finishChapter" || !active.chooseOperation)) {
        if (active.chooseOperation === "finishChapter") {
          const finish = active.workStatus.readyActions.find(action => action.operation === "finishChapter");
          assert.ok(finish?.sourceRevisionId);
          active.chapterReviewRevision = finish.sourceRevisionId;
        }
        evidenceTool("read_context", { budget: 24, byteBudget: 60000 }, "context");
        return;
      }
      const deliveryCompletion = ["complete_task", { ...active.binding, summary: "已核对本次授权目标及实际保存成果；检查仅覆盖本轮改稿与指定后续章节。",
            deliveryReview: {
              expectedWorkVersion: active.workStatus?.version,
              expectedSourceDigest: active.workStatus?.sourceDigest,
              goalStatus: "fulfilled", goalAssessment: "本次授权目标已按当前保存回执完成",
              checkedScope: "核对本轮限定场景修改、效果评估及明确授权的后续章节；未检查全书",
              unresolved: [], issueResolutions: [], authorDecisions: [],
              evidence: (active.workStatus?.receipts.items ?? []).flatMap(receipt => receipt.outputs.filter(output => output.revisionId).map(output => ({ taskId: receipt.taskId, revisionId: output.revisionId }))),
            },
          }];
      const stages = active.kind.startsWith("decision-")
        ? [["read_work_status", { ...active.binding }], ...(active.chooseOperation ? [["request_work_action", {
            ...active.binding, expectedWorkVersion: active.workStatus?.version,
            expectedSourceDigest: active.workStatus?.sourceDigest,
            actionKey: active.workStatus?.readyActions.find(action => action.operation === active.chooseOperation)?.actionKey,
            instruction: "根据来源和交付证据完成本次授权目标", idempotencyKey: active.kind,
            ...(active.workStatus?.workTaskId?.startsWith("work-generation-") && ["generateChapter","writeNextChapter"].includes(active.chooseOperation) ? {
              chapterAcceptance:active.workStatus.chapterAcceptance ?? {requirements:[{id:"saved-chapter",text:"保存本次授权的章节"}],length:{requirement:"本测试没有篇幅限制",minCharacters:null,maxCharacters:null}},
            } : {}),
            ...(active.chooseOperation === "finishChapter" ? { chapterReview: active.chapterReview } : {}),
          }], ...(active.chooseOperation === "finishChapter" ? [["read_work_status", { ...active.binding }], deliveryCompletion] : [])] : [deliveryCompletion])]
        : ["advisory", "discussion", "repair-discussion"].includes(active.kind)
        ? [["read_context", { ...active.binding, budget: 24, byteBudget: 60000 }]]
        : active.kind === "repair-proposal"
        ? [
            ["read_context", { ...active.binding, budget: 24, byteBudget: 60000 }],
            ["request_candidate_task", { ...active.binding, expectedIntentRevision: 0, idempotencyKey: "repair-motive-proposal",
              operation: "generateChapter", target: repairTarget, instruction: "只调整密函场景，让藏信与主动留下证词的动机相连" }],
          ]
        : active.kind === "repair-outcome"
        ? [
            ["read_context", { ...active.binding, budget: 24, byteBudget: 60000 }],
            ["submit_revision_outcome", { ...active.binding, writerTaskId: active.target.writerTaskId,
              expectedCandidateRevisionId: null, idempotencyKey: "chapter-one-repair-outcome",
              payload: { writerTaskId: active.target.writerTaskId, chapterId: active.target.chapterId,
                originalRevisionId: active.target.originalRevisionId, draftRevisionId: active.target.draftRevisionId,
                knowledgeBatchId: active.target.knowledgeBatchId, uncoveredRequirements: [],
                outcomes: active.stage > 0 ? active.outcomeInput.desiredOutcomes.map(item => ({ id: item.id, status: "supported", issue: null,
                  reason: item.id === "protected-scenes" ? "原保护场景和锁定结尾保持不变" : "新稿明确藏信并亲自留下证词",
                  evidence: [{ groupId: item.id === "protected-scenes" ? "ending" : "letter", quote: item.id === "protected-scenes" ? "白塔熄灭。" : "决定亲自留下证词" }] })) : [] } }],
            ["complete_task", { ...active.binding, summary: "再次改稿的批准目标已提交候选评估" }],
          ]
        : ["revision-outcome", "next-revision-outcome"].includes(active.kind)
        ? [
            ["read_context", { ...active.binding, budget: 24, byteBudget: 60000 }],
            ["submit_revision_outcome", { ...active.binding, writerTaskId: active.target.writerTaskId,
              expectedCandidateRevisionId: null, idempotencyKey: `outcome-${active.kind}-${active.target.chapterId}`,
              payload: { writerTaskId: active.target.writerTaskId, chapterId: active.target.chapterId,
                originalRevisionId: active.target.originalRevisionId, draftRevisionId: active.target.draftRevisionId,
                knowledgeBatchId: active.target.knowledgeBatchId, uncoveredRequirements: [],
                outcomes: active.stage > 0 ? active.outcomeInput.desiredOutcomes.map(item => ({ id: item.id, status: "supported", issue: null,
                  reason: active.kind === "next-revision-outcome" ? "已读取实际下一章全文并核对冻结要求" : item.id === "protected-scenes" ? "原保护场景和锁定结尾保持不变" : "新稿明确林夕主动藏信",
                  evidence: [{ groupId: active.kind === "next-revision-outcome" ? "opening-two" : item.id === "protected-scenes" ? "ending" : "letter", quote: active.kind === "next-revision-outcome" ? nextPayload.body : item.id === "protected-scenes" ? "白塔熄灭。" : "她将密函藏进衣襟。" }] })) : [] } }],
            ["complete_task", { ...active.binding, summary: "改稿预期已提交候选评估" }],
          ]
        : active.kind === "extraction"
        ? [
            ["read_context", { ...active.binding, budget: 24, byteBudget: 60000 }],
            ["submit_commitment_extraction", { ...active.binding, chapterId: active.chapterId,
              expectedCandidateRevisionId: null, idempotencyKey: "chapter-two-extraction",
              payload: { chapterId: active.chapterId, sourceRevisionId: active.sourceRevisionId,
                examinedRanges: [{ start: 0, end: Array.from(nextPayload.body).length }], candidates: [] } }],
            ["complete_task", { ...active.binding, summary: "第二章已逐字检查叙事承诺" }],
          ]
        : active.kind === "assessment"
          ? [
              ["read_context", { ...active.binding, budget: 24, byteBudget: 60000 }],
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
            ["read_context", { ...active.binding, budget: 24, byteBudget: 60000 }],
            ["save_chapter_plans", { ...active.binding, expectedCandidateRevisionId: null, idempotencyKey: "rolling-plan-two",
              payload: { chapters: [{ chapterPlanId: "plan-two", chapterNumber: 2, goal: "循藏信线索追查密函",
                scenes: [{ id: "scene-two", goal: "林夕确认密函仍在衣襟里", beats: [{ id: "beat-two", description: "林夕决定继续藏信" }],
                  expectedEffects: [{ id: "effect-two", dimension: "knowledge", description: "密函仍由林夕保管", commitmentRefs: [] }] }] }] } }],
            ["complete_task", { ...active.binding, summary: "沿修订正文规划第二章" }],
          ]
        : ["writer", "repair-writer"].includes(active.kind)
        ? [
            ["read_context", { ...active.binding, readingChapter: 1, sceneOrder: 1, budget: 24, byteBudget: 60000 }],
            ["save_draft_checkpoint", { ...active.binding,
              expectedCandidateRevisionId: active.kind === "writer" ? original.revisionId : repairTarget.scope.sourceRevisionId,
              idempotencyKey: active.kind + "-revision", payload: {
                chapterId: "chapter-one", chapterNumber: 1, title: originalPayload.title, completeness: "chapterComplete",
                sceneGroups: active.kind === "writer" ? revisedPayload.sceneGroups : repairedPayload.sceneGroups } }],
            ["complete_task", { ...active.binding, summary: "只修改密函场景" }],
          ]
        : active.kind === "continuation"
          ? [
              ["read_context", { ...active.binding, readingChapter: 2, sceneOrder: 1, budget: 24, byteBudget: 60000 }],
              ["save_draft_checkpoint", { ...active.binding, expectedCandidateRevisionId: null, idempotencyKey: "next-chapter",
                payload: { chapterId: active.chapterId, chapterNumber: nextPayload.chapterNumber,
                  title: nextPayload.title, completeness: "chapterComplete", sceneGroups: nextPayload.sceneGroups } }],
              ["complete_task", { ...active.binding, summary: "沿藏信新稿续写第二章" }],
            ]
          : [
            ["read_context", { ...active.binding, budget: 24, byteBudget: 60000 }],
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
        response.end(`data: ${JSON.stringify({ id: `scoped-final-${active.kind}`, choices: [{ delta: { content: active.finalText }, finish_reason: "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 } })}\n\ndata: [DONE]\n\n`);
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
      downstreamFailure ??= error;
      response.writeHead(500, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: error.message } }));
    }
  });
  await new Promise((resolve, reject) => { downstream.once("error", reject); downstream.listen(0, "127.0.0.1", resolve); });
  // This multi-task contract journey retains the full real Codex history. The scripted
  // provider declares room for it; this does not verify real-provider compaction.
  const connection = defineConnection({ id: "scoped-model", provider: "fixture", endpoint: `http://127.0.0.1:${downstream.address().port}`,
    model: "fixture-scoped", credentialRef: "FIXTURE_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 256000, outputTokens: 4000 } });
  const plan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection, { protocol: connection.protocol,
    capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING] }),
    policy: { defaultConnectionId: connection.id, allowedMcpServerIds: ["novel_workspace"] } });
  async function runTask(kind, operation, taskTarget, batchId, authorIntent, sourceRevisionId, chooseOperation) {
    const bridge = new ModelBridge({ sessionPlan: plan, connections: [connection], credentialResolver: async () => "fixture-key", token: `scoped-${kind}-bridge` });
    bridges.push(bridge);
    enterStage(`Codex ${kind}: starting model bridge`);
    const address = await bridge.start();
    const home = join(root, "codex-home");
    await mkdir(join(home, "sqlite"), { recursive: true, mode: 0o700 });
    const catalog = join(home, "catalog.json");
    await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(plan, { baseInstructions: "只调用授权的小说工作区工具。" })), { mode: 0o600 });
    await writeFile(join(home, "config.toml"), compileCodexConfig({ sessionPlan: plan, modelCatalogPath: catalog,
      bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
      mcpServers: [{ id: "novel_workspace", command: nativeBinary("novel-workspace-mcp"), args: [],
        envVars: ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"], approvalMode: "approve", required: true, startupTimeoutMs: 90_000,
        tools: NOVEL_TOOL_OUTPUT_POLICY }],
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
    startup = createMcpStartupWaiter({ serverName: "novel_workspace", process: adapter.process, timeoutMs: 90_000 });
    t.after(startup.dispose);
    enterStage(`Codex ${kind}: starting isolated session`);
    const workId = taskTarget?.workDecision?.workTaskId ?? (taskTarget?.runId ? `work-${taskTarget.runId}` : null);
    const existingWork = workId ? await request("GET", `${base}/work-tasks/${workId}`) : null;
    if (existingWork?.threadId) await adapter.resumeThread(existingWork.threadId);
    else await adapter.ensureSession();
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
    if (kind.startsWith("decision-") || ["discussion", "repair-proposal"].includes(kind)) {
      await updateFixtureDeliveryPlan((method, path, body) => request(method, path, body, candidateToken), base, task);
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
    active.fixtureTask = task;
    active.planSatisfied = false;
    active.chooseOperation = chooseOperation;
    active.sourceQueue = null;
    active.context = null;
    active.workStatus = null;
    active.selectedStep = null;
    active.binding = { taskId: task.id, projectId: project.id, threadId: adapter.threadId, expectedRevision: workspaceRevision };
    active.batchId = batchId;
    active.sourceRevisionId = sourceRevisionId;
    active.chapterId = taskTarget?.chapterId;
    active.target = taskTarget;
    active.stage = 0; active.decisionReadPhase = null; active.decisionReadDone = false; active.evidenceSequence = 0; active.chapterReview = null; active.chapterAssessment = null;
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
    const settlementDeadline = Date.now() + 1000;
    while (budget.snapshot().reservedTokens && Date.now() < settlementDeadline) await new Promise(resolve => setTimeout(resolve, 10));
    const usage = budget.snapshot();
    assert.deepEqual({ tokens: usage.tokens, unknown: usage.unknownTokens, reserved: usage.reservedTokens },
      { tokens: usage.requests * 15, unknown: 0, reserved: 0 }, "every scripted provider response must settle exact usage within the original budget");
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
    assert.deepEqual(active.calls.filter((name, index, calls) => name !== "read_task_source" && (name !== "read_work_status" || calls.indexOf(name) === index)), kind.startsWith("decision-")
      ? ["read_work_status", ...(chooseOperation ? ["request_work_action", ...(chooseOperation === "finishChapter" ? ["complete_task"] : [])] : ["complete_task"])] : kind === "repair-proposal" ? ["read_context", "request_candidate_task"]
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
    if (operation === "advisory" && !(kind.startsWith("decision-") && (!chooseOperation || chooseOperation === "finishChapter"))) await request("POST", `${base}/tasks/${task.id}/complete`, { threadId: adapter.threadId, summary: result.message });
    if (taskTarget?.runId) {
      budget.complete();
      const runPath = `${base}/generation-runs/${taskTarget.runId}`;
      const charged = await request("POST", `${runPath}/provider-usage`, { taskId: task.id });
      await request("POST", `${runPath}/advance-task`, { expectedVersion: charged.version, taskId: task.id });
    } else {
      await settle(task);
    }
    if (kind.startsWith("decision-")) task.selectedStep = active.selectedStep;
    await adapter.close();
    return task;
  }
  let decisionSequence = 0;
  async function chooseWork(workPath, operation) {
    const work = await request("GET", workPath);
    const source = await request("GET", `${base}/creative-workspace`);
    const next = await request("POST", `${workPath}/decision`, {
      expectedWorkVersion: work.version, expectedRevision: source.revision, expectedSourceDigest: source.sourceDigest,
    });
    assert.ok(next.step, JSON.stringify(next));
    const parent = await runTask(`decision-${++decisionSequence}`, "advisory", {
      ...next.step.target, resumeStepId: next.step.id,
    }, undefined, undefined, undefined, operation);
    return parent.selectedStep;
  }
  async function chooseGeneration(runPath, operation) {
    const next = await request("POST", `${runPath}/next`, {});
    assert.equal(next.action.operation, "advisory");
    await runTask(`decision-${++decisionSequence}`, "advisory", next.action.target,
      undefined, undefined, undefined, operation);
    return request("POST", `${runPath}/next`, {});
  }
  const discussion = await runTask("discussion", "advisory", { type: "project" });
  const authorHistory = `${discussion.inputManifest.authorIntent}\n\n作者追加要求：\n${appendedText}`;
  const scopePath = `${base}/tasks/${discussion.id}/execution-scope`;
  const scopeRequest = { actionIds: [action.id], maxActions: 1, maxTaskAttempts: 16, expectedIntentRevision: 1 };
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
  const workPath = `${base}/work-tasks/${preview.workTaskId}`;
  const modelStep = await chooseWork(workPath, "modelKnowledge");
  const batch = { id: modelStep.target.batchId };
  await runTask("model", "modelKnowledge", { ...modelStep.target, resumeStepId: modelStep.id }, batch.id);
  const updated = await request("GET", statePath);
  assert.equal(updated.entries.length, 1);
  assert.equal(updated.entries[0].sourceRevisionId, draft.id);
  assert.equal(updated.entries[0].assertion.value, "hidden");
  assert.equal(updated.coverage.complete, true);
  const outcomeStep = await chooseWork(workPath, "assessRevisionOutcome");
  assert.equal(outcomeStep.target.writerTaskId, writer.id);
  const outcomeTask = await runTask("revision-outcome", "assessRevisionOutcome", { ...outcomeStep.target, resumeStepId: outcomeStep.id });
  assert.equal(grants.get(outcomeTask.id).workTaskId, preview.workTaskId);
  const outcome = await request("GET", `${base}/revision-outcomes/${writer.id}`);
  assert.equal(outcome.authority, "candidateAnalysis");
  assert.equal(outcome.state, "current");
  assert.deepEqual(outcome.assessment.outcomes.filter(item => item.id === "retain-letter"), [{ id: "retain-letter", status: "supported", issue: null, reason: "新稿明确林夕主动藏信",
    evidence: [{ groupId: "letter", ...sourceEvidence("她将密函藏进衣襟。", "她将密函藏进衣襟。") }] }]);
  await chooseWork(workPath, null);
  const sourceAfterFinish = await request("GET", `${base}/creative-workspace`);
  const beforeFinish = await request("GET", workPath);
  const finishRequest = { expectedWorkVersion: beforeFinish.version, expectedRevision: sourceAfterFinish.revision,
    expectedSourceDigest: sourceAfterFinish.sourceDigest };
  assert.equal((await request("POST", `${workPath}/decision`, finishRequest)).stopReason, "completed");
  const finishedWork = await request("GET", workPath);
  assert.equal(finishedWork.status, "completed");
  assert.equal(finishedWork.deliveryStatus, "ready_for_review");
  assert.deepEqual(finishedWork.deliveryReport.pendingActionIds, []);
  assert.deepEqual(finishedWork.deliveryReport.unsettledAttemptIds, []);
  for (const kind of ["knowledgeBatch", "draft", "revisionOutcome"]) {
    assert.ok(finishedWork.deliveryReport.evidence.some(item => item.kind === kind && item.state === "recorded"));
  }
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
  const repairModelStep = await chooseWork(repairWorkPath, "modelKnowledge");
  const repairBatch = { id: repairModelStep.target.batchId };
  const repairModel = await runTask("repair-model", "modelKnowledge",
    { ...repairModelStep.target, resumeStepId: repairModelStep.id }, repairBatch.id);
  const repairOutcomeStep = await chooseWork(repairWorkPath, "assessRevisionOutcome");
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
  assert.ok(repairedOutcome.assessment.outcomes.some(item => item.id === repairGoals[0].id));
  await chooseWork(repairWorkPath, null);
  const repairedWork = await request("GET", repairWorkPath);
  assert.equal(repairedWork.deliveryStatus, "ready_for_review");
  assert.deepEqual(repairedWork.deliveryReport.pendingActionIds, []);
  assert.deepEqual(repairedWork.deliveryReport.unsettledAttemptIds, []);
  assert.ok(providerRequests.filter(kind => kind.startsWith("repair-")).length < 256);
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
    targetChapters: 1, maxTaskAttempts: 32, maxProviderRequests: 256, maxProviderTokens: 1_000_000,
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
  const planning = await chooseGeneration(runPath, "generatePlan");
  assert.equal(planning.action.operation, "generatePlan");
  assert.equal(planning.action.target.planFromChapter, 2);
  await runTask("planner", "generatePlan", planning.action.target, null, "客户端不可替换：恢复烧信设定", repairedDraft.id);
  const writing = await chooseGeneration(runPath, "writeNextChapter");
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
  const modeling = await chooseGeneration(runPath, "modelKnowledge");
  assert.equal(modeling.action.operation, "modelKnowledge");
  const nextBatch = { id: modeling.action.target.batchId };
  await runTask("model-next", "modelKnowledge", modeling.action.target,
    nextBatch.id, modeling.action.authorIntent);
  const nextState = await request("GET", statePath);
  assert.ok(nextState.entries.some(entry => entry.sourceRevisionId === workspace.chapters[1].candidate.id
    && entry.assertion.value === "hidden"), "the second chapter must produce its own current knowledge");
  assert.ok(nextState.entries.every(entry => entry.assertion.value !== "burned"));
  assert.equal(nextState.coverage.complete, true);
  const nextIndependent = await chooseGeneration(runPath, "assessRevisionOutcome");
  await runTask("next-revision-outcome", "assessRevisionOutcome", nextIndependent.action.target,
    null, nextIndependent.action.authorIntent);
  const extraction = await chooseGeneration(runPath, "extractCommitments");
  assert.equal(extraction.action.operation, "extractCommitments");
  assert.equal(extraction.action.target.sourceRevisionId, workspace.chapters[1].candidate.id);
  await runTask("extraction", "extractCommitments", extraction.action.target,
    null, extraction.action.authorIntent, workspace.chapters[1].candidate.id);
  const assessment = await chooseGeneration(runPath, "assessSceneOutcome");
  assert.equal(assessment.action.operation, "assessSceneOutcome");
  assert.equal(assessment.action.target.knowledgeBatchId, nextBatch.id);
  await runTask("assessment", "assessSceneOutcome", assessment.action.target,
    nextBatch.id, assessment.action.authorIntent);
  await chooseGeneration(runPath, "finishChapter");
  const completed = await request("GET", runPath);
  assert.equal(completed.status, "completed");
  assert.equal(completed.completedChapters, 2);
  assert.equal(completed.chapterDeliveries.length, 1);
  const delivery = completed.chapterDeliveries[0];
  assert.equal(delivery.chapterId, writing.action.target.chapterId);
  assert.equal(delivery.chapterNumber, 2);
  assert.equal(delivery.draftRevisionId, workspace.chapters[1].candidate.id);
  assert.equal(delivery.knowledgeBatchId, nextBatch.id);
  assert.ok(Number.isSafeInteger(delivery.checkedRunVersion));
  assert.equal(delivery.sourceDigest, completed.sourceDigest);
  assert.equal(delivery.feedbackDecisionId, null);
  assert.deepEqual((await request("GET", runPath)).chapterDeliveries, completed.chapterDeliveries);
  const generationWork = await request("GET", `${base}/work-tasks/work-${run.id}`);
  assert.equal(generationWork.deliveryStatus, "ready_for_review");
  assert.deepEqual(generationWork.deliveryReport.unsettledAttemptIds, []);
  assert.deepEqual([...finalReplies.keys()].filter(kind => !kind.startsWith("decision-")), ["discussion", "writer", "model", "revision-outcome", "repair-discussion",
    "repair-proposal", "repair-writer", "repair-model", "repair-outcome", "planner",
    "continuation", "model-next", "next-revision-outcome", "extraction", "assessment"]);
  const beforeRead = providerRequests.length;
  await rereadFinalReplies();
  assert.equal(providerRequests.length, beforeRead, "historical receipt reads must not add provider usage");
});

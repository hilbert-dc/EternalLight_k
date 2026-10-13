import { mcpRequestIdentity } from "../mcp-authority.mjs";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { DELEGATION_EXECUTION_CAPABILITIES, ModelBridgeProcess, compileCodexConfig, compileCodexModelCatalog,
  defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS } from "../creative-mission.mjs";
import { NOVEL_TOOL_DISCOVERY_POLICY } from "../codex-runtime.mjs";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

// Production Host coordinator, actual Core/bridge/MCP/Native and one persistent
// Work. Only model responses are scripted. No external correction or second
// author message is supplied; this is mechanism evidence, not DeepSeek quality.
test("one author goal autonomously repairs a failed independent assessment before delivery", { timeout: 180000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-autonomous-creation-"));
  const suffix = process.platform === "win32" ? ".exe" : "";
  const binary = nativeTestBinary;
  const authorToken = "autonomous-author-token-123456789", candidateToken = "autonomous-candidate-token-123456789";
  const runtime = spawn(binary("novel-codex-story-runtime"), ["--port", "0", "--db", join(directory, "story.sqlite3"),
    "--engine-budget-db", join(directory, "engine.sqlite3"), "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
  runtime.stderr.resume();
  let main, bridge, provider, providerFailure, active;
  const created = [], decisions = [], requests = [], calls = [], protocolChecks = [], states = new Map();
  let initialBudgetCount = 0, budgetIncreaseCount = 0;
  t.after(async () => {
    main?.chapterStructureConsumer.stop(); main?.adapter.close();
    await bridge?.close();
    if (provider?.listening) await new Promise(resolve => { provider.closeAllConnections(); provider.close(resolve); });
    if (runtime.exitCode === null && runtime.signalCode === null) {
      const stopped = new Promise(resolve => runtime.once("exit", resolve)); runtime.kill(); await stopped;
    }
    await rm(directory, { recursive: true, force: true });
  });
  const origin = await new Promise((resolve, reject) => {
    const lines = createInterface({ input: runtime.stdout });
    lines.on("line", line => { if (line.startsWith("Story Runtime listening on ")) resolve(line.slice("Story Runtime listening on ".length)); });
    runtime.once("error", reject); runtime.once("exit", () => reject(new Error("Runtime exited before listening")));
    runtime.stdin.end(`${JSON.stringify({ authorToken, candidateToken })}\n`);
  });
  const http = async (path, body, method = body ? "POST" : "GET", capability = null) => {
    const response = await fetch(`${origin}${path}`, { method, headers: { "content-type": "application/json",
      authorization: `Bearer ${capability ? candidateToken : authorToken}`,
      ...(capability ? { "x-novel-task-capability": capability } : {}) }, ...(body ? { body: JSON.stringify(body) } : {}) });
    const raw = await response.text();
    assert.match(response.headers.get("content-type") ?? "", /^application\/json\b/, `${method} ${path}: HTTP ${response.status}: ${raw.slice(0, 2000)}`);
    const value = JSON.parse(raw); assert.ok(response.ok, `${method} ${path}: ${response.status} ${JSON.stringify(value)}`); return value;
  };
  const { project } = await http("/api/projects", { title: "自主改稿机制验收", logline: "限定改稿和独立检查的机制验收", medium: "novel", idempotencyKey: "autonomous-creation-project" });
  const base = `/api/projects/${project.id}`;
  const groups = [
    { id: "opening", sceneId: null, body: "林夕走进港口。", locked: false },
    { id: "letter", sceneId: null, body: "她烧掉密函。", locked: false },
    { id: "ending", sceneId: null, body: "白塔熄灭。", locked: true },
  ];
  const original = await http(`${base}/drafts/checkpoints`, { expectedRevision: 0, expectedCandidateRevisionId: null,
    idempotencyKey: "original", payload: { chapterId: "chapter-one", chapterNumber: 1, title: "密函",
      body: groups.map(group => group.body).join("\n\n---\n\n"), sceneGroups: groups } });
  const author = "只把第1章密函场景改为林夕主动藏信并留下证词，其他场景及锁定结尾保持原文。独立检查，未达到要求就自行修订，保存候选不要采用。本次无篇幅限制。";
  const acceptance = { requirements: [{ id: "letter-action", text: "密函场景为林夕主动藏信并留下证词" }, { id: "protected-scenes", text: "其他场景及锁定结尾保持原文" }],
    length: { requirement: "本次无篇幅限制", minCharacters: null, maxCharacters: null } };
  const goal = { id: "chapter-delivery", title: "完成限定改稿并独立检查", kind: "deliverable", status: "inProgress",
    acceptance: "密函场景改为主动藏信并留下证词，其余正文原样保留，独立检查全部通过后交付候选。",
    dependsOn: [], readingQuestion: "原稿、实际改稿及独立报告是否支持限定交付？", checkScope: "第1章密函场景和保护范围",
    approach: "读原稿，限定起草，读取独立失败报告后定向修订和复评。", assessment: "尚未达成，不宣称通过。",
    evidence: [], sourceIds: [], sourceChecks: [], continuation: null };
  const parseResult = body => {
    const previous = body.input.findLast(item => item.type === "function_call_output");
    assert.ok(previous, "tool output must reach the model");
    const text = typeof previous.output === "string" ? previous.output : previous.output.filter(part => part.type === "input_text").at(-1).text;
    const marker = text.indexOf("Output:");
    const value = JSON.parse(marker < 0 ? text : text.slice(marker + 7).trim());
    assert.equal(value.isError, undefined, text.slice(0, 1000)); assert.ok(value.error == null,text.slice(0,2000)); assert.equal(value.code,undefined,text.slice(0,2000));
    return value;
  };
  provider = createServer(async (incoming, response) => {
    try {
      const chunks = []; for await (const chunk of incoming) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks)); requests.push({ task: active.id, operation: active.operation, body });
      let state = states.get(active.id);
      if (!state) {
        const initial = body.tools.find(entry => entry.type === "namespace" && entry.name === "mcp__novel_workspace");
        assert.ok(initial);
        const allowed = [...NOVEL_TOOL_DISCOVERY_POLICY.initialTools, ...NOVEL_TOOL_DISCOVERY_POLICY.discoveryTools].map(tool => tool.name);
        for (const tool of initial.tools) {
          if (allowed.includes(tool.name)) continue;
          const prior = requests.slice(0, -1).filter(request => created.find(task => task.id === request.task)?.threadId === active.threadId)
            .flatMap(request => request.body.tools.find(entry => entry.name === "mcp__novel_workspace")?.tools ?? [])
            .find(entry => entry.name === tool.name && JSON.stringify(entry) === JSON.stringify(tool));
          assert.ok(prior, `${tool.name}: new task may retain only an unchanged schema already loaded for this caller`);
        }
        state = { phase: "context", sources: [], values: new Map(), text: "" }; states.set(active.id, state);
      }
      const binding = { projectId: project.id, taskId: active.id, threadId: active.threadId, expectedRevision: active.inputRevision };
      const tool = (name, input) => {
        const { projectId: _project, taskId: _task, threadId: _thread, ...args } = input;
        const namespace = body.tools.find(entry => entry.type === "namespace" && entry.name === "mcp__novel_workspace");
        const declared = namespace?.tools.find(entry => entry.name === name);
        if (!declared && name !== "discover_creative_capabilities") {
          state.deferred = { name, args, cursor: null, query: name === "save_draft_checkpoint" ? "正文 save_draft_checkpoint" : name };
          return tool("discover_creative_capabilities", { threadId: active.threadId, query: state.deferred.query });
        }
        assert.ok(declared, `available tool ${name} for ${active.operation}: ${JSON.stringify(body.tools.map(entry=>({type:entry.type,name:entry.name})))}`);
        calls.push({ taskId: active.id, operation: active.operation, name, args });
        const index = requests.length;
        const item = { type: "function_call", id: `item-${index}`, call_id: `call-${index}`, namespace: namespace.name, name: declared.name, arguments: JSON.stringify(args) };
        response.writeHead(200, { "content-type": "text/event-stream" });
        for (const event of [{ type: "response.created", response: { id: `r-${index}` } },
          { type: "response.output_item.done", output_index: 0, item },
          { type: "response.completed", response: { id: `r-${index}`, status: "completed", output: [item], usage: { input_tokens: 10, output_tokens: 5, total_tokens: 15 } } }]) response.write(`data: ${JSON.stringify(event)}\n\n`);
        response.end("data: [DONE]\n\n");
      };
      const finish = () => {
        const index = requests.length, item = { type: "message", id: `msg-${index}`, role: "assistant", status: "completed",
          content: [{ type: "output_text", text: "当前职责已完成，依据实际保存和独立检查结果继续。", annotations: [] }] };
        response.writeHead(200, { "content-type": "text/event-stream" });
        response.write(`data: ${JSON.stringify({ type: "response.output_item.done", output_index: 0, item })}\n\n`);
        response.end(`data: ${JSON.stringify({ type: "response.completed", response: { id: `r-${index}`, status: "completed", output: [item], usage: { input_tokens: 10, output_tokens: 5, total_tokens: 15 } } })}\n\ndata: [DONE]\n\n`);
      };
      const next = (phase, name, args) => { state.phase = phase; tool(name, { ...binding, ...args }); };
      if (state.deferred) {
        const discovery = parseResult(body), deferred = state.deferred;
        assert.equal(discovery.toolDiscovery.version, 1);
        assert.ok(discovery.toolDiscovery.tools.every(tool => discovery.items.some(item => item.id === tool.name && item.kind === "tool" && item.availability === "available")));
        if (!discovery.toolDiscovery.tools.some(tool => tool.name === deferred.name)) {
          assert.ok(discovery.nextCursor != null, `discovery must locate ${deferred.name} in its current admitted catalog`);
          return tool("discover_creative_capabilities", { threadId: active.threadId, query: deferred.query, cursor: discovery.nextCursor });
        }
        state.deferred = null;
        const loaded = body.tools.find(entry => entry.type === "namespace" && entry.name === "mcp__novel_workspace")?.tools.find(tool => tool.name === deferred.name);
        assert.ok(loaded?.parameters, "the actual discovery result loads the real complete schema before its call");
        return tool(deferred.name, deferred.args);
      }
      if (state.phase === "context") return next("context-result", "read_context", { budget: 24, byteBudget: 60000 });
      if (state.phase === "context-result") {
        const value = parseResult(body);
        state.sources = value.taskSources.entries.filter(entry => ["/relevantText", "/operationContext/revisionOutcomeInput", "/authorRequirementsSource"].includes(entry.sectionPath));
        state.phase = "source-start";
      } else if (state.phase === "source-result") {
        const value = parseResult(body), entry = state.sources[0];
        assert.equal(value.sourceId, entry.sourceId); state.text += value.payloadPart;
        if (value.nextRead) return next("source-result", value.nextRead.tool, value.nextRead.arguments);
        state.values.set(entry.sectionPath, JSON.parse(state.text)); state.text = ""; state.sources.shift(); state.phase = "source-start";
      }
      if (state.phase === "source-start") {
        if (state.sources.length) return next("source-result", "read_source_content", { view: "raw", sourceId: state.sources[0].sourceId });
        if (active.operation === "generateChapter") {
          const writerNumber = created.filter(task => task.operation === "generateChapter").length;
          const scene = writerNumber === 1 ? "她把密函投入火中，纸角烧成灰。" : "她将密函藏进衣襟，决定亲自留下证词。";
          const payload = { chapterId: "chapter-one", chapterNumber: 1, title: "密函", completeness: "chapterComplete",
            sceneGroups: groups.map(group => group.id === "letter" ? { ...group, body: scene } : group) };
          assert.ok(state.values.has("/relevantText"), "writer must read complete protected original");
          return next("write-result", "save_draft_checkpoint", { expectedCandidateRevisionId: active.inputManifest.expectedCandidateRevisionId,
            idempotencyKey: `write-${active.id}`, payload });
        }
        if (active.operation === "assessRevisionOutcome") {
          const input = state.values.get("/operationContext/revisionOutcomeInput");
          assert.ok(input, "independent critic reads frozen criteria and actual current prose");
          const repaired = input.afterScenes.find(scene => scene.groupId === "letter").body.includes("留下证词");
          state.assessmentInput = input; state.repaired = repaired;
          if (repaired) {
            const priorIssue = input.issues.entries.find(issue => issue.issueId === "letter-action-problem");
            assert.ok(priorIssue, "the critic discovers the previously registered problem in its current bound input");
            state.issueId = priorIssue.issueId;
            return next("prior-issue-result", "read_work_issues", { issueId: state.issueId, cursor: 0, limit: 16 });
          }
          state.phase = "submit-assessment";
        }
        if (active.operation !== "assessRevisionOutcome") return next("plan-result", "read_work_plan", {});
      }
      if (state.phase === "prior-issue-result") {
        const issues = parseResult(body);
        assert.equal(issues.issueId, state.issueId);
        state.issueObservations ??= [];
        state.issueObservations.push(...issues.entries);
        if (issues.nextCursor !== null) {
          assert.ok(Number.isSafeInteger(issues.nextCursor) && issues.nextCursor > 0);
          state.issueCursors ??= new Set();
          assert.ok(!state.issueCursors.has(issues.nextCursor), "copy real keyset cursors without repeating or skipping a page");
          state.issueCursors.add(issues.nextCursor);
          return next("prior-issue-result", "read_work_issues", { issueId: state.issueId, cursor: issues.nextCursor, limit: 16 });
        }
        assert.equal(state.issueObservations.length, issues.totalCount);
        const prior = state.issueObservations.filter(observation => observation.status === "unmet");
        assert.equal(prior.length, 1, "the repaired critic must locate the original problem, not create a new identity");
        assert.equal(prior[0].taskId, created.find(task => task.operation === "assessRevisionOutcome").id);
        assert.ok(prior[0].reportRevisionId && prior[0].observationId);
        state.resolutionOf = prior.map(observation => observation.observationId);
        state.phase = "submit-assessment";
      }
      if (state.phase === "submit-assessment") {
        const input = state.assessmentInput, repaired = state.repaired, target = active.target;
        return next("assessment-result", "submit_revision_outcome", { writerTaskId: target.writerTaskId,
          expectedCandidateRevisionId: active.inputManifest.expectedCandidateRevisionId, idempotencyKey: `review-${active.id}`,
          payload: { writerTaskId: target.writerTaskId, chapterId: target.chapterId, originalRevisionId: target.originalRevisionId,
            draftRevisionId: target.draftRevisionId, knowledgeBatchId: target.knowledgeBatchId, uncoveredRequirements: [],
            outcomes: input.desiredOutcomes.map(criterion => {
              const protectedCriterion = criterion.criterion.includes("其他场景");
              const scene = input.afterScenes.find(scene => scene.groupId === (protectedCriterion ? "ending" : "letter"));
              return { id: criterion.id, status: protectedCriterion || repaired ? "supported" : "unmet",
                reason: protectedCriterion ? "锁定结尾保持原文。" : repaired ? "正文明确藏信及亲自留下证词。" : "正文仍烧毁密函，没有主动藏信及留下证词，须定向修订密函场景。",
                issue: protectedCriterion ? null : { issueId: repaired ? state.issueId : "letter-action-problem", newIssue: repaired ? null : { summary: "密函仍被烧毁，未主动藏信并留下证词" }, resolutionOf: repaired ? state.resolutionOf : [] },
                evidence: [{ groupId: scene.groupId, quote: scene.body }] };
            }) } });
      }
      if (["write-result", "assessment-result", "action-result"].includes(state.phase)) {
        state.output = parseResult(body); return next("completed", "complete_task", { summary: "本项已保存，交由同Work继续检查。" });
      }
      if (state.phase === "completed") { parseResult(body); return finish(); }
      if (state.phase === "plan-result") {
        const plan = parseResult(body); state.planVersion = plan.version;
        if (!active.inputManifest.workDecision) return next("initial-plan-saved", "update_work_plan", {
          expectedPlanVersion: plan.version, idempotencyKey: `plan-${active.id}`, plan: { intentMode: "executeGoals", goals: [goal] } });
        return next("status-result", "read_work_status", {});
      }
      if (state.phase === "initial-plan-saved") {
        parseResult(body);
        return next("action-result", "request_candidate_task", { expectedIntentRevision: 0, idempotencyKey: `initial-${active.id}`,
          operation: "generateChapter", target: { type: "chapter", chapterId: "chapter-one", chapterNumber: 1, readingChapter: 1, sceneOrder: 1,
            scope: { type: "selectedScenes", sourceRevisionId: original.revisionId, groupIds: ["letter"] }, chapterAcceptance: acceptance },
          instruction: "只把密函场景改为主动藏信并留下证词，保护其他原文，保存完整章节候选。" });
      }
      if (["status-result", "status-more"].includes(state.phase)) {
        const page = parseResult(body);
        if (state.phase === "status-result") state.status = page;
        else {
          assert.equal(page.version, state.status.version); assert.equal(page.sourceDigest, state.status.sourceDigest);
          state.status.requirements.push(...page.requirements); state.status.readyActions.push(...page.readyActions);
          state.status.receipts.items.push(...page.receipts.items); state.status.pagination = page.pagination;
        }
        if (page.pagination.nextCursor) return next("status-more", "read_work_status", { cursor: page.pagination.nextCursor });
        state.phase = "status-ready";
      }
      if (state.phase === "acceptance-page") {
        const page = parseResult(body);
        assert.equal(page.contentHash, state.status.chapterAcceptanceSource.contentHash);
        state.acceptanceText += page.payloadPart;
        if (page.nextRead) return next("acceptance-page", page.nextRead.tool, page.nextRead.arguments);
        state.acceptance = JSON.parse(state.acceptanceText);
        state.acceptanceHash = page.contentHash;
        state.phase = "status-ready";
      }
      if (state.phase === "status-ready") {
        if (state.status.chapterAcceptanceSource && state.acceptanceHash !== state.status.chapterAcceptanceSource.contentHash) {
          state.acceptanceText = "";
          return next("acceptance-page", "read_source_content", { sourceId: state.status.chapterAcceptanceSource.sourceId, view: "raw" });
        }
        const assessment = state.status.requirements.find(requirement => requirement.kind === "revisionOutcome")?.assessment;
        assert.ok(assessment, "saved writer must always require independent evaluation");
        if (assessment.reportRevisionId) return next("report-result", "read_revision_outcome", { writerTaskId: assessment.writerTaskId });
        state.phase = "status-evaluate";
      }
      if (state.phase === "report-result") {
        state.report = parseResult(body);
        const assessment = state.status.requirements.find(requirement => requirement.kind === "revisionOutcome").assessment;
        assert.equal(state.report.state, "current"); assert.equal(state.report.revisionId, assessment.reportRevisionId);
        assert.ok(state.report.assessment.sourceId, "the report reference is not delivered report content");
        state.reportFields = new Map(); state.reportOffset = 0;
        return next("report-page-result", "read_source_content", { sourceId: state.report.assessment.sourceId });
      }
      if (state.phase === "report-page-result") {
        const page = parseResult(body);
        assert.equal(page.sourceId, state.report.assessment.sourceId); assert.equal(page.contentRead, true);
        assert.equal(page.coverage.fromEntry, state.reportOffset); state.reportOffset = page.coverage.toEntry;
        for (const field of page.entries) state.reportFields.set(field.path, Object.hasOwn(field,"value") ? field.value : (state.reportFields.get(field.path)??"")+field.text);
        if (page.nextRead) return next("report-page-result", page.nextRead.tool, page.nextRead.arguments);
        assert.equal(page.coverage.toEntry, page.coverage.totalEntries);
        const assessment = state.status.requirements.find(requirement => requirement.kind === "revisionOutcome").assessment;
        assert.equal(state.reportFields.get("/draftRevisionId"), assessment.draftRevisionId);
        assert.ok([...state.reportFields].some(([path,value])=>/\/evidence\/\d+\/quote$/.test(path) && typeof value==="string" && value.length), "model reads actual report evidence, not its directory");
        state.phase = "status-evaluate";
      }
      if (state.phase === "status-evaluate") {
        const status = state.status;
        const assessment = status.requirements.find(requirement => requirement.kind === "revisionOutcome").assessment;
        decisions.push({ taskId: active.id, assessment, status });
        if (assessment.verdict === "notMet") {
          assert.ok(assessment.checkCounts.unmet > 0, "current Native summary preserves the actual failed check count");
          assert.ok([...state.reportFields].some(([path,value]) => /\/reason$/.test(path) && typeof value === "string" && value.includes("烧毁")),
            "model reads the actual failed report reason before selecting repair");
          assert.ok([...state.reportFields.values()].some(value=>typeof value==="string" && value.includes("烧成灰")), "the repair decision has actually read the failed prose quote");
          if (!state.repairPlanned) {
            state.repairPlanned = true;
            return next("repair-plan-saved", "update_work_plan", { expectedPlanVersion: state.planVersion, idempotencyKey: `method-${active.id}`,
              plan: {intentMode:"executeGoals",goals:[{...goal,approach:"独立报告指出密函仍烧成灰；仅letter改为藏信并明确亲自留下证词，再独立复评。",assessment:"第一项未满足，实际引文是纸角烧成灰；其余保护原文保留，标准不能降低。"}]} });
          }
          return next("action-result", "request_work_action", { expectedWorkVersion: status.version, expectedSourceDigest: status.sourceDigest,
            proposal: { operation: "generateChapter", target: { chapterId: "chapter-one", scope: { type: "selectedScenes", groupIds: ["letter"] } } },
            chapterAcceptance: state.acceptance, instruction: "独立报告指出密函仍被烧毁：仅改letter场景为藏进衣襟，并明确亲自留下证词的动机，保护其余原文。",
            idempotencyKey: `repair-${active.id}` });
        }
        if (assessment.verdict !== "passed") {
          const action = status.readyActions.find(action => action.operation === "assessRevisionOutcome"); assert.ok(action);
          return next("action-result", "request_work_action", { expectedWorkVersion: status.version, expectedSourceDigest: status.sourceDigest,
            actionKey: action.actionKey, instruction: "独立读取冻结标准、当前稿及必要原稿，逐项评估，不继承writer成功宣称。", idempotencyKey: `critic-${active.id}` });
        }
        assert.equal(assessment.current, true); assert.equal(assessment.length.satisfied, true);
        assert.equal(status.issues.openCount, 0); assert.equal(status.issues.staleCount, 0); assert.equal(status.issues.disputedCount, 0);
        const evidence = status.receipts.items.filter(receipt => receipt.status === "completed")
          .flatMap(receipt => receipt.outputs.filter(output => [assessment.draftRevisionId, assessment.reportRevisionId].includes(output.revisionId))
            .map(output => ({ taskId: receipt.taskId, revisionId: output.revisionId })));
        state.evidence = evidence; assert.equal(evidence.length, 2);
        state.deliveryEvidence = status.receipts.items.filter(receipt=>receipt.status==="completed").flatMap(receipt=>receipt.outputs.filter(output=>output.kind!=="draft" || output.revisionId===assessment.draftRevisionId).map(output=>({taskId:receipt.taskId,revisionId:output.revisionId})));
        state.phase="final-plan-update";
      }
      if(state.phase==="final-plan-update")return next("final-plan-saved", "update_work_plan", { expectedPlanVersion: state.planVersion, idempotencyKey: `fulfilled-${active.id}`,
        plan: { intentMode: "executeGoals", goals: [{ ...goal, status: "satisfied", assessment: "真实当前稿和独立报告全部标准通过，保护范围保持。", evidence:state.evidence }] } });
      if (state.phase === "repair-plan-saved") { parseResult(body); return next("status-result", "read_work_status", {}); }
      if (state.phase === "final-plan-saved") { parseResult(body); return next("final-status-result", "read_work_status", {}); }
      if (state.phase === "final-status-result") {
        const status = parseResult(body); assert.equal(status.canFinish, true);
        return next("completed", "complete_task", { summary: "限定改稿与独立检查已通过，候选已保存。", deliveryReview: {
          expectedWorkVersion: status.version, expectedSourceDigest: status.sourceDigest, goalStatus: "fulfilled",
          goalAssessment: "当前稿明确主动藏信及留下证词，独立评估全部通过。", checkedScope: "第1章密函场景与原文保护范围",
          unresolved: [], issueResolutions: [], authorDecisions: [], evidence: state.deliveryEvidence } });
      }
      throw new Error(`unexpected phase ${state.phase}`);
    } catch (error) { providerFailure ??= error; response.writeHead(500, { "content-type": "application/json" }); response.end(JSON.stringify({ error: error.message })); }
  });
  await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", protocol: "responses",
    endpoint: `http://127.0.0.1:${provider.address().port}`, credentialRef: "FIXTURE", protocolOptions: { reasoningEffort: "none" },
    // This fixture scripts literary decisions, not compaction summaries. Use
    // the production context capacity; real compaction + lease rotation has
    // its own Core/MCP integration test.
    declaredLimits: { contextTokens: 1048576, outputTokens: 8192 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection),
    policy: { allowedMcpServerIds: ["novel_workspace"], productExtensions: DELEGATION_EXECUTION_CAPABILITIES, toolDiscovery: NOVEL_TOOL_DISCOVERY_POLICY } });
  bridge = new ModelBridgeProcess({mcpAuthority:true,requestConfigurationProvider:({threadId})=>mcpRequestIdentity(main.adapter,threadId), sessionPlan, connections: [connection], credentialResolver: async () => "fixture" });
  const address = await bridge.start(), catalog = join(directory, "models.json");
  await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Controlled integration provider." })));
  const codexConfig = compileCodexConfig({ sessionPlan, modelCatalogPath: catalog,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" }, developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS,
    mcpServers: [{ id: "novel_workspace", command: binary("novel-workspace-mcp"), args: [], envVars: ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION","NOVEL_CODEX_ENGINE_AUTHORITY_URL","NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN"], approvalMode: "approve", required: true, startupTimeoutMs: 90000, inputSchemaMaxBytes: 40000 }] });
  assert.match(codexConfig, /^tool_input_schema_max_bytes = 40000$/m);
  await writeFile(join(directory, "config.toml"), codexConfig);
  const state = { onPrepareDomain: domain => { domain.modelBridge = bridge; }, runtimeUrl: origin, nativeRuntime: true, agentStorage: { budgetDb: join(directory, "engine.sqlite3") }, modelBridge: bridge,
    providerConfig: { id: "fixture", provider: "fixture", model: "fixture", connections: [connection], sessionPlan,
      operations: ["advisory", "generateChapter", "assessRevisionOutcome"] },
    adapterEnv: { CODEX_HOME: directory, CODEX_SQLITE_HOME: join(directory, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token,NOVEL_CODEX_ENGINE_AUTHORITY_URL:address.baseUrl,NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN:address.token,
      NOVEL_CODEX_RUNTIME_URL: origin, NOVEL_CODEX_CANDIDATE_TOKEN: candidateToken }, confirmations: [] };
  const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/codex${suffix}`, import.meta.url));
  main = await mainHandlers(async (path, body, options, url) => {
    const value = await http(path + url.search, body, options.method ?? "GET", options.headers?.["x-novel-task-capability"] ?? null);
    if (path === `${base}/tasks` && body) { active = value; created.push(value); if(body.initialBudget)initialBudgetCount++; }
    if (!body && path.startsWith(`${base}/tasks/`) && path.endsWith("/work-decision")) {
      protocolChecks.push({ taskId: path.split("/").at(-2), deliveryReviewRequired: value.deliveryReviewRequired,
        finalDeliveryReviewRequired: value.finalDeliveryReviewRequired, canFinish: value.canFinish });
    }
    if(path.endsWith("/budget-authorization") && body)budgetIncreaseCount++;
    return value;
  }, [codex, "app-server"], state);
  let result;
  try { result = await main.handlers.get("agent:send-prompt")(rendererEvent(), project.id, author, null, { view: "chapters", chapterId: "chapter-one", chapterNumber: 1 }); }
  catch (error) { t.diagnostic(JSON.stringify({ created: created.map(task=>task.operation), phases:[...states].map(([id,state])=>({id,phase:state.phase})), calls:calls.slice(-3) })); throw providerFailure ?? error; }
  if (providerFailure) t.diagnostic(JSON.stringify({
    phases: [...states].map(([id, state]) => ({ id, operation: created.find(task => task.id === id)?.operation,
      phase: state.phase, readSections: [...state.values.keys()], sources: state.sources })),
    calls: calls.slice(-12),
  }));
  assert.ifError(providerFailure);
  if (result.executionFailure) {
    const tasks = await Promise.all(created.map(task => http(`${base}/tasks/${task.id}`)));
    const work = await http(`${base}/tasks/${created[0].id}/work-task`);
    t.diagnostic(JSON.stringify({ tasks: tasks.map(task => ({ id: task.id, operation: task.operation, status: task.status,
      error: task.error })), work: { id: work.id, status: work.status, deliveryStatus: work.deliveryStatus,
      deliveryReview: work.deliveryReport.deliveryReview }, protocolChecks, failure: result.executionFailure }));
  }
  assert.equal(result.executionFailure, undefined, JSON.stringify(result.executionFailure));
  const workspace = await http(`${base}/creative-workspace`), current = workspace.chapters[0].candidate;
  assert.match(current.body, /藏进衣襟，决定亲自留下证词/);
  assert.deepEqual(current.sceneGroups.filter(group => group.id !== "letter"), groups.filter(group => group.id !== "letter"));
  assert.deepEqual(created.map(task => task.operation), ["advisory", "generateChapter", "advisory", "assessRevisionOutcome", "advisory", "generateChapter", "advisory", "assessRevisionOutcome", "advisory"]);
  assert.ok(decisions.some(decision => decision.assessment.verdict === "notMet")); assert.equal(decisions.at(-1).assessment.verdict, "passed");
  assert.equal(new Set((await Promise.all(created.map(task => http(`${base}/tasks/${task.id}/work-task`)))).map(work => work.id)).size, 1);
  const critics = created.filter(task => task.operation === "assessRevisionOutcome"), writers = created.filter(task => task.operation === "generateChapter");
  for (const critic of critics) assert.ok(writers.every(writer => writer.threadId !== critic.threadId));
  const history = await main.adapter.readThread(created[0].threadId);
  assert.equal(history.turns.flatMap(turn => turn.items).filter(item => item.type === "userMessage" && item.text === author).length, 1);
  assert.equal(state.confirmations.length, 0);
  assert.equal(initialBudgetCount,1); assert.equal(budgetIncreaseCount,0);
  assert.equal(result.workTask.status,"completed"); assert.equal(result.workTask.deliveryReport.deliveryReview.goalStatus,"fulfilled");
  assert.equal(result.workTask.deliveryReport.deliveryReview.unresolved.length,0);
  assert.equal(result.workTask.deliveryReport.deliveryReview.evidence.length,3,"delivery covers latest draft and both real independent report receipts; failed history is retained");
  const problemPage = await http(`${base}/work-tasks/${result.workTask.id}/issues?cursor=0&limit=16`);
  assert.equal(problemPage.totalCount, 1, "repairs preserve one stable problem rather than rotating identities");
  const problem = problemPage.entries[0];
  assert.equal(problem.issueId, "letter-action-problem"); assert.equal(problem.status, "resolved");
  assert.equal(problem.observationCount, 2);
  assert.deepEqual(problem.observations, [], "overview metadata never substitutes for reading actual observations");
  const observations = [], issueCursors = new Set();
  let issueCursor = 0;
  for (;;) {
    const page = await http(`${base}/work-tasks/${result.workTask.id}/issues?issueId=${encodeURIComponent(problem.issueId)}&cursor=${issueCursor}&limit=16`);
    assert.equal(page.issueId, problem.issueId); assert.equal(page.totalCount, 2);
    observations.push(...page.entries);
    if (page.nextCursor === null) break;
    assert.ok(Number.isSafeInteger(page.nextCursor) && page.nextCursor > 0 && !issueCursors.has(page.nextCursor));
    issueCursors.add(page.nextCursor); issueCursor = page.nextCursor;
  }
  assert.equal(observations.length, 2);
  const negative = observations.find(observation => observation.status === "unmet");
  const supported = observations.find(observation => observation.status === "supported");
  assert.ok(negative && supported); assert.notEqual(negative.taskId, supported.taskId);
  assert.notEqual(negative.draftRevisionId, supported.draftRevisionId);
  assert.equal(negative.current, false); assert.equal(negative.superseded, true); assert.equal(negative.recheckRequired, false);
  assert.equal(supported.current, true); assert.equal(supported.superseded, false); assert.equal(supported.recheckRequired, false);
  assert.equal(negative.issueId, supported.issueId);
  assert.deepEqual(supported.resolutionOf, [negative.observationId], "independent current assessment explicitly resolves the actual original observation");
  assert.equal(supported.reportRevisionId, decisions.at(-1).assessment.reportRevisionId);
  assert.equal(supported.draftRevisionId, current.id);
  assert.ok(calls.some(call => call.name === "read_work_issues" && call.args.issueId === problem.issueId), "the model reads actual Native observations before resolving the problem");
  assert.equal(calls.some(call => call.name === "discover_creative_capabilities" && call.args.query === "read_work_issues"), false, "essential issue state is loaded from the admitted catalog without a discovery round trip");
  for (const critic of critics) {
    const first=requests.find(request=>request.task===critic.id).body;
    assert.equal(JSON.stringify(first.input).includes(author),false,"fresh critic receives frozen standards/prose, not full author or writer conversation");
  }
  assert.equal(calls.filter(call => call.name === "request_work_action" && call.args.proposal).length, 1, "the model selected the repair after its actual failed report");
  assert.equal(calls.some(call => call.name === "discover_creative_capabilities" && call.args.query === "正文 save_draft_checkpoint"), false, "the admitted core save operation is already a bootstrap tool");
  assert.ok(calls.some(call => call.name === "discover_creative_capabilities" && call.args.query === "submit_revision_outcome"), "independent critics discover their own authorized schema");
});

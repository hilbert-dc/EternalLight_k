import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";

test("chapter-scoped tool events retain meaningful actions in names and summaries", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "conversation-operation-scope-"));
  try {
    await build({ entryPoints: Object.fromEntries(["operation-presentation", "conversation-value", "agent-events"]
      .map(name => [name, new URL(`../../src/${name}.ts`, import.meta.url).pathname])), outdir: directory,
      bundle: true, platform: "node", format: "esm", outExtension: { ".js": ".mjs" } });
    const get = name => import(pathToFileURL(path.join(directory, `${name}.mjs`)).href);
    const { operationPresentation: present, operationHeading } = await get("operation-presentation");
    const { operationSummary } = await get("conversation-value");
    const { createAgentRun, reduceAgentEvent: reduce, summarizeAgentRun } = await get("agent-events");
    const cases = [
      ["query_story", "查询第 7 章 · 故事证据", "read"],
      ["search_story", "查找第 7 章 · 故事内容", "read"],
      ["search_relations", "查询第 7 章 · 人物关系", "read"],
      ["resolve_story_evidence", "定位第 7 章 · 正文依据", "read"],
      ["analyze_story_impact", "检查第 7 章 · 故事关联影响", "analysis"],
      ["simulate_story_hypothesis", "分析第 7 章 · 故事假设", "analysis"],
      ["read_chapter_structure_source", "读取第 7 章 · 删前章节", "read"],
      ["publish_knowledge_batch", "保存第 7 章 · 故事设定分析", "write"],
      ["record_quality_note", "记录第 7 章 · 作品质量线索", "write"],
      ["submit_planning_review_report", "提交第 7 章 · 审阅报告", "write"],
      ["request_generation_handoff", "准备第 7 章 · 续写安排", "arrange"],
      ["request_planning_review", "安排第 7 章 · 多视角规划检查", "arrange"],
    ];
    for (const [tool, title, category] of cases) {
      const item = { id: tool, type: "mcpToolCall", tool, arguments: { chapterNumber: 7, query: "PRIVATE_QUERY" } };
      let run = { ...createAgentRun("核对章节"), threadId: "thread", turnId: "turn" };
      const event = (method, value) => ({ method, params: { threadId: "thread", turnId: "turn", item: value } });
      run = reduce(run, event("item/started", item));
      const active = run.steps.find(step => step.id === tool);
      assert.equal(active.title, title, tool);
      assert.equal(operationSummary([active]), `正在${title}`, tool);
      run = reduce(run, event("item/completed", { ...item, status: "completed", result: { structuredContent: { bounded: true } } }));
      const completed = run.steps.find(step => step.id === tool);
      assert.equal(completed.title, title, tool);
      assert.equal(completed.operation.category, category);
      const summary = operationSummary([completed]);
      assert.equal(summary, category === "read" ? `已${title}` : category === "write" ? `${title} · 保存结果待核实` : `已执行：${title}`, tool);
      const snapshot = summarizeAgentRun({ ...run, state: "completed" }).displayExecution;
      assert.equal(snapshot.steps.find(step => step.id === tool).title, title);
      assert.ok(!JSON.stringify(snapshot).includes("PRIVATE_QUERY"));
      assert.equal(completed.operation.outcome, category === "read" ? "read" : undefined,
        "A precise chapter name must not manufacture a save receipt or validated analysis");
    }
    for (const chapterNumber of [undefined, "7", -1, 0, 2.5, Number.MAX_SAFE_INTEGER + 1]) {
      const item = { tool: "search_story", arguments: { chapterNumber, query: "查询第 9 章人物动机" }, status: "completed" };
      assert.equal(operationHeading(present(item)), "查找故事内容", "Free query text and malformed scope cannot create a chapter title");
    }
    assert.equal(operationHeading(present({ tool: "analyze_story_impact", status: "completed", result: { structuredContent: { chapterNumber: 9 } } })),
      "检查第 9 章 · 故事关联影响", "Returned structured chapter metadata remains visible");
    assert.equal(operationHeading(present({ tool: "read_work_status", arguments: { chapterNumber: 7 } })), "内部操作");
    for (const tool of ["constructor", "toString", "unknown_tool"]) assert.equal(present({ tool }).category, "diagnostic");
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("value projection binds sources, receipts, recovery and meaningful progress without protocol leakage", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "conversation-value-"));
  try {
    await build({ entryPoints: Object.fromEntries(["operation-presentation", "conversation-value", "agent-events", "work-conversation"]
      .map(name => [name, new URL(`../../src/${name}.ts`, import.meta.url).pathname])), outdir: directory,
      bundle: true, platform: "node", format: "esm", outExtension: { ".js": ".mjs" } });
    const get = name => import(pathToFileURL(path.join(directory, `${name}.mjs`)).href);
    const { operationPresentation: present, operationHeading } = await get("operation-presentation");
    const { meaningfulProgress, readingRecovery, operationSummary, operationIcon } = await get("conversation-value");
    const { createAgentRun, reduceAgentEvent: reduce, summarizeAgentRun } = await get("agent-events");
    const { conversationWorkSteps } = await get("work-conversation");
    let run = { ...createAgentRun("核对章纲"), threadId: "thread", turnId: "turn" };
    const emit = item => { run = reduce(run, { method: "item/completed", params: { threadId: "thread", turnId: "turn", item } }); };
    emit({ id: "directory", type: "mcpToolCall", tool: "read_context", status: "completed", result: {
      structuredContent: { taskSources: { entries: [{ sourceId: "source-plan", sectionPath: "/plannedEffects", chapterNumbers: [7, 8, 9] }] } },
    } });
    const read = { type: "mcpToolCall", tool: "read_source_content", arguments: { sourceId: "source-plan" } };
    emit({ ...read, id: "failed", status: "completed", result: { isError: true, content: [{ type: "text", text: "READ_ERROR" }] } });
    emit({ ...read, id: "success", status: "completed", result: { structuredContent: { sourceId: "source-plan", contentRead: true, returnedChapterNumbers: [], entries: [{ path: "/0/title", value: "PRIVATE_PAYLOAD" }] } } });
    const success = run.steps.find(step => step.id === "success"), failed = run.steps.find(step => step.id === "failed");
    assert.equal(success.title, "读取章节规划片段", "Source catalog range is not returned page coverage");
    assert.equal(failed.status, "failed"); assert.equal(failed.detail, "READ_ERROR");
    assert.equal(readingRecovery(run.steps, failed), "recovered");
    assert.equal(readingRecovery(run.steps, success), "recovery");
    const nextPage = { ...success, operation: { ...success.operation, retryKey: "different-page" } };
    assert.equal(readingRecovery([failed, nextPage], failed), null);
    assert.equal(present({ ...read, status: "completed", result: { structuredContent: { sourceId: "foreign-source" } } }).outcome, undefined);
    assert.equal(present({ ...read, status: "completed", result: { structuredContent: { sourceId: "source-plan", contentRead: false } } }).outcome, undefined,
      "Metadata-only returns do not prove body reading");
    assert.equal(present({ ...read, status: "completed", result: { structuredContent: { sourceId: "source-plan" } } }).outcome, undefined,
      "An identity without returned content is not a read receipt");
    assert.equal(conversationWorkSteps(run).some(step => step.id === "directory"), false);
    const snapshot = summarizeAgentRun({ ...run, state: "completed" }).displayExecution;
    assert.equal(snapshot.steps.find(step => step.id === "success").operation.object, success.operation.object);
    assert.equal(JSON.stringify(snapshot).includes("PRIVATE_PAYLOAD"), false);
    const draft = { tool: "save_draft_checkpoint", status: "completed", arguments: { payload: { chapterId: "chapter", chapterNumber: 7 } },
      result: { structuredContent: { chapterId: "chapter", revisionId: "rev", checkpointId: "checkpoint", version: 1 } } };
    assert.equal(operationHeading(present(draft)), "已保存第 7 章正文");
    assert.equal(present({ ...draft, result: null }).outcome, undefined);
    assert.equal(present({ ...draft, result: { structuredContent: { chapterId: "foreign", revisionId: "rev", checkpointId: "checkpoint", version: 1 } } }).outcome, undefined);
    assert.equal(present({ ...draft, result: { ...draft.result, isError: true } }).outcome, undefined);
    run = reduce(run, { method: "item/started", params: { threadId: "thread", turnId: "turn",
      item: { ...draft, id: "draft", type: "mcpToolCall", result: undefined, status: "running" } } });
    emit({ id: "draft", type: "mcpToolCall", status: "completed", result: draft.result });
    assert.equal(run.steps.find(step => step.id === "draft").title, "已保存第 7 章正文");
    const artifact = { tool: "save_outline", status: "completed", result: { structuredContent: { artifactKind: "outline", revisionId: "revision", version: 1, payload: {} } } };
    assert.equal(operationHeading(present(artifact)), "已保存大纲");
    assert.equal(present({ ...artifact, result: { structuredContent: { artifactKind: "brief", revisionId: "revision", version: 1, payload: {} } } }).outcome, undefined);
    assert.equal(present({ tool: "unknown_protocol_tool", status: "completed" }).category, "diagnostic");
    const dynamic = present({ tool: "read_source_content", status: "completed", contentItems: [{ type: "inputText", text: JSON.stringify({ sourceId: "source", sectionPath: "/narrativeGoals/outline" }) }] });
    assert.equal(operationHeading(dynamic), "读取作品大纲片段");
    const note = (id, text) => ({ id, kind: "system", messagePhase: "commentary", status: "completed", text, title: "Agent 进度" });
    const notes = [note("one", "正在读取章纲。"), note("two", "继续读取章纲。"), note("finding", "发现第七章的时间线冲突，需要先核对。"),
      note("phase", "正在读取作品大纲。"), note("decision", "需要你确认结尾方向。")];
    assert.deepEqual(meaningfulProgress(notes).map(step => step.id), ["one", "two", "finding", "phase", "decision"]);
    assert.deepEqual(meaningfulProgress([notes[0], { ...notes[0], id: "duplicate" }]).map(step => step.id), ["duplicate"]);
    assert.equal(meaningfulProgress([notes[0], success, { ...notes[0], id: "after-read" }]).length, 3);
    const write = { id: "write", kind: "tool", status: "completed", title: "保存章节", operation: { category: "write" } };
    assert.equal(meaningfulProgress([notes[0], write, notes[1]]).length, 3);

    const material = { id: "material", kind: "tool", status: "completed", title: "读取章节规划（第 7—15 章）",
      operation: { category: "read", tool: "read_source_content", object: "章节规划（第 7—15 章）", sourceId: "source-plan", outcome: "read" } };
    const saved = { id: "saved", kind: "tool", status: "completed", title: "已保存第 7 章正文",
      operation: { category: "write", tool: "save_draft_checkpoint", object: "第 7 章正文", outcome: "saved", referenceId: "checkpoint" } };
    const unconfirmed = { ...saved, id: "unconfirmed", title: "保存第 7 章正文", operation: { ...saved.operation, outcome: undefined, referenceId: undefined } };
    assert.equal(operationSummary([material]), "已读取章节规划（第 7—15 章）");
    assert.equal(operationSummary([material, { ...material, id: "page-two" }]), operationSummary([material]), "Pagination should not add a call count or repeat its source");
    assert.equal(operationSummary([saved]), "已保存第 7 章正文");
    assert.match(operationSummary([unconfirmed]), /保存第 7 章正文.*结果待核实/);
    assert.doesNotMatch(operationSummary([unconfirmed]), /已保存/);
    assert.equal(operationSummary([material, saved]), "已读取章节规划（第 7—15 章） · 已保存第 7 章正文");
    const active = { ...material, id: "active", status: "running", operation: { ...material.operation, outcome: undefined } };
    const unresolved = { ...material, id: "unresolved", status: "failed", title: "读取作品大纲",
      operation: { ...material.operation, object: "作品大纲", sourceId: "source-outline", outcome: undefined } };
    const stopped = { ...material, id: "stopped", status: "cancelled", operation: { ...material.operation, outcome: undefined } };
    const mixed = operationSummary([unconfirmed, unresolved, stopped, active]);
    assert.match(mixed, /正在.*章节规划/);
    assert.match(mixed, /部分操作未完成/);
    assert.match(mixed, /已停止/);
    assert.match(mixed, /结果待核实/);
    assert.doesNotMatch(mixed, /已保存/);
    assert.match(operationSummary([{ ...active, status: "waiting" }, unresolved]), /等待.*章节规划.*部分操作未完成/);
    assert.match(operationSummary([unresolved]), /作品大纲.*未完成/);
    assert.equal(operationSummary([{ id: "command", kind: "tool", title: "核对文本", status: "completed" }]), "已执行：核对文本");
    assert.equal(operationSummary([{ id: "already", kind: "tool", title: "已核对文本", status: "completed" }]), "已核对文本");
    assert.doesNotMatch(operationSummary([material, saved, { ...material, id: "brief", operation: { ...material.operation, object: "创作简报", sourceId: "source-brief" } }]), /3 次|第 1 次/);
    assert.match(operationSummary([material, saved, { ...material, id: "brief", operation: { ...material.operation, object: "创作简报", sourceId: "source-brief" } }]), /等/);
    assert.equal(operationIcon([material]), "read");
    assert.equal(operationIcon([{ ...material, operation: { ...material.operation, tool: "search_story_evidence" } }]), "search");
    assert.equal(operationIcon([{ ...material, operation: { ...material.operation, tool: "query_story_evidence" } }]), "search");
    assert.equal(operationIcon([material, { ...saved, status: "running" }]), "write", "Current operation should determine a mixed group's icon");
    for (const category of ["analysis", "arrange"]) assert.equal(operationIcon([{ ...material, operation: { ...material.operation, category } }]), category);
    assert.equal(operationIcon([{ id: "untyped", kind: "tool", title: "执行操作", status: "completed" }]), "operation");
    assert.equal(operationIcon([{ ...material, operation: { ...material.operation, category: "diagnostic" } }]), "operation");
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("isolated draft receipts stay distinct from manuscript saves through events and snapshots", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "conversation-isolated-candidate-"));
  try {
    await build({ entryPoints: Object.fromEntries(["operation-presentation", "conversation-value", "agent-events"]
      .map(name => [name, new URL(`../../src/${name}.ts`, import.meta.url).pathname])), outdir: directory,
      bundle: true, platform: "node", format: "esm", outExtension: { ".js": ".mjs" } });
    const get = name => import(pathToFileURL(path.join(directory, `${name}.mjs`)).href);
    const { operationPresentation: present, operationHeading } = await get("operation-presentation");
    const { operationSummary } = await get("conversation-value");
    const { createAgentRun, reduceAgentEvent: reduce, summarizeAgentRun } = await get("agent-events");
    const receipt = { candidateId: "candidate", writeState: "committed", state: "isolated", manuscriptChanged: false };
    const candidate = { tool: "save_delegated_draft", status: "completed", arguments: {
      payload: { chapterId: "chapter", chapterNumber: 7, body: "PRIVATE_CANDIDATE_BODY" } },
      result: { structuredContent: receipt } };
    assert.equal(present(candidate).outcome, "isolatedCandidate");
    assert.equal(operationHeading(present(candidate)), "已保存第 7 章候选稿，待整合");
    for (const damage of [
      { candidateId: undefined }, { writeState: "pending" }, { state: "adopted" }, { manuscriptChanged: true },
    ]) assert.equal(present({ ...candidate, result: { structuredContent: { ...receipt, ...damage } } }).outcome, undefined);
    assert.equal(present({ ...candidate, status: "running" }).outcome, undefined);
    assert.equal(present({ ...candidate, result: { ...candidate.result, isError: true } }).outcome, undefined);
    assert.equal(present({ ...candidate, result: { structuredContent: { ...receipt, replayed: true } } }).outcome, "isolatedCandidate",
      "An idempotent Native replay is still only the same isolated candidate");
    let run = { ...createAgentRun("修订章节"), threadId: "thread", turnId: "turn" };
    run = reduce(run, { method: "item/started", params: { threadId: "thread", turnId: "turn",
      item: { ...candidate, id: "candidate-save", type: "mcpToolCall", status: "running", result: undefined } } });
    run = reduce(run, { method: "item/completed", params: { threadId: "thread", turnId: "turn",
      item: { id: "candidate-save", type: "mcpToolCall", status: "completed", result: candidate.result } } });
    const step = run.steps.find(step => step.id === "candidate-save");
    assert.equal(operationSummary([step]), "已保存第 7 章候选稿，待整合");
    const snapshot = summarizeAgentRun({ ...run, state: "completed" }).displayExecution;
    assert.equal(snapshot.steps.find(step => step.id === "candidate-save").operation.outcome, "isolatedCandidate");
    assert.ok(!JSON.stringify(snapshot).includes("PRIVATE_CANDIDATE_BODY"));
    assert.equal(snapshot.steps.some(step => step.operation?.outcome === "saved"), false,
      "A completed child or isolated candidate never manufactures a manuscript save");
    const integrated = { tool: "save_draft_checkpoint", status: "completed", arguments: {
      delegatedCandidateId: "candidate", payload: { chapterId: "chapter", chapterNumber: 7 } }, result: {
      structuredContent: { chapterId: "chapter", revisionId: "revision", checkpointId: "checkpoint", version: 1 } } };
    assert.equal(operationHeading(present(integrated)), "已保存第 7 章正文",
      "Parent integration uses the existing formal checkpoint receipt");
    assert.equal(present(integrated).outcome, "saved");
    const listing = { tool: "read_delegated_draft", status: "completed", arguments: {}, result: {
      structuredContent: { state: "isolated", manuscriptChanged: false, candidates: [{ candidateId: "candidate" }] } } };
    assert.equal(operationHeading(present(listing)), "查看候选稿目录");
    assert.equal(present(listing).outcome, undefined, "A candidate listing is not returned prose");
    const page = { ...listing, arguments: { candidateId: "candidate", cursor: 1800 }, result: {
      structuredContent: { candidateId: "candidate", state: "isolated", manuscriptChanged: false,
        payloadPart: "PRIVATE_CANDIDATE_BODY", contentHash: "a".repeat(64), byteRange: { start: 1800, end: 1900, total: 2000 }, nextCursor: 1900 } } };
    assert.equal(operationHeading(present(page)), "读取候选稿片段");
    assert.equal(present(page).outcome, "read");
    assert.equal(present(page).sourceId, "candidate", "Distinct candidate branches must not fold as the same source");
    assert.notEqual(present(page).retryKey, present({ ...page, arguments: { candidateId: "candidate", cursor: 1900 } }).retryKey,
      "Another candidate page must not be treated as recovery of the first page");
    assert.equal(present({ ...page, arguments: { candidateId: "other" } }).outcome, undefined);
    assert.equal(present({ ...page, result: { structuredContent: { ...page.result.structuredContent, payloadPart: undefined } } }).outcome, undefined);
    assert.equal(present({ tool: "authorize_delegated_writer", status: "completed", result: { structuredContent: { state: "authorized" } } }).category, "diagnostic");
    const authorSource = { tool: "read_source_content", status: "completed", arguments: { sourceId: "author" }, result: {
      structuredContent: { sourceId: "author", sectionPath: "/authorRequirementsSource", contentRead: true } } };
    assert.equal(operationHeading(present(authorSource)), "读取作者要求片段");
    assert.equal(present({ ...authorSource, arguments: { sourceId: "author", view: "index" } }).outcome, undefined);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("actual report receipt fields project a save without promoting assessment, adoption or delivery", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "report-receipt-projection-"));
  try {
    await build({ entryPoints: Object.fromEntries(["operation-presentation", "conversation-value", "agent-events"]
      .map(name => [name, new URL(`../../src/${name}.ts`, import.meta.url).pathname])), outdir: directory,
      bundle: true, platform: "node", format: "esm", outExtension: { ".js": ".mjs" } });
    const get = name => import(pathToFileURL(path.join(directory, `${name}.mjs`)).href);
    const { operationPresentation: present, operationHeading } = await get("operation-presentation");
    const { operationSummary } = await get("conversation-value");
    const { createAgentRun, reduceAgentEvent: reduce, summarizeAgentRun } = await get("agent-events");
    // Current Native submit_revision_outcome response and current MCP recovery
    // proof shape. Report judgments are not part of the save receipt.
    const receipt = { revisionId: "revision-outcome-saved", writerTaskId: "writer-task", status: "candidate", recoveryOperation: {
      authority: "novelWorkspaceMcp", callId: "report-call", callerThreadId: "critic-thread", parentThreadId: null,
      rootThreadId: "critic-thread", runId: "critic-task", projectId: "project", turnId: "critic-turn",
      toolName: "submit_revision_outcome", requestHash: "a".repeat(64), phase: "runtimeLedger", outcome: "committed",
      operationId: "operation-report", receiptRef: "runtime:candidate:revision-outcome-saved",
    } };
    const item = { id: "report-call", type: "mcpToolCall", server: "novel_workspace", tool: "submit_revision_outcome",
      status: "completed", arguments: { projectId: "project", taskId: "critic-task", threadId: "critic-thread",
        writerTaskId: "writer-task", payload: { privateEvidence: "PRIVATE_REPORT_BODY" } }, result: { structuredContent: receipt } };
    assert.equal(present(item).outcome, "saved");
    assert.equal(present(item).referenceId, receipt.revisionId);
    assert.equal(operationHeading(present(item)), "已保存章节检查结果");
    assert.equal(present({ ...item, result: { content: [{ type: "text", text: JSON.stringify(receipt) }] } }).outcome, "saved",
      "The current MCP text representation carries the same real receipt");
    assert.equal(present({ ...item, result: { structuredContent: { ...receipt, replayed: true } } }).outcome, "saved");

    const invalidReceipts = [
      { ...receipt, revisionId: undefined }, { ...receipt, writerTaskId: undefined }, { ...receipt, writerTaskId: "other-writer" },
      { ...receipt, status: "supported" }, { ...receipt, recoveryOperation: undefined },
      ...[{ authority: "model" }, { callId: "other-call" }, { toolName: "save_draft_checkpoint" },
        { phase: "beforeDispatch" }, { outcome: "accepted" }, { outcome: "rejected" }, { outcome: "unknown" },
        { operationId: undefined }, { receiptRef: "runtime:candidate:other-report" }]
        .map(change => ({ ...receipt, recoveryOperation: { ...receipt.recoveryOperation, ...change } })),
    ];
    for (const invalid of invalidReceipts) assert.equal(present({ ...item, result: { structuredContent: invalid } }).outcome, undefined);
    for (const change of [{ id: "other-call" }, { server: "untrusted" }, { type: "dynamicToolCall" },
      { status: "running" }, { status: "cancelled" }, { success: false }, { error: { code: "failed" } },
      { arguments: { writerTaskId: "other-writer" } }, { arguments: {} }, { result: { ...item.result, isError: true } }]) {
      assert.equal(present({ ...item, ...change }).outcome, undefined, "An unconfirmed/error/foreign call is not a saved report");
    }
    const initial = { ...createAgentRun("检查已保存的章节"), threadId: "critic-thread", turnId: "critic-turn", taskId: "critic-task" };
    const event = (method, value) => ({ method, params: { threadId: "critic-thread", turnId: "critic-turn", item: value } });
    const started = reduce(initial, event("item/started", { ...item, status: "running", result: undefined }));
    const completion = { id: item.id, type: item.type, server: item.server, status: "completed", result: item.result };
    const completed = reduce(started, event("item/completed", completion));
    const step = completed.steps.find(value => value.id === item.id);
    assert.equal(step.title, "已保存章节检查结果", "Only the same call can restore its omitted writer argument");
    assert.equal(operationSummary([step]), "已保存章节检查结果");
    assert.equal(completed.state, initial.state, "A saved report is not final Work delivery");
    assert.equal(completed.taskId, initial.taskId);
    assert.deepEqual(completed.artifacts, initial.artifacts, "A report cannot manufacture an adopted or published chapter");
    assert.deepEqual(reduce(completed, event("item/completed", completion)), completed, "Repeated completion has no second effect");
    const snapshot = summarizeAgentRun({ ...completed, state: "completed" }).displayExecution;
    const retained = snapshot.steps.find(value => value.id === item.id);
    assert.equal(retained.operation.outcome, "saved");
    assert.equal(operationSummary([retained]), "已保存章节检查结果");
    assert.ok(!JSON.stringify(snapshot).includes("PRIVATE_REPORT_BODY"));

    for (const invalid of invalidReceipts) {
      const current = reduce(started, event("item/completed", { ...completion, result: { structuredContent: invalid } }));
      const currentStep = current.steps.find(value => value.id === item.id);
      assert.equal(currentStep.operation.outcome, undefined);
      assert.match(operationSummary([currentStep]), /保存结果待核实/);
    }
    for (const argumentsValue of [null, {}, { writerTaskId: "other-writer" }]) {
      const current = reduce(started, event("item/completed", { ...completion, arguments: argumentsValue }));
      assert.equal(current.steps.find(value => value.id === item.id).operation.outcome, undefined,
        "Explicit empty or replacement arguments cannot borrow the writer from start");
    }
    const foreignReceipt = { ...receipt, recoveryOperation: { ...receipt.recoveryOperation, callId: "other-call" } };
    const foreign = reduce(started, event("item/completed", { ...completion, id: "other-call", tool: item.tool,
      result: { structuredContent: foreignReceipt } }));
    assert.equal(foreign.steps.find(value => value.id === "other-call").operation.outcome, undefined);
    assert.equal(foreign.steps.find(value => value.id === "other-call").operation.reportWriterTaskId, undefined,
      "A different call cannot inherit an earlier writer binding even with an otherwise committed proof");
    const changedTool = reduce(started, event("item/completed", { ...completion, tool: "save_brief" }));
    assert.equal(changedTool.steps.find(value => value.id === item.id).operation.reportWriterTaskId, undefined);
    assert.equal(changedTool.steps.find(value => value.id === item.id).operation.outcome, undefined);
    const failed = reduce(started, event("item/completed", { ...completion, result: { ...item.result, isError: true } }));
    assert.equal(failed.steps.find(value => value.id === item.id).status, "failed");
    assert.equal(failed.steps.find(value => value.id === item.id).operation.outcome, undefined);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

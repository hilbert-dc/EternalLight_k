import { createMcpAuthorityFixture } from "./helpers/mcp-authority.mjs";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import test from "node:test";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";
import { HostRecoveryJournal, RecoveryIndex } from "@eternallight/agent-engine";
import { createNovelRecoveryOperationContract, novelRecoveryOperationHash } from "../novel-recovery-contract.mjs";

// Production Runtime and MCP, temporary current-schema storage. RPC metadata
// models the already-tested trusted Engine boundary; no UI or real provider.
test("production MCP and Runtime isolate child drafts and integrate exact immutable references", { timeout: 120000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-native-delegation-"));
  const author = "delegation-author-fixture-123456789";
  const candidate = "delegation-candidate-fixture-123456789";
  const capability = "delegation-capability-fixture-123456789";
  const rootId = randomUUID(), childId = randomUUID(), otherChild = randomUUID();
  const children = [];
  t.after(async () => {
    for (const child of children.reverse()) {
      if (child.exitCode === null && child.signalCode === null) {
        const stopped = new Promise(resolve => child.once("exit", resolve));
        child.kill(); await stopped;
      }
    }
    await rm(directory, { recursive: true, force: true });
  });
  const runtime = spawn(nativeTestBinary("novel-codex-story-runtime"), ["--port", "0", "--db", join(directory, "story.sqlite3"),
    "--engine-budget-db", join(directory, "engine.sqlite3"), "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
  children.push(runtime); runtime.stderr.resume();
  const origin = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("Production Runtime startup timed out")), 90000);
    const finish = (error, value) => { clearTimeout(timer); error ? reject(error) : resolve(value); };
    createInterface({ input: runtime.stdout }).on("line", line => {
      if (line.startsWith("Story Runtime listening on ")) finish(null, line.slice("Story Runtime listening on ".length));
    });
    runtime.once("error", finish); runtime.once("exit", () => finish(new Error("Runtime exited")));
    runtime.stdin.end(`${JSON.stringify({ authorToken: author, candidateToken: candidate })}\n`);
  });
  const request = async (method, path, body, actor = author, headers = {}) => {
    const response = await fetch(`${origin}${path}`, { method, headers: { authorization: `Bearer ${actor}`, "content-type": "application/json",
      ...(actor === candidate ? { "x-novel-task-capability": capability } : {}), ...headers },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
    const raw = await response.text();
    assert.match(response.headers.get("content-type") ?? "", /^application\/json\b/, `${method} ${path}: HTTP ${response.status}: ${raw.slice(0, 2000)}`);
    const value = JSON.parse(raw); return { status: response.status, value };
  };
  const authorRequest = async (method, path, body) => {
    const result = await request(method, path, body); assert.ok(result.status < 300, `${method} ${path}: ${JSON.stringify(result)}`); return result.value;
  };
  const { project } = await authorRequest("POST", "/api/projects", { title: "临时隔离候选验收", logline: "独立子智能体候选和父任务整合验收", medium: "novel", idempotencyKey: randomUUID() });
  const base = `/api/projects/${project.id}`;
  const originalPayload = { chapterId: "chapter-one", chapterNumber: 1, title: "密函", completeness: "chapterComplete",
    body: "林夕烧掉密函。\n\n---\n\n白塔熄灭。",
    sceneGroups: [{ id: "letter", sceneId: null, body: "林夕烧掉密函。", locked: false },
      { id: "ending", sceneId: null, body: "白塔熄灭。", locked: true }] };
  const original = await authorRequest("POST", `${base}/drafts/checkpoints`, {
    expectedRevision: 0, expectedCandidateRevisionId: null, idempotencyKey: "original", payload: originalPayload });
  const createTask = async (threadId, scope = { type: "wholeChapter" }) => {
    const workspace = await authorRequest("GET", `${base}/creative-workspace`);
    return authorRequest("POST", `${base}/tasks`, { operation: "generateChapter", threadId, inputRevision: 0,
      target: { type: "chapter", chapterId: "chapter-one", chapterNumber: 1, readingChapter: 1, sceneOrder: 1, scope },
      authorIntent: "把烧信改为藏信，保留锁定结尾。", capabilityHash: createHash("sha256").update(threadId === rootId ? capability : `other-${capability}`).digest("hex"),
      capabilityExpiresAt: Math.floor(Date.now() / 1000) + 600,
      initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: workspace.headSnapshotId,
        expectedSourceDigest: workspace.sourceDigest, maxRequests: 48, maxTokens: 1000000, deadlineAt: Math.floor(Date.now() / 1000) + 600 } });
  };
  const task = await createTask(rootId);
  const authority=await createMcpAuthorityFixture(t,capability);
  const mcp = spawn(nativeTestBinary("novel-workspace-mcp"), [], { env: { ...authority.env, PATH: process.env.PATH,
    NOVEL_CODEX_RUNTIME_URL: origin, NOVEL_CODEX_CANDIDATE_TOKEN: candidate,
    NOVEL_CODEX_TASK_CAPABILITY: capability, NOVEL_CODEX_OPERATION: "generateChapter" }, stdio: ["pipe", "pipe", "pipe"] });
  children.push(mcp);
  let mcpErrors = "";
  mcp.stderr.on("data", chunk => { mcpErrors = (mcpErrors + chunk).slice(-2000); });
  const waiting = new Map(); let nextId = 1;
  createInterface({ input: mcp.stdout }).on("line", line => {
    const reply = JSON.parse(line), pending = waiting.get(reply.id);
    if (pending) { clearTimeout(pending.timer); waiting.delete(reply.id); pending.resolve(reply); }
  });
  const rpc = (method, params = {}) => new Promise((resolve, reject) => {
    const id = nextId++, timer = setTimeout(() => { waiting.delete(id); reject(new Error(`${params.name ?? method} id=${id} timed out; MCP stderr=${mcpErrors}`)); }, 10000);
    waiting.set(id, { resolve, timer }); mcp.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
  });
  const tools = (await rpc("tools/list")).result.tools;
  for (const name of ["save_delegated_draft", "read_delegated_draft", "save_draft_checkpoint", "read_source_content"]) {
    assert.ok(tools.some(tool => tool.name === name), name);
  }
  assert.ok(!tools.some(tool => tool.name === "authorize_delegated_writer"));
  assert.equal(tools.find(tool => tool.name === "save_draft_checkpoint").inputSchema.oneOf.length, 3);
  const binding = { taskId: task.id, projectId: project.id, threadId: rootId, expectedRevision: 0 };
  const index=new RecoveryIndex(join(directory,"recovery.json"));
  const journal=new HostRecoveryJournal(index,createNovelRecoveryOperationContract({getBinding:()=>({runId:task.id,projectId:project.id,threadId:rootId})}));
  await journal.beginRun({runId:task.id,projectId:project.id,threadId:rootId,planId:"fixture"});
  const nonRootEventCallIds=[];
  const call = async (name, extra = {}, caller = rootId, override = {}, metaOverride = {}) => {
    const bound = { ...binding, ...override };
    const arguments_={...(tools.find(tool => tool.name === name)?.inputSchema.properties.expectedRevision ? {expectedRevision:bound.expectedRevision} : {}),...override,...extra}, callId=`call-${nextId}`;
    if(caller!==rootId)nonRootEventCallIds.push(callId);
    const item={id:callId,type:"mcpToolCall",server:"novel_workspace",tool:name,arguments:arguments_};
    await journal.observe({method:"item/started",params:{threadId:caller,turnId:"fixture-turn",item:{...item,status:"inProgress"}}});
    const meta={threadId:caller,sessionId:bound.threadId,rootThreadId:bound.threadId,callerThreadId:caller,
      ...(caller===bound.threadId?{}:{parentThreadId:bound.threadId,agentRole:caller===childId?"candidate_writer":"explorer"}),turnId:"fixture-turn",callId,...metaOverride};
    await authority.admit(name,arguments_,meta,tools);
    const result=await rpc("tools/call",{name,arguments:arguments_,_meta:meta});
    assert.equal(result.error, undefined);
    const proof=result.result.structuredContent?.recoveryOperation;
    if(proof) assert.equal(proof.requestHash,novelRecoveryOperationHash(item),"Host Native canonical identity must equal production MCP identity");
    // Current app-server wire omits isError from result and reports failed status.
    const {isError,...wireResult}=result.result;
    await journal.observe({method:"item/completed",params:{threadId:caller,turnId:"fixture-turn",item:{...item,status:isError?"failed":"completed",result:wireResult}}});
    const text = result.result.content.map(item => item.text).join("");
    return { error: result.result.isError === true, text, proof,callId, value: text.startsWith("{") ? JSON.parse(text) : null };
  };
  const ok = async (...args) => { const result = await call(...args); assert.equal(result.error, false, result.text); return result.value; };
  const denied = async (...args) => { const result = await call(...args); assert.equal(result.error, true, `${args[0]} should be rejected: ${result.text}`); return result; };

  const method = await ok("read_creative_strategy", { strategyId: "expert_investigation" });
  assert.equal(method.status, "unvalidated");
  assert.equal(method.authority, "advisoryMethodOnly");
  assert.equal(method.content.examples.length, 3, "Current bundled MCP must expose the optional diagnostic examples");
  const context = await ok("read_context", {}, childId);
  assert.equal(context.executionIdentity.callerThreadId, childId);
  const workspaceSource = context.taskWorkspace.sourceId;
  const directoryPage = await ok("read_source_content", { sourceId: workspaceSource, view: "workspace", chapterNumber: 1 }, childId);
  const entries = directoryPage.items;
  assert.ok(entries?.length, JSON.stringify(directoryPage));
  const sourceId = entries[0].sourceId;
  let cursor, pages = 0, sourceText = "";
  do {
    const page = await ok("read_source_content", { sourceId, ...(cursor ? { cursor } : {}) }, childId);
    pages++; sourceText += JSON.stringify(page); cursor = page.nextRead?.arguments.cursor;
  } while (cursor);
  assert.ok(pages > 0); assert.match(sourceText, /烧掉密函/);
  const firstPayload = { chapterId: "chapter-one", chapterNumber: 1, title: "密函", completeness: "chapterComplete",
    sceneGroups: [{ ...originalPayload.sceneGroups[0], body: "林夕将密函藏入衣襟。" }, originalPayload.sceneGroups[1]] };
  const save = { payload: firstPayload, expectedDelegatedCandidateId: null, idempotencyKey: "child-first" };
  await denied("save_delegated_draft", save);
  await denied("save_delegated_draft", { ...save, agentRole: "candidate_writer" }, otherChild);
  await denied("save_delegated_draft", save, childId, {}, { agentRole: null });
  await denied("save_delegated_draft", save, childId, {}, { agentRole: "explorer" });
  await denied("save_delegated_draft", save, childId, {}, { parentThreadId: otherChild });
  await denied("save_delegated_draft", save, childId, {}, { agentRole: "candidate_writer\r\nX-Injected: bad" });
  const first = await ok("save_delegated_draft", save, childId);
  assert.equal(first.state, "isolated"); assert.equal(first.manuscriptChanged, false);
  assert.equal((await ok("save_delegated_draft", save, childId)).candidateId, first.candidateId);
  assert.equal((await authorRequest("GET", `${base}/creative-workspace`)).chapters[0].candidate.id, original.revisionId);
  await denied("save_delegated_draft", save, otherChild);
  await denied("read_delegated_draft", { candidateId: first.candidateId }, otherChild);
  await denied("save_draft_checkpoint", { delegatedCandidateId: first.candidateId, expectedCandidateRevisionId: original.revisionId, idempotencyKey: "child-formal" }, childId);
  await denied("complete_task", { summary: "child cannot complete parent" }, childId);

  const changedLocked = structuredClone(firstPayload); changedLocked.sceneGroups[1].body = "修改锁定结尾。";
  await denied("save_delegated_draft", { payload: changedLocked, expectedDelegatedCandidateId: first.candidateId, idempotencyKey: "locked-change" }, childId);
  const nextPayload = structuredClone(firstPayload); nextPayload.sceneGroups[0].body = "林夕将密函藏入夹层。";
  await denied("save_delegated_draft", { payload: nextPayload, expectedDelegatedCandidateId: null, idempotencyKey: "stale-child-cas" }, childId);
  const second = await ok("save_delegated_draft", { payload: nextPayload, expectedDelegatedCandidateId: first.candidateId, idempotencyKey: "child-next" }, childId);
  assert.notEqual(second.candidateId, first.candidateId);
  const frozen = await ok("read_delegated_draft", { candidateId: first.candidateId });
  assert.match(frozen.payloadPart, /藏入衣襟/); assert.doesNotMatch(frozen.payloadPart, /藏入夹层/);
  const badArguments=await denied("save_draft_checkpoint", { delegatedCandidateId: first.candidateId, payload: firstPayload, expectedCandidateRevisionId: original.revisionId, idempotencyKey: "ambiguous" });
  assert.equal(badArguments.proof?.phase,"beforeDispatch");assert.equal(badArguments.proof?.outcome,"rejected");
  await denied("save_draft_checkpoint", { delegatedCandidateId: first.candidateId, expectedCandidateRevisionId: "wrong-revision", idempotencyKey: "stale-formal-cas" });
  const formalResult=await call("save_draft_checkpoint", { delegatedCandidateId: first.candidateId, expectedCandidateRevisionId: original.revisionId, idempotencyKey: "integrate-first" });
  assert.equal(formalResult.error,false,formalResult.text);
  assert.equal(formalResult.proof?.phase,"runtimeLedger");assert.equal(formalResult.proof?.outcome,"committed");
  const formal=formalResult.value;
  assert.equal(formal.saved, true); assert.ok(formal.revisionId); assert.ok(formal.checkpointId);
  const saved = (await authorRequest("GET", `${base}/creative-workspace`)).chapters[0].candidate;
  assert.equal(saved.id, formal.revisionId); assert.match(saved.body, /藏入衣襟/); assert.doesNotMatch(saved.body, /藏入夹层/);
  assert.deepEqual(saved.sceneGroups[1], originalPayload.sceneGroups[1]);
  const taskAfter = await authorRequest("GET", `${base}/tasks/${task.id}`);
  assert.equal(taskAfter.outputRefs.length, 1); assert.equal(taskAfter.outputRefs[0].revisionId, formal.revisionId);
  const patchRequest = { patch: { chapterId: "chapter-one", sourceRevisionId: formal.revisionId,
    edits: [{ groupId: "letter", start: 0, end: Array.from(firstPayload.sceneGroups[0].body).length,
      oldText: firstPayload.sceneGroups[0].body, newText: nextPayload.sceneGroups[0].body }] },
    expectedCandidateRevisionId: formal.revisionId, idempotencyKey: "root-local-patch" };
  const badPatch = structuredClone(patchRequest); badPatch.patch.edits[0].oldText = "不匹配的原文";
  const rejectedPatch = await denied("save_draft_checkpoint", badPatch);
  assert.equal(rejectedPatch.proof?.outcome, "rejected", "An exact-anchor rejection must not leave an unknown write fence");
  const patched = await call("save_draft_checkpoint", patchRequest);
  assert.equal(patched.error, false, patched.text); assert.equal(patched.proof?.outcome, "committed");
  assert.equal((await ok("save_draft_checkpoint", patchRequest)).revisionId, patched.value.revisionId);
  const afterPatch = (await authorRequest("GET", `${base}/creative-workspace`)).chapters[0].candidate;
  assert.equal(afterPatch.body, `${nextPayload.sceneGroups[0].body}\n\n---\n\n${originalPayload.sceneGroups[1].body}`);
  assert.deepEqual(afterPatch.sceneGroups[1], originalPayload.sceneGroups[1]);
  const different = await createTask(randomUUID());
  await denied("read_delegated_draft", { candidateId: first.candidateId }, different.threadId, { taskId: different.id, threadId: different.threadId });
  await denied("save_draft_checkpoint", { delegatedCandidateId: first.candidateId, expectedCandidateRevisionId: formal.revisionId, idempotencyKey: "cross-task" }, different.threadId, { taskId: different.id, threadId: different.threadId });
  const ordinary = await ok("save_draft_checkpoint", { payload: nextPayload, expectedCandidateRevisionId: patched.value.revisionId, idempotencyKey: "ordinary-payload" });
  assert.equal(ordinary.saved, true, "Normal payload saves must remain supported without delegatedCandidateId");
  assert.match((await authorRequest("GET", `${base}/creative-workspace`)).chapters[0].candidate.body, /藏入夹层/);
  const notebook = await ok("read_quality_notebook", { before: null });
  assert.ok(notebook && typeof notebook === "object", "Default quality-note page uses the declared required nullable before");
  await journal.flush();await journal.abandonRun();
  const requests=(await index.read()).runs[task.id].requests;
  for(const result of [formalResult,patched])assert.equal(requests[`mcp:${result.callId}`].outcome,"committed");
  assert.equal(requests[`mcp:${badArguments.callId}`].outcome,"rejected","Failed current app-server result consumes the bound Native beforeDispatch proof");
  assert.ok(nonRootEventCallIds.length>0);
  for(const callId of nonRootEventCallIds)assert.equal(requests[`mcp:${callId}`],undefined,"Actual child and foreign-root events never create bound root requests");
});

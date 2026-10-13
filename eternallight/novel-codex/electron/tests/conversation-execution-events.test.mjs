import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";

async function load() {
  const directory = await mkdtemp(join(tmpdir(), "novel-decision-events-"));
  await build({ entryPoints: { events: new URL("../../src/conversation-execution-events.ts", import.meta.url).pathname,
    run: new URL("../../src/agent-events.ts", import.meta.url).pathname }, outdir: directory,
    outExtension: { ".js": ".mjs" }, bundle: true, format: "esm", platform: "node" });
  return { directory, ...await import(pathToFileURL(join(directory, "events.mjs")).href),
    ...await import(pathToFileURL(join(directory, "run.mjs")).href) };
}
const rootTask = { id: "root-author", threadId: "root-thread", projectId: "project", operation: "advisory", target: {} };
const workflow = (task, executionBinding) => ({ method: "agent/workflow", params: { workflow: { task, ...(executionBinding ? { executionBinding } : {}) } } });
function decision(id = "decision-1", stepId = "decision-step-1") {
  const frozen = { workTaskId: "work-root", rootThreadId: "root-thread", originTaskId: rootTask.id,
    decisionStepId: stepId, decisionTaskId: id, sourceTaskId: "first-writer" };
  return workflow({ id, threadId: "root-thread", projectId: "project", operation: "advisory",
    target: { workDecision: { workTaskId: frozen.workTaskId, originTaskId: frozen.originTaskId, sourceTaskId: frozen.sourceTaskId } },
    inputManifest: { workDecision: frozen } }, { ...frozen, stepId, taskId: id, actionKey: null });
}
function action(id = "check-1", operation = "assessSceneOutcome") {
  const frozen = { workTaskId: "work-root", rootThreadId: "root-thread", originTaskId: rootTask.id,
    decisionStepId: "decision-step-1", decisionTaskId: "decision-1", actionKey: "check-exact-saved-draft", operation,
    sourceTaskId: "historical-writer", sourceRevisionId: "saved-draft" };
  const declared = { workTaskId: frozen.workTaskId, decisionStepId: frozen.decisionStepId,
    decisionTaskId: frozen.decisionTaskId, actionKey: frozen.actionKey };
  return workflow({ id, threadId: `worker-${id}`, projectId: "project", operation,
    target: { workDecisionAction: declared }, inputManifest: { workDecisionAction: frozen } },
  { ...declared, rootThreadId: frozen.rootThreadId, originTaskId: frozen.originTaskId, taskId: id, stepId: `step-${id}` });
}

test("Runtime-admitted decisions keep one goal, restore truthful running state and preserve prior failure", async () => {
  const m = await load();
  try {
    const owner = m.createConversationExecutionOwner("project");
    let run = m.createAgentRun("改稿并核对故事衔接");
    run = m.reduceAgentEvent(m.applyConversationExecutionEvent(run, workflow(rootTask), owner), workflow(rootTask));
    run = { ...run, state: "completed" };
    run = m.applyConversationExecutionEvent(run, workflow({ id: "first-writer", threadId: "writer", projectId: "project",
      operation: "generateChapter", target: { requestedByTaskId: rootTask.id, requestedActionId: "author-action" } }), owner);
    run = { ...run, state: "error", error: "首次工具参数被拒绝" };
    run = m.applyConversationExecutionEvent(run, decision(), owner);
    assert.deepEqual({ taskId: run.taskId, threadId: run.threadId, state: run.state, error: run.error, goal: run.prompt,
      failedHistory: run.history.filter(item => item.state === "error").length },
    { taskId: "decision-1", threadId: "root-thread", state: "running", error: null, goal: "改稿并核对故事衔接", failedHistory: 1 });
    run = { ...run, state: "completed" };
    run = m.applyConversationExecutionEvent(run, action(), owner);
    assert.equal(run.taskId, "check-1", "Admitted assessment does not rely on the old candidate operation list");
    const before = run;
    assert.equal(m.applyConversationExecutionEvent(run, action(), owner), before, "Duplicate workflow must not archive the current task");
    run = { ...run, state: "completed", answer: "修订已保存，检查范围明确。" };
    run = m.applyConversationExecutionEvent(run, decision("decision-2", "decision-step-2"), owner);
    assert.equal(run.taskId, "decision-2");
    assert.equal(run.history.filter(item => item.taskId === "check-1").length, 1);
  } finally { await rm(m.directory, { recursive: true, force: true }); }
});

test("Unrelated Work, root thread, action or origin cannot rebind; cancelled execution cannot revive", async () => {
  const m = await load();
  try {
    const owner = m.createConversationExecutionOwner("project", { taskId: rootTask.id, threadId: rootTask.threadId });
    let run = m.applyConversationExecutionEvent(m.createAgentRun("续写两章"), decision(), owner);
    run = { ...run, state: "completed" };
    for (const edit of [
      event => { event.params.workflow.executionBinding.workTaskId = "work-other"; },
      event => { event.params.workflow.executionBinding.rootThreadId = "other-thread"; },
      event => { event.params.workflow.executionBinding.originTaskId = "other-author"; },
      event => { event.params.workflow.executionBinding.decisionTaskId = "unknown-decision"; },
      event => { event.params.workflow.task.target.workDecisionAction.actionKey = "other-action"; },
      event => { event.params.workflow.task.projectId = "project-other"; },
      event => { delete event.params.workflow.executionBinding; },
    ]) {
      const event = action(); edit(event);
      assert.equal(m.applyConversationExecutionEvent(run, event, owner), run);
    }
    const cancelled = { ...run, state: "cancelled" };
    assert.equal(m.applyConversationExecutionEvent(cancelled, action(), owner), cancelled);
    const running = { ...run, state: "running" };
    assert.equal(m.applyConversationExecutionEvent(running, action(), owner), running, "A new workflow cannot discard an unterminated execution");
    assert.equal(owner.workers.size, 0);
  } finally { await rm(m.directory, { recursive: true, force: true }); }
});

test("actual tool events project meaningful actions and keep status plumbing diagnostic", async () => {
  const m = await load();
  try {
    let run = m.reduceAgentEvent(m.createAgentRun("继续创作"), { method: "turn/started", params: { threadId: "thread", turn: { id: "turn" } } });
    for (const [tool, title, category] of [["read_source_content", "读取写作资料片段", "read"], ["read_work_status", "内部操作", "diagnostic"], ["request_work_action", "安排下一步", "arrange"], ["request_generation_handoff", "准备续写安排", "arrange"]]) {
      run = m.reduceAgentEvent(run, { method: "item/started", params: { threadId: "thread", turnId: "turn", item: { id: tool, type: "mcpToolCall", tool, status: "running" } } });
      const step = run.steps.find(item => item.id === tool);
      assert.equal(step.title, title);
      assert.equal(step.operation.category, category);
      assert.equal(step.operation.tool, tool);
    }
  } finally { await rm(m.directory, { recursive: true, force: true }); }
});

test("resume initializes only exact persisted Work origin and binds an already queued decision", async () => {
  const m = await load();
  try {
    const work = { id: "work-root", projectId: "project", threadId: "root-thread", kind: "conversation",
      steps: [{ id: "root-step", operation: "advisory", target: {}, attempts: [{ id: rootTask.id, threadId: rootTask.threadId }] },
        { id: "decision-step-1", operation: "advisory", target: { workDecision: { workTaskId: "work-root", originTaskId: rootTask.id } }, attempts: [{ id: "decision-1", threadId: "root-thread" }] }] };
    const owner = m.createRecoveredExecutionOwner(work);
    const run = m.applyConversationExecutionEvent(m.createAgentRun("原始目标"), decision(), owner);
    assert.deepEqual({ taskId: run.taskId, threadId: run.threadId, work: owner.workTaskId }, { taskId: "decision-1", threadId: "root-thread", work: "work-root" });
    const unstarted = { ...work, kind: "generation", threadId: "", steps: [{ id: "first", operation: "advisory", target: {}, attempts: [], currentAttemptId: null }] };
    const waitingOwner = m.createRecoveredExecutionOwner(unstarted);
    const waiting = m.createAgentRun("等待首次真实任务");
    assert.equal(m.applyConversationExecutionEvent(waiting, workflow({ ...rootTask, id: "unrelated" }), waitingOwner), waiting,
      "An unstarted persisted Work must not accept a same-project advisory without Native binding");
    assert.deepEqual({ rootThread: waitingOwner.originalThreadId, origin: waitingOwner.originTaskId, parents: [...waitingOwner.parents] },
      { rootThread: null, origin: null, parents: [] });
    const firstDecision = decision(); firstDecision.params.workflow.task.target.workDecision.originTaskId = null;
    firstDecision.params.workflow.task.inputManifest.workDecision.originTaskId = null;
    firstDecision.params.workflow.executionBinding.originTaskId = null;
    assert.equal(m.applyConversationExecutionEvent(waiting, firstDecision, waitingOwner).taskId, "decision-1");
    assert.equal(waitingOwner.originalThreadId, "root-thread");
    const conflicting = structuredClone(work); conflicting.steps[1].target.workDecision.workTaskId = "another-work";
    assert.equal(m.createRecoveredExecutionOwner(conflicting), null);
    const unknown = structuredClone(work); unknown.steps[1].target.workDecision.originTaskId = undefined;
    assert.equal(m.createRecoveredExecutionOwner(unknown), null);
  } finally { await rm(m.directory, { recursive: true, force: true }); }
});

test("background Work continuation streams in its original conversation without a send pipeline", async () => {
  const m = await load();
  try {
    const event = decision();
    const task = event.params.workflow.task;
    const work = { id:'work-root',projectId:'project',threadId:'root-thread',kind:'generation',status:'running',goal:'继续写作',steps:[{
      id:'decision-step-1',operation:'advisory',target:task.target,currentAttemptId:task.id,
      attempts:[{id:task.id,threadId:task.threadId,turnId:null}],
    }] };
    const prior = {...m.createAgentRun('原作者要求'),state:'completed',taskId:'previous',threadId:'root-thread',answer:'前一步完成'};
    const attached = m.applyBackgroundExecutionEvent(prior,event,[work],'project','root-thread');
    assert.equal(attached.rootThreadId,'root-thread');
    assert.equal(attached.run.taskId,'decision-1'); assert.equal(attached.run.state,'running');
    assert.equal(attached.run.history.filter(item=>item.taskId==='previous').length,1);
    assert.equal(m.applyBackgroundExecutionEvent(attached.run,event,[work],'project','root-thread'),null);
    let streamed = m.reduceAgentEvent(attached.run, { method: 'turn/started', params: { threadId: 'root-thread', turn: { id: 'background-turn' } } });
    const scope = { threadId: 'root-thread', turnId: 'background-turn' };
    streamed = m.reduceAgentEvent(streamed, { method: 'item/started', params: { ...scope, item: { id: 'progress', type: 'agentMessage', phase: 'commentary', text: '' } } });
    for (const delta of ['已读回保存的正文。', '正在检查前后衔接。']) {
      streamed = m.reduceAgentEvent(streamed, { method: 'item/agentMessage/delta', params: { ...scope, itemId: 'progress', delta } });
    }
    assert.equal(streamed.steps.find(step => step.id === 'message:progress').text, '已读回保存的正文。正在检查前后衔接。');
    assert.equal(streamed.steps.find(step => step.id === 'message:progress').messagePhase, 'commentary');
    for (const status of ['running', 'completed']) {
      streamed = m.reduceAgentEvent(streamed, { method: status === 'running' ? 'item/started' : 'item/completed', params: { ...scope, item: { id: 'read-source', type: 'mcpToolCall', tool: 'read_source_content', status } } });
      assert.equal(streamed.steps.find(step => step.id === 'read-source').status, status);
    }
    assert.equal(m.reduceAgentEvent(streamed, { method: 'item/agentMessage/delta', params: { ...scope, threadId: 'other-thread', itemId: 'progress', delta: '无关内容' } }), streamed);
    assert.equal(streamed.history.filter(item => item.taskId === 'previous').length, 1);
    for (const changed of [{projectId:'other'},{threadId:'other'},{status:'paused'},{status:'cancelled'}]) {
      assert.equal(m.applyBackgroundExecutionEvent(prior,event,[{...work,...changed}],'project','root-thread'),null);
    }
    assert.equal(m.applyBackgroundExecutionEvent(prior,event,[work],'project','other-thread'),null);
    assert.equal(m.applyBackgroundExecutionEvent({...prior,state:'cancelled'},event,[work],'project','root-thread'),null);
    const forged = structuredClone(event); forged.params.workflow.executionBinding.originTaskId='other-author';
    assert.equal(m.applyBackgroundExecutionEvent(prior,forged,[work],'project','root-thread'),null);
  } finally { await rm(m.directory,{recursive:true,force:true}); }
});

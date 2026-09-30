import assert from "node:assert/strict";
import { test } from "node:test";
import { ModelingConsumer } from "../modeling-consumer.mjs";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";
import { fileURLToPath } from "node:url";
import { withFrozenMethods } from "./fixtures/frozen-methods.mjs";

function modelingTask(body, work) {
  const task = { ...body, id: "task-auto", projectId: "project", purpose: "critic",
    inputManifest: { domainInstruction: "从绑定章节提取候选知识，不修改正文或确认 Canon。", domainTaskSource: {
      type: "modelingOutbox", workTaskId: work.id, stepId: work.steps[0].id,
      batchId: body.target.batchId, chapterId: body.target.chapterId, objectId: `draft-${body.target.chapterId}`,
      sourceRevisionId: "revision-one", intentGeneration: 1,
    } } };
  withFrozenMethods(task, "project");
  return task;
}

test("automatic consumption stays idle while dirty or unavailable and serializes wakeups", async () => {
  let available = false;
  let release;
  const calls = [];
  const consumer = new ModelingConsumer({
    available: () => available,
    dispatch: async (project) => { calls.push(project); await new Promise((resolve) => { release = resolve; }); return { action: { workTaskId: "work-current" } }; },
    run: async (context, id) => calls.push([context.projectId, id]),
    failed: async () => assert.fail("unexpected failure"), updated: () => {},
  });
  consumer.select({ projectId: "first", dirty: false });
  await consumer.tick(); available = true;
  consumer.select({ projectId: "first", dirty: true });await consumer.tick();assert.deepEqual(calls, []);
  consumer.select({ projectId: "first", dirty: false });const pending = consumer.tick();await consumer.tick();
  consumer.select({ projectId: "second", dirty: false });release();await pending;
  assert.deepEqual(calls, ["first"]);
  const next = consumer.tick();release();await next;
  assert.deepEqual(calls, ["first", "second", ["second", "work-current"]]);consumer.stop();
});

test("a failed automatic workflow is held for author control, not retried by the timer", async () => {
  const calls = [];
  const consumer = new ModelingConsumer({available:()=>true,dispatch:async()=>({action:{workTaskId:"work-failed"}}),run:async()=>{calls.push("run");throw new Error("authentication failed");},failed:async(context,id,error)=>calls.push([context.projectId,id,error.message]),updated:()=>calls.push("updated")});
  consumer.select({projectId:"current",dirty:false});await consumer.tick();await consumer.tick();
  assert.deepEqual(calls,["run",["current","work-failed","authentication failed"],"updated"]);consumer.stop();
});

test("production automatic dispatch reuses the bound task runner and completion ledger", async () => {
  const requests=[];
  const deadlineAt = Math.floor(Date.now() / 1000) + 120;
  const work={id:"work-auto",projectId:"project",kind:"direct",goal:"自动分析章节知识",status:"running",version:0,steps:[{id:"step-auto",operation:"modelKnowledge",status:"queued",target:{batchId:"batch-auto",chapterId:"chapter-1",modelingIntent:true}}]};
  const command=`${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs",import.meta.url))}`;
  const {handlers,adapter,modelingConsumer}=await mainHandlers(async(path,body)=>{
    requests.push({path,body});
    if(path.endsWith("/dispatch"))return {action:{workTaskId:work.id}};
    if(path.endsWith("/budget-authorization"))return {authorization:{deadlineAt}};
    if(path.endsWith("/provider-budget"))return {taskId:"task-auto",workTaskId:work.id,budgetId:"budget-task-auto",maxRequests:3,maxTokens:700,deadlineAt};
    if(path.endsWith("/provider-usage")&&!body)return {pending:null};
    if(path.endsWith("/work-tasks/work-auto")||path.endsWith("/work-task")||path.endsWith("/execution-scope/continue"))return work;
    if(path.endsWith("/creative-workspace"))return {revision:0};
    if(path.endsWith("/tasks")){work.steps[0].status="running";work.steps[0].currentAttemptId="task-auto";return modelingTask(body, work);}
    if(path.endsWith("/complete")){work.status="completed";work.steps[0].status="completed";return {status:"completed"};}
    return {};
  },command);
  const event=rendererEvent();event.sender.getURL=()=>"http://127.0.0.1:5173/";
  try{
    assert.throws(()=>handlers.get("agent:set-modeling-context")(event,{projectId:"project",dirty:"no"}),/上下文无效/);
    event.sender.getURL=()=>"https://untrusted.example/";
    await handlers.get("agent:set-modeling-context")(event,{projectId:"project",dirty:false});await modelingConsumer.tick();assert.equal(requests.length,0);
    event.sender.getURL=()=>"http://127.0.0.1:5173/";
    await handlers.get("agent:set-modeling-context")(event,{projectId:"project",dirty:true});await modelingConsumer.tick();assert.equal(requests.length,0);
    await handlers.get("agent:set-modeling-context")(event,{projectId:"project",dirty:false});
    await modelingConsumer.tick();
    const claim=requests.find(({path})=>path.endsWith("/tasks"));
    assert.deepEqual(JSON.parse(JSON.stringify(claim.body.target)),{...work.steps[0].target,resumeStepId:"step-auto"});
    assert.equal(claim.body.operation,"modelKnowledge");
    assert.equal(requests.filter(({path})=>path.endsWith("/complete")).length,1);
    assert.equal(requests.filter(({path})=>path.endsWith("/modeling-batches")).length,0);
    assert.equal(work.status,"completed");
  }finally{modelingConsumer.stop();adapter.close();}
});

test("automatic dispatch cannot resume a task paused between dispatch and execution", async () => {
  const requests=[];
  const command=`${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs",import.meta.url))}`;
  const {handlers,adapter,modelingConsumer}=await mainHandlers(async(path)=>{
    requests.push(path);
    if(path.endsWith("/dispatch"))return {action:{workTaskId:"work-auto"}};
    if(path.endsWith("/work-tasks/work-auto"))return {id:"work-auto",status:"paused",kind:"direct",steps:[]};
    throw new Error(`unexpected request: ${path}`);
  },command);
  const event=rendererEvent();event.sender.getURL=()=>"http://127.0.0.1:5173/";
  try{
    await handlers.get("agent:set-modeling-context")(event,{projectId:"project",dirty:false});await modelingConsumer.tick();
    assert.deepEqual(requests,["/api/projects/project/modeling-outbox/dispatch","/api/projects/project/work-tasks/work-auto"]);
  }finally{modelingConsumer.stop();adapter.close();}
});

test("unfunded automatic work pauses, then author confirms and executes the same work", async () => {
  const work = { id: "work-auto", projectId: "project", kind: "direct", goal: "分析正文", status: "running", version: 0,
    steps: [{ id: "step-auto", operation: "modelKnowledge", status: "queued", target: { batchId: "batch-auto", chapterId: "chapter-1", modelingIntent: true } }] };
  let authorization = null;
  const claims = [];
  const controls = [];
  const state = { confirmations: [], budgets: [] };
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter, modelingConsumer } = await mainHandlers(async (path, body) => {
    if (path.endsWith("/dispatch")) return { action: { workTaskId: work.id } };
    if (path.endsWith("/budget-authorization")) {
      if (body) { assert.equal(body.expectedVersion, work.version); authorization = body; work.version++; return {}; }
      return { authorization };
    }
    if (path.endsWith("/provider-usage")) return { pending: null };
    if (path.endsWith("/control")) {
      assert.equal(body.expectedVersion, work.version);
      controls.push(body.command); work.status = body.command === "pause" ? "paused" : "running"; work.version++;
      return work;
    }
    if (path.endsWith("/work-tasks/work-auto") || path.endsWith("/work-task") || path.endsWith("/execution-scope/continue")) return work;
    if (path.endsWith("/creative-workspace")) return { revision: 0, headSnapshotId: "snapshot", sourceDigest: "source" };
    if (path.endsWith("/provider-budget")) return { taskId: "task-auto", workTaskId: work.id, budgetId: "budget-task-auto",
      maxRequests: authorization.maxRequests, maxTokens: authorization.maxTokens, deadlineAt: authorization.deadlineAt };
    if (path.endsWith("/tasks")) {
      claims.push(body); work.steps[0].status = "running"; work.steps[0].currentAttemptId = "task-auto";
      return modelingTask(body, work);
    }
    if (path.endsWith("/complete")) { work.status = "completed"; work.steps[0].status = "completed"; return { status: "completed" }; }
    return {};
  }, command, state);
  const event = rendererEvent(); event.sender.getURL = () => "http://127.0.0.1:5173/";
  try {
    await handlers.get("agent:set-modeling-context")(event, { projectId: "project", dirty: false });
    await modelingConsumer.tick(); await modelingConsumer.tick();
    assert.equal(work.status, "paused");
    assert.equal(claims.length, 0);
    assert.equal(state.confirmations.length, 0);
    assert.equal(state.budgets.length, 0);
    const result = await handlers.get("agent:resume-work-task")(event, "project", work.id);
    assert.equal(result.id, "work-auto"); assert.equal(result.status, "completed");
    assert.equal(state.confirmations.length, 1);
    assert.deepEqual(controls, ["pause", "resume"]);
    assert.equal(claims.length, 1);
    assert.equal(claims[0].initialBudget, undefined);
    assert.equal(claims[0].target.resumeStepId, "step-auto");
    assert.equal(state.budgets[0].maxRequests, authorization.maxRequests);
  } finally { modelingConsumer.stop(); adapter.close(); }
});

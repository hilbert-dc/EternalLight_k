import assert from "node:assert/strict";
import { test } from "node:test";
import { advanceWorkSteps } from "../work-step-continuation.mjs";
import { readWorkExecutionBinding } from "../work-execution-binding.mjs";
import { grantNewRootBudget } from "../task-budget-grant.mjs";
import { qualityReaderInstructions } from "../quality-notebook.mjs";

test("autonomous reader runs between parent decisions without a UI action or new root budget", async () => {
  const work = { id: "work-quality", projectId: "project", threadId: "author-thread", goal: "改善阅读体验", version: 1, status: "running", steps: [] };
  const sequence = [];
  let count = 0;
  await advanceWorkSteps({}, "project", work.id, {
    currentWorkFlowState: async () => work.status,
    runtimeRequest: async path => {
      if (path.endsWith("/creative-workspace")) return { revision: 1, sourceDigest: "source" };
      if (path.endsWith("/decision")) {
        if (count === 2) return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "completed" };
        const step = { id: `decision-${++count}`, operation: "advisory", status: "queued", target: { workDecision: { workTaskId: work.id } } };
        work.steps.push(step);
        return { workTaskId: work.id, threadId: work.threadId, decisionStepId: step.id, step, stopReason: null };
      }
      return work;
    },
    runAdvisoryAgentTask: async (_event, _project, _instruction, step, _a, _b, thread) => {
      assert.equal(thread, work.threadId); sequence.push(step.id); step.status = "completed";
      if (count === 1) work.steps.push({ id: "reader-step", operation: "reviewQuality", status: "queued", dependsOnStepId: step.id, instruction: "SECRET_DIAGNOSIS", target: { type: "project", qualityPacketId: "packet", workDecisionAction: { workTaskId: work.id } } });
      else assert.equal(work.steps.find(s => s.id === "reader-step").status, "completed");
    },
    runCreativeAgentTask: async (_event, _project, instruction, kind, target) => {
      assert.equal(kind.operation, "reviewQuality");
      assert.equal(grantNewRootBudget({ target }), undefined);
      const prompt = qualityReaderInstructions({ operation: kind.operation, projectId: "project", id: "reader", threadId: "fresh-reader", inputRevision: 1, target, inputManifest: { candidateInstruction: instruction } });
      assert.doesNotMatch(prompt, /SECRET_DIAGNOSIS|author-thread|work-quality/);
      sequence.push("reader"); work.steps.find(s => s.id === target.resumeStepId).status = "completed";
    },
  });
  assert.deepEqual(sequence, ["decision-1", "reader", "decision-2"]);
});

function fixture() {
  const work = { id: "work-one", projectId: "project", threadId: "root-thread", goal: "修改本章", version: 1, status: "completed", steps: [] };
  const calls = [];
  const decision = { id: "decision-one", operation: "advisory", status: "queued", target: { workDecision: { workTaskId: work.id } } };
  let final = false;
  const dependencies = {
    currentWorkFlowState: async () => work.status,
    runtimeRequest: async (path, options) => {
      calls.push({ path, options });
      if (path.endsWith("/creative-workspace")) return { revision: 9, sourceDigest: "digest-nine" };
      if (path.endsWith("/decision")) {
        assert.deepEqual(JSON.parse(options.body), { expectedWorkVersion: work.version, expectedRevision: 9, expectedSourceDigest: "digest-nine" });
        if (final) { work.status = "completed"; return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "completed" }; }
        work.status = "running"; work.steps.push(decision);
        return { workTaskId: work.id, threadId: work.threadId, decisionStepId: decision.id, step: decision, stopReason: null };
      }
      return work;
    },
    runAdvisoryAgentTask: async (...args) => {
      assert.equal(args[6], work.threadId);
      decision.status = "completed";
      // The Agent chooses a rule edit, rather than the Host forcing knowledge
      // extraction or requiring a brief/outline before it.
      work.steps.push({ id: "action-one", operation: "generatePropertyRules", status: "queued", instruction: "保持昼夜规则一致", dependsOnStepId: decision.id,
        target: { type: "artifact", workDecisionAction: { workTaskId: work.id, decisionStepId: decision.id, decisionTaskId: "task-decision", actionKey: "rules" } } });
      return { workflow: { task: { id: "task-decision" } } };
    },
    runCreativeAgentTask: async (...args) => {
      calls.push({ worker: args });
      work.steps[1].status = "completed"; work.version++; final = true;
      return { workflow: { task: { id: "task-worker" } } };
    },
  };
  return { work, calls, dependencies };
}

test("direct and resumed Work dispatch Native-selected actions and finish only on its decision", async () => {
  const f = fixture();
  const result = await advanceWorkSteps({}, "project", f.work.id, f.dependencies);
  assert.equal(result.status, "completed");
  const [call] = f.calls.filter(item => item.worker);
  assert.deepEqual(call.worker.slice(1), ["project", "保持昼夜规则一致", { operation: "generatePropertyRules" },
    { ...f.work.steps[1].target, resumeStepId: "action-one" }]);
});

test("unknown-save and budget stops cannot become successful delivery or a new grant", async () => {
  for (const stopReason of ["pendingRecovery", "budgetExhausted", "budgetExpired", "noReadyActions", "unrecognized"]) {
    const f = fixture();
    const request = f.dependencies.runtimeRequest;
    f.dependencies.runtimeRequest = (path, options) => path.endsWith("/decision")
      ? { workTaskId: f.work.id, threadId: f.work.threadId, step: null, stopReason } : request(path, options);
    await assert.rejects(advanceWorkSteps({}, "project", f.work.id, f.dependencies), { code: "CONVERSATION_INCOMPLETE" });
    assert.equal(f.calls.some(item => item.worker), false);
  }
});

test("pause before the next dispatch preserves saved work without another model call", async () => {
  const f = fixture(); f.work.status = "paused";
  assert.equal(await advanceWorkSteps({}, "project", f.work.id, f.dependencies), f.work);
  assert.equal(f.calls.length, 1);
});

test("a queued step not advanced by Runtime is never replayed in the same Host drive", async () => {
  const f = fixture();
  f.dependencies.runCreativeAgentTask = async () => ({});
  await assert.rejects(advanceWorkSteps({}, "project", f.work.id, f.dependencies), /停止重复调用/);
});

test("workflow provenance checks actual step membership and original parent thread", async () => {
  const binding = { workTaskId: "work-one", rootThreadId: "root-thread", decisionStepId: "step-decision", decisionTaskId: "task-decision", originTaskId: "task-root", actionKey: "write-one" };
  const task = { id: "task-child", projectId: "project", threadId: "worker-thread", operation: "generateChapter",
    inputManifest: { workDecisionAction: binding }, target: { workDecisionAction: binding } };
  const work = { id: "work-one", projectId: "project", threadId: "root-thread", steps: [
    { id: "step-decision", operation: "advisory", attempts: [{ id: "task-decision", threadId: "root-thread" }] },
    { id: "step-child", operation: "generateChapter", currentAttemptId: task.id, attempts: [{ id: task.id, threadId: task.threadId }] },
  ] };
  assert.deepEqual(await readWorkExecutionBinding(async () => work, "project", task), {
    ...binding, stepId: "step-child", taskId: task.id,
  });
  work.steps[0].attempts[0].threadId = "foreign-thread";
  await assert.rejects(readWorkExecutionBinding(async () => work, "project", task), /原对话/);
});

test("decision parent binds the persisted step without inventing target-only fields", async () => {
  const target = { workTaskId: "work-one", originTaskId: "task-root", sourceTaskId: "task-writer" };
  const task = { id: "task-decision", projectId: "project", threadId: "root-thread", operation: "advisory",
    target: { workDecision: target }, inputManifest: { workDecision: { ...target, rootThreadId: "root-thread", decisionStepId: "step-one", decisionTaskId: "task-decision" } } };
  const work = { id: "work-one", projectId: "project", threadId: "root-thread", steps: [
    { id: "step-one", operation: "advisory", currentAttemptId: task.id, attempts: [{ id: task.id, threadId: task.threadId }] },
  ] };
  assert.deepEqual(await readWorkExecutionBinding(async () => work, "project", task), {
    workTaskId: "work-one", rootThreadId: "root-thread", stepId: "step-one", taskId: "task-decision",
    decisionStepId: "step-one", decisionTaskId: "task-decision", actionKey: null, originTaskId: "task-root",
  });
  task.target.workDecision = { ...target, sourceTaskId: "another-source" };
  await assert.rejects(readWorkExecutionBinding(async () => work, "project", task), /来源不一致/);
});

test("failed worker continues through a fresh Native correction without replay or new authorization", async () => {
  const work = { id: "work-one", projectId: "project", threadId: "root-thread", goal: "写本章", version: 1,
    status: "running", steps: [{ id: "first", operation: "modelKnowledge", status: "queued", target: {} }] };
  const dispatched = [];
  const error = Object.assign(new Error("invalid_tool_arguments"), { code: "TOOL_ARGUMENTS" });
  const correction = { id: "correction", operation: "advisory", status: "queued", target: { workDecision: { workTaskId: work.id } } };
  const dependencies = {
    currentWorkFlowState: async () => work.status,
    runtimeRequest: async (path, options) => {
      if (path.endsWith("/creative-workspace")) return { revision: 1, sourceDigest: "source" };
      if (path.endsWith("/decision")) {
        assert.deepEqual(JSON.parse(options.body), { expectedWorkVersion: work.version, expectedRevision: 1, expectedSourceDigest: "source" });
        if (dispatched.includes("fixed")) return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "completed" };
        work.steps.push(correction); work.status = "running";
        return { workTaskId: work.id, threadId: work.threadId, decisionStepId: correction.id, stopReason: null, step: correction };
      }
      assert.equal(path, "/api/projects/project/work-tasks/work-one");
      return work;
    },
    runCreativeAgentTask: async (_event, _project, _instruction, _kind, target) => {
      dispatched.push(target.resumeStepId);
      const step = work.steps.find(item => item.id === target.resumeStepId);
      if (step.id === "first") { step.status = "blocked"; work.status = "blocked"; throw error; }
      step.status = "completed"; return {};
    },
    runAdvisoryAgentTask: async () => {
      dispatched.push(correction.id); correction.status = "completed";
      work.steps.push({ id: "fixed", operation: "modelKnowledge", status: "queued", dependsOnStepId: correction.id, target: {} });
      return {};
    },
  };
  await advanceWorkSteps({}, "project", work.id, dependencies);
  assert.deepEqual(dispatched, ["first", "correction", "fixed"]);
});

for (const stopReason of ["pendingRecovery", "budgetExhausted", "executionBlocked", "recoveryStalled"]) {
  test(`failed worker preserves the original failure when Native stops at ${stopReason}`, async () => {
    const f = fixture();
    const original = new Error("worker failed");
    const request = f.dependencies.runtimeRequest;
    let failed = false;
    f.dependencies.runCreativeAgentTask = async () => { failed = true; f.work.status = "blocked"; throw original; };
    f.dependencies.runtimeRequest = async (path, options) => failed && path.endsWith("/decision")
      ? { workTaskId: f.work.id, threadId: f.work.threadId, stopReason, step: null } : request(path, options);
    await assert.rejects(advanceWorkSteps({}, "project", f.work.id, f.dependencies), error => error === original);
  });
}


test("queued media continuation keeps the original scoped conversation", async () => {
  const step={id:"media-step",operation:"advisory",status:"queued",target:{type:"edition",editionId:"comic"}};
  const work={id:"work-media",projectId:"project",threadId:"media-thread",status:"running",steps:[step]};
  let called=false;
  await advanceWorkSteps({},"project",work.id,{
    currentWorkFlowState:async()=>work.status,
    runtimeRequest:async path=>{assert.equal(path,`/api/projects/project/work-tasks/${work.id}`);return work;},
    runAdvisoryAgentTask:async(...args)=>{called=true;assert.equal(args[3],step);assert.equal(args[6],null);assert.equal(args[7],work.threadId);work.status="paused";},
    runCreativeAgentTask:async()=>assert.fail("media advisory must retain its operation"),
  });
  assert.equal(called,true);
});

test("media continuation returns Native completion without a novel decision and preserves unresolved states", async () => {
  const completed={id:"work-media",projectId:"project",threadId:"media-thread",status:"completed",deliveryStatus:"discussion_ended",pendingContinuation:false,
    deliveryReport:{unsettledAttemptIds:[]},steps:[{id:"media-step",operation:"advisory",status:"completed",target:{type:"edition"}}]};
  for(const change of [null,{status:"blocked"},{deliveryStatus:"pending"},{pendingContinuation:true},{deliveryReport:{unsettledAttemptIds:["task-media"]}},{resumeBlockReason:"unknownUsage"}]) {
    const work={...completed,...change};
    const dependencies={currentWorkFlowState:async()=>work.status,runtimeRequest:async path=>{
      assert.equal(path,"/api/projects/project/work-tasks/work-media","media recovery must not request a novel workspace or decision");return work;
    }};
    if(change)await assert.rejects(advanceWorkSteps({},"project",work.id,dependencies),{code:"CONVERSATION_INCOMPLETE"});
    else assert.equal(await advanceWorkSteps({},"project",work.id,dependencies),work);
  }
});

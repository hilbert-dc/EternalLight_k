import assert from "node:assert/strict";
import { test } from "node:test";

import { CAPABILITIES } from "../src/index.mjs";
import { textDigest } from "../src/observability/message-evidence.mjs";
import { evaluateDelegationCapability, inspectTerminalChildWait, observedTerminalChildWait, summarizeCollaborationEvents } from "../qualification/delegation-proof.mjs";

test("delegation capabilities retain completed prefix evidence after a later timeout", () => {
  const observations = {
    selectedOperations: ["spawn_agent", "send_message"],
    completedOperations: ["spawn_agent"],
    childIdentityObserved: true,
    childGateObserved: true,
    parentFollowupTokenQueued: true,
    childResultObserved: false,
    directedRouteObserved: true,
    terminated: false,
  };
  assert.equal(evaluateDelegationCapability(CAPABILITIES.CHILD_SPAWN, observations, "trace").status, "verified");
  assert.equal(evaluateDelegationCapability(CAPABILITIES.CHILD_DIRECTED_ROUTE, observations, "trace").status, "verified");
  assert.equal(evaluateDelegationCapability(CAPABILITIES.CHILD_MESSAGE_RUNNING, observations, "trace").status, "unknown");
  assert.equal(evaluateDelegationCapability(CAPABILITIES.CHILD_WAIT, observations, "trace").status, "unknown");
});

test("selected collaboration tools do not count as completed operations", () => {
  const summary = summarizeCollaborationEvents([], [{ toolNames: ["spawn_agent", "send_message"] }]);
  assert.deepEqual(summary.selectedOperations, ["send_message", "spawn_agent"]);
  assert.deepEqual(summary.completedOperations, []);
  assert.deepEqual(summary.failedOperations, []);

  const completed = summarizeCollaborationEvents([{
    method: "item/completed",
    params: { item: { type: "collabAgentToolCall", tool: "spawn_agent", status: "completed" } },
  }], []);
  assert.deepEqual(completed.completedOperations, ["spawn_agent"]);
});

test("wait proof uses a trusted terminal child result without requiring the message receipt", () => {
  const proof = evaluateDelegationCapability(CAPABILITIES.CHILD_WAIT, {
    completedOperations: ["wait_agent"],
    childTerminalResultObserved: true,
    terminated: true,
  }, "trace");
  assert.equal(proof.status, "verified");
  assert.deepEqual(proof.assertions.map(({ id, result }) => ({ id, result })), [
    { id: "childWait", result: "passed" },
    { id: "child-terminal-result-observed", result: "passed" },
  ]);
});

test("message certification requires delivery, model visibility and consumption independently", () => {
  const evidence = { completedOperations: ["send_message"], childGateObserved: true, parentFollowupTokenQueued: true, childFollowupVisible: true, childResultObserved: true, terminated: true };
  assert.equal(evaluateDelegationCapability(CAPABILITIES.CHILD_MESSAGE_RUNNING, evidence).status, "verified");
  for (const field of ["parentFollowupTokenQueued", "childFollowupVisible", "childResultObserved"]) {
    assert.equal(evaluateDelegationCapability(CAPABILITIES.CHILD_MESSAGE_RUNNING, { ...evidence, [field]: false }).status, "failed");
  }
});

test("wait completion requires the same child's terminal result and rejects timed-out mailbox waits", () => {
  const expected = "CHILD_TERMINAL:nonce";
  const events = [
    { method: "item/completed", params: { threadId: "parent", item: { id: "wait", type: "collabAgentToolCall", tool: "wait", status: "completed", agentsStates: {} } } },
    { method: "item/completed", params: { threadId: "parent", item: { type: "subAgentActivity", kind: "completed", agentThreadId: "child", agentPath: "/root/child" } } },
  ];
  const children = [{ threadId: "child", parentThreadId: "parent" }];
  const responses = [{ threadId: "child", parentThreadId: "parent", status: "completed", finalTextDigest: textDigest(expected), finalTextLineDigests: [textDigest(expected)] }];
  const requests = [{ threadId: "parent", visibleTextDigests: [textDigest(expected)], finalAgentMessages: [{ author: "/root/child", textDigests: [textDigest(expected)] }], waitResults: [{ callId: "wait", completed: true }] }];
  assert.equal(observedTerminalChildWait(events, children, expected, responses, requests), true);
  assert.deepEqual(inspectTerminalChildWait(events, children, expected, responses, requests), {
    waitCompleted: true, trustedChildObserved: true, stateCompleted: false,
    stateMessageMatched: false, stateResultPresent: false, childResponseCompleted: true,
    childResponseDigestMatched: true, completionEventMatched: true,
    parentWaitResultMatched: true, parentMessageMatched: true, observed: true,
  });
  assert.equal(observedTerminalChildWait(events, children, expected, responses, [{ ...requests[0], waitResults: [{ callId: "wait", completed: false }] }]), false);
  assert.equal(observedTerminalChildWait(events, children, expected, [{ ...responses[0], threadId: "another-child" }], requests), false);
  assert.equal(observedTerminalChildWait(events, children, expected, [{ ...responses[0], finalTextDigest: null }], requests), false);
  assert.equal(observedTerminalChildWait(events, children, expected, responses, [{ ...requests[0], finalAgentMessages: [] }]), false);
  const actual = `${expected}.`;
  const delivered = [{ ...responses[0], finalTextDigest: textDigest(actual), finalTextLineDigests: [textDigest(actual)] }];
  const consumed = [{ ...requests[0], finalAgentMessages: [{ author: "/root/child", textDigests: [textDigest(actual)] }] }];
  const deliveredProof = inspectTerminalChildWait(events, children, expected, delivered, consumed);
  assert.equal(deliveredProof.childResponseDigestMatched, false);
  assert.equal(deliveredProof.parentMessageMatched, true);
  assert.equal(deliveredProof.observed, true);
  assert.equal(observedTerminalChildWait(events, children, expected, delivered, requests), false);
  const unmatched = inspectTerminalChildWait(events, children, expected, [{ ...responses[0], finalTextDigest: null }], requests);
  assert.equal(unmatched.observed, false);
  assert.equal(unmatched.childResponseCompleted, true);
  assert.equal(unmatched.childResponseDigestMatched, false);
  assert.equal(Object.values(unmatched).every((value) => typeof value === "boolean"), true);
  const stateEvents = [{ ...events[0], params: { ...events[0].params, item: { ...events[0].params.item,
    agentsStates: { child: { completed: expected } } } } }];
  const stateProof = inspectTerminalChildWait(stateEvents, children, expected);
  assert.equal(stateProof.stateMessageMatched, true);
  assert.equal(stateProof.stateResultPresent, true);
  assert.equal(stateProof.observed, true);
  const nonLiteralState = inspectTerminalChildWait([{ ...stateEvents[0], params: { ...stateEvents[0].params,
    item: { ...stateEvents[0].params.item, agentsStates: { child: { completed: actual } } } } }], children, expected);
  assert.equal(nonLiteralState.stateMessageMatched, false);
  assert.equal(nonLiteralState.observed, true);
  const noResultState = inspectTerminalChildWait([{ ...stateEvents[0], params: { ...stateEvents[0].params,
    item: { ...stateEvents[0].params.item, agentsStates: { child: { completed: null } } } } }], children, expected);
  assert.equal(noResultState.observed, false);
});

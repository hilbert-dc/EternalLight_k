import { CAPABILITIES } from "../src/contracts/capabilities.mjs";
import { textDigest } from "../src/observability/message-evidence.mjs";

const OPERATION_BY_CAPABILITY = Object.freeze({
  [CAPABILITIES.CHILD_SPAWN]: "spawn_agent",
  [CAPABILITIES.CHILD_MESSAGE_RUNNING]: "send_message",
  [CAPABILITIES.CHILD_WAIT]: "wait_agent",
});

function resultFor(passed, conclusive) {
  if (passed) return "passed";
  return conclusive ? "failed" : "unknown";
}

function assertion(id, passed, conclusive, traceRef, observed) {
  return Object.freeze({
    id,
    result: resultFor(passed, conclusive),
    expected: true,
    observed,
    traceRef,
  });
}

function allPassed(assertions) {
  return assertions.every((entry) => entry.result === "passed");
}

function anyFailed(assertions) {
  return assertions.some((entry) => entry.result === "failed");
}

export function evaluateDelegationCapability(capability, observations = {}, traceRef = null) {
  const completed = new Set(observations.completedOperations ?? []);
  const selected = new Set(observations.selectedOperations ?? []);
  const failed = new Set(observations.failedOperations ?? []);
  const terminated = observations.terminated === true;
  let assertions;
  if (capability === CAPABILITIES.CHILD_SPAWN) {
    const spawnCompleted = completed.has("spawn_agent") || observations.childIdentityObserved === true;
    assertions = [
      assertion("childSpawn", spawnCompleted, terminated || failed.has("spawn_agent"), traceRef, { selected: selected.has("spawn_agent"), completedEvent: completed.has("spawn_agent"), trustedChildObserved: observations.childIdentityObserved === true, failed: failed.has("spawn_agent") }),
      assertion("trusted-child-identity", observations.childIdentityObserved === true, terminated || completed.has("spawn_agent"), traceRef, observations.childIdentityObserved === true),
    ];
  } else if (capability === CAPABILITIES.CHILD_MESSAGE_RUNNING) {
    const messageCompleted = completed.has("send_message") || observations.childResultObserved === true;
    assertions = [
      assertion("childMessageRunning", messageCompleted, terminated || failed.has("send_message"), traceRef, { selected: selected.has("send_message"), completedEvent: completed.has("send_message"), followupConsumed: observations.childResultObserved === true, failed: failed.has("send_message") }),
      assertion("child-running-gate-observed", observations.childGateObserved === true, terminated || completed.has("spawn_agent"), traceRef, observations.childGateObserved === true),
      assertion("parent-followup-token-queued", observations.parentFollowupTokenQueued === true, terminated || completed.has("send_message"), traceRef, observations.parentFollowupTokenQueued === true),
      assertion("child-followup-visible", observations.childFollowupVisible === true, terminated, traceRef, observations.childFollowupVisible === true),
      assertion("child-followup-consumed", observations.childResultObserved === true, terminated, traceRef, observations.childResultObserved === true),
    ];
  } else if (capability === CAPABILITIES.CHILD_WAIT) {
    assertions = [
      assertion("childWait", completed.has("wait_agent"), terminated || failed.has("wait_agent"), traceRef, { selected: selected.has("wait_agent"), completed: completed.has("wait_agent"), failed: failed.has("wait_agent") }),
      assertion("child-terminal-result-observed", observations.childTerminalResultObserved === true, terminated, traceRef, observations.childTerminalResultObserved === true),
    ];
  } else if (capability === CAPABILITIES.CHILD_DIRECTED_ROUTE) {
    assertions = [
      assertion("directed-child-model-observed", observations.directedRouteObserved === true, terminated || completed.has("spawn_agent"), traceRef, observations.directedRouteObserved === true),
      assertion("trusted-child-identity", observations.childIdentityObserved === true, terminated || completed.has("spawn_agent"), traceRef, observations.childIdentityObserved === true),
    ];
  } else {
    throw new Error(`unsupported delegation capability: ${capability}`);
  }
  return Object.freeze({
    status: allPassed(assertions) ? "verified" : anyFailed(assertions) ? "failed" : "unknown",
    assertions: Object.freeze(assertions),
  });
}

export function observedTerminalChildWait(events, childRequests, expectedText, responses = [], requests = []) {
  return events.some((event) => {
    const item = event.params?.item;
    if (event.method !== "item/completed" || item?.type !== "collabAgentToolCall" || !/wait/i.test(item.tool ?? "") || item.status !== "completed") return false;
    const stateProof = Object.entries(item.agentsStates ?? {}).some(([threadId, state]) =>
      state.status === "completed" && state.message?.trim() === expectedText
      && childRequests.some((request) => request.threadId === threadId && request.parentThreadId === event.params.threadId));
    const mailboxProof = responses.some((response) => response.parentThreadId === event.params.threadId
        && response.status === "completed" && response.finalTextDigest === textDigest(expectedText)
        && childRequests.some((request) => request.threadId === response.threadId && request.parentThreadId === response.parentThreadId)
        && events.some((delivery) => delivery.method === "item/completed" && delivery.params?.threadId === event.params.threadId
          && delivery.params.item?.type === "subAgentActivity" && delivery.params.item.kind === "completed"
          && delivery.params.item.agentThreadId === response.threadId
          && requests.some((request) => request.threadId === event.params.threadId
            && request.waitResults?.some((result) => result.callId === item.id && result.completed)
            && request.finalAgentMessages?.some((message) => message.author === delivery.params.item.agentPath && message.textDigests?.includes(textDigest(expectedText))))));
    return stateProof || mailboxProof;
  });
}

export function summarizeCollaborationEvents(events = [], responses = []) {
  const selected = new Set();
  const completed = new Set();
  const failed = new Set();
  const normalize = (value) => {
    const label = String(value ?? "");
    if (/spawn/i.test(label)) return "spawn_agent";
    if (/send|message/i.test(label)) return "send_message";
    if (/wait/i.test(label)) return "wait_agent";
    return null;
  };
  for (const response of responses) {
    for (const name of response.toolNames ?? []) {
      const operation = normalize(name);
      if (operation) selected.add(operation);
    }
  }
  for (const event of events) {
    const item = event.params?.item;
    if (item?.type !== "collabAgentToolCall") continue;
    const operation = normalize(item.tool ?? item.action);
    if (!operation) continue;
    selected.add(operation);
    if (event.method === "item/completed" && !["failed", "cancelled"].includes(item.status)) completed.add(operation);
    if (event.method === "item/completed" && ["failed", "cancelled"].includes(item.status)) failed.add(operation);
  }
  return Object.freeze({
    selectedOperations: Object.freeze([...selected].sort()),
    completedOperations: Object.freeze([...completed].sort()),
    failedOperations: Object.freeze([...failed].sort()),
  });
}

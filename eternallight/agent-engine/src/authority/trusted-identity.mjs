const WRITE_OPERATIONS = new Set(["save_brief", "save_outline", "save_chapter_plans", "save_draft_checkpoint", "submit_fact_proposals", "submit_planning_review_report", "complete_task"]);

export function trustedIdentityFromMcpMeta(meta) {
  if (!meta || typeof meta !== "object" || Array.isArray(meta)) throw new Error("trusted MCP metadata is required");
  const callerThreadId = typeof meta.callerThreadId === "string" && meta.callerThreadId ? meta.callerThreadId : null;
  const rootThreadId = typeof meta.rootThreadId === "string" && meta.rootThreadId ? meta.rootThreadId : null;
  if (!callerThreadId || !rootThreadId) throw new Error("trusted MCP caller and root thread identities are required");
  if (meta.sessionId !== rootThreadId) throw new Error("trusted MCP session and root identities disagree");
  const parentThreadId = typeof meta.parentThreadId === "string" && meta.parentThreadId ? meta.parentThreadId : null;
  const isRoot = callerThreadId === rootThreadId && !parentThreadId;
  return Object.freeze({
    callerThreadId,
    parentThreadId,
    rootThreadId,
    sessionId: meta.sessionId,
    turnId: typeof meta.turnId === "string" && meta.turnId ? meta.turnId : null,
    toolCallId: typeof meta.callId === "string" && meta.callId ? meta.callId : null,
    source: typeof meta.sessionSource === "string" && meta.sessionSource ? meta.sessionSource : null,
    role: typeof meta.agentRole === "string" && meta.agentRole ? meta.agentRole : null,
    depth: Number.isInteger(meta.agentDepth) && meta.agentDepth >= 0 ? meta.agentDepth : parentThreadId ? 1 : 0,
    kind: isRoot ? "root" : "subagent",
  });
}

export function authorizeMcpOperation(identity, operation, requestedThreadId) {
  const boundThreadId = identity.rootThreadId;
  if (requestedThreadId !== boundThreadId) return Object.freeze({ allowed: false, reason: "thread_identity_mismatch", boundThreadId });
  if (identity.kind === "subagent" && (WRITE_OPERATIONS.has(operation) || operation !== "read_context")) {
    return Object.freeze({ allowed: false, reason: "subagent_read_only", boundThreadId });
  }
  return Object.freeze({ allowed: true, reason: null, boundThreadId });
}

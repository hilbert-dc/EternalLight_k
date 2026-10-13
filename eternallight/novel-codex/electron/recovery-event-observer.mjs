// @ts-check
const TURN_METHODS = new Set(["turn/started", "turn/completed"]);
const MCP_METHODS = new Set(["item/started", "item/completed"]);

function identities(params) {
  return {
    threadId: params?.threadId,
    turnId: params?.turnId,
    turn: params?.turn ? { id: params.turn.id } : undefined,
  };
}

// This follows the current RecoveryJournal state machine. Rendering/history
// payloads have no recovery authority; MCP arguments and receipts do.
export function projectRecoveryEvent(event) {
  const params = event?.params;
  if (TURN_METHODS.has(event?.method)) {
    return { method: event.method, params: identities(params) };
  }
  if (params?.item?.type === "mcpToolCall") {
    return {
      method: event.method,
      params: { ...identities(params), item: params.item },
    };
  }
  return null;
}

export function recoveryEventDiagnostic(event) {
  const method = TURN_METHODS.has(event?.method) || MCP_METHODS.has(event?.method)
    ? event.method
    : event?.params?.item?.type === "mcpToolCall" ? "mcp/other" : "other";
  return { method, bytes: Buffer.byteLength(JSON.stringify(event) ?? "null", "utf8") };
}

export async function observeRecoveryEvent(journal, event, onFailure = (_error) => {}) {
  if (!journal) return;
  const projected = projectRecoveryEvent(event);
  if (!projected) return;
  try {
    await journal.observe(projected);
  } catch (error) {
    try { onFailure(recoveryEventDiagnostic(event)); } catch { /* Preserve the journal's original failure. */ }
    // Journal.flush() must still reject. Runtime reconciliation alone can
    // establish an unknown write's outcome; display compaction cannot do so.
    throw error;
  }
}

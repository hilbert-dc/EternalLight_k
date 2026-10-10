import { defineRecoveryOperationContract, digest } from "@eternallight/agent-engine";
import { readFileSync } from "node:fs";

// Explicit domain effects, not a name-prefix heuristic. Unknown tools fail closed.
export const NOVEL_READ_ONLY_RECOVERY_TOOLS = Object.freeze(JSON.parse(readFileSync(new URL("../shared/novel-recovery-read-tools.json", import.meta.url), "utf8")));
const READS = new Set(NOVEL_READ_ONLY_RECOVERY_TOOLS);
const identity = value => typeof value === "string" && value.length > 0 && value.length <= 160 && !/[\u0000-\u001f\u007f]/.test(value);
const unknown = () => ({ outcome: "unknown", receiptRef: null });

// Both identities use Native serde_json's deterministic object order and JSON
// number serialization. No JavaScript key sorting or alternate hash is used.
export function novelRecoveryOperationHash(toolCall) {
  return digest({ tool: toolCall.tool, arguments: toolCall.arguments });
}

export function createNovelRecoveryOperationContract({ getBinding = () => null } = {}) {
  return defineRecoveryOperationContract({
    classify(call) {
      const trustedServer = call?.server === "novel_workspace";
      const key = call?.arguments?.idempotencyKey;
      return { effect: trustedServer && READS.has(call.tool) ? "read" : "write", operation: call?.tool ?? null,
        idempotencyRef: typeof key === "string" && key ? `sha256:${digest(key)}` : null };
    },
    hash: novelRecoveryOperationHash,
    receipt({ event, toolCall, result }) {
      if (toolCall?.server !== "novel_workspace") return null;
      const proof = result?.structuredContent?.recoveryOperation;
      const args = toolCall.arguments;
      const binding = getBinding();
      if (!proof || typeof proof !== "object" || !args || !binding) return null;
      const turn = event?.params?.turnId ?? event?.params?.turn?.id;
      if (proof.authority !== "novelWorkspaceMcp"
        || ![proof.projectId, proof.runId, proof.rootThreadId, proof.callerThreadId, proof.turnId, proof.callId, proof.toolName].every(identity)
        || proof.projectId !== binding.projectId || proof.runId !== binding.runId || proof.rootThreadId !== binding.threadId
        || proof.projectId !== args.projectId || proof.runId !== args.taskId || proof.rootThreadId !== args.threadId
        || proof.rootThreadId !== event?.params?.threadId || proof.callerThreadId !== proof.rootThreadId
        || ![null, ""].includes(proof.parentThreadId) || proof.turnId !== turn
        || proof.callId !== toolCall.id || proof.toolName !== toolCall.tool
        || proof.requestHash !== novelRecoveryOperationHash(toolCall)) return null;
      if (proof.phase === "beforeDispatch") {
        const feedback = result.structuredContent;
        if ((result.isError !== true && toolCall.status !== "failed") || feedback.tool !== toolCall.tool
          || !["invalid_tool_arguments", "tool_argument_correction_limit"].includes(feedback.code)
          || proof.outcome !== "rejected" || proof.operationId !== null
          || proof.receiptRef !== `mcp:preflight:${proof.callId}:${proof.requestHash}`) return null;
        return { outcome: "rejected", receiptRef: proof.receiptRef };
      }
      if (proof.phase !== "runtimeLedger" || !identity(proof.operationId)) return null;
      if (proof.outcome === "committed" && typeof proof.receiptRef === "string" && proof.receiptRef.startsWith("runtime:") && proof.receiptRef.length <= 500) {
        return { outcome: "committed", receiptRef: proof.receiptRef };
      }
      if (proof.outcome === "rejected" && proof.receiptRef === `runtime:operation:${proof.operationId}:rejected`) return { outcome: "rejected", receiptRef: proof.receiptRef };
      return null;
    },
    // Persisted reconciliation remains the fenced Runtime ledger path. Never
    // infer absence, commitment or rejection from a terminal task or prose.
    reconcile: unknown,
  });
}

import { digest } from "../contracts/capabilities.mjs";
import { nativeCall } from "../native.mjs";

const EFFECTS = new Set(["read", "write"]);
const OUTCOMES = new Set(["committed", "rejected", "cancelled-before-execution", "unknown"]);

/**
 * Defines the domain boundary used by generic recovery code. Implementations
 * classify effects, produce content-free operation hashes, recognize
 * authoritative receipts, and reconcile unresolved writes against the domain
 * ledger. Arguments and result bodies are never persisted by this contract.
 */
export function defineRecoveryOperationContract({ classify, hash, receipt, reconcile }) {
  if (![classify, hash, receipt, reconcile].every((operation) => typeof operation === "function")) {
    throw new Error("recovery operation contract requires classify, hash, receipt, and reconcile functions");
  }
  return Object.freeze({
    classify(toolCall) {
      return Object.freeze(nativeCall("recoveryContract.classify", { value: classify(toolCall), tool: toolCall?.tool ?? null }));
    },
    async hash(toolCall, classification) {
      return nativeCall("recoveryContract.hash", await hash(toolCall, classification));
    },
    async receipt(observation, classification) {
      return Object.freeze(nativeCall("recoveryContract.resolve", { value: await receipt(observation, classification), source: "receipt" }));
    },
    async reconcile(candidate, context) {
      return Object.freeze(nativeCall("recoveryContract.resolve", { value: await reconcile(candidate, context), source: "reconcile" }) ?? { outcome: "unknown", receiptRef: null });
    },
  });
}

/**
 * Fail-closed fallback for consumers that have not installed a domain
 * contract yet. Every MCP operation is treated as a possible write and no
 * event result is accepted as an authoritative domain receipt.
 */
export function createConservativeRecoveryOperationContract() {
  return defineRecoveryOperationContract({
    classify(toolCall) {
      const idempotencyKey = toolCall?.arguments?.idempotencyKey;
      return {
        effect: "write",
        operation: toolCall?.tool ?? null,
        idempotencyRef: typeof idempotencyKey === "string" && idempotencyKey ? `sha256:${digest(idempotencyKey)}` : null,
      };
    },
    hash(toolCall) {
      return digest({ tool: toolCall?.tool ?? null, arguments: toolCall?.arguments ?? null });
    },
    receipt() {
      return null;
    },
    reconcile() {
      return { outcome: "unknown", receiptRef: null };
    },
  });
}

export const RECOVERY_OPERATION_EFFECTS = Object.freeze([...EFFECTS]);
export const RECOVERY_OPERATION_OUTCOMES = Object.freeze([...OUTCOMES]);

// @ts-check
// Only matched App-server responses/events may establish these local facts.
const terminalFailures = new WeakMap();
const rejectedSteers = new WeakSet();

export function coreRpcError(method, response) {
  const error = Object.assign(new Error(response?.message ?? "创作引擎请求失败"),
    { rpcCode: Number.isInteger(response?.code) ? response.code : null });
  if (method === "turn/steer" && error.rpcCode === -32600) rejectedSteers.add(error);
  return error;
}

export function isRejectedSteer(error) {
  return error !== null && typeof error === "object" && rejectedSteers.has(error);
}

export function coreTurnError(threadId, turn) {
  const diagnostic = Array.from(String(turn.error?.message ?? "创作引擎回合失败")).slice(0, 4000).join("");
  const failure = Object.freeze({ issuer: "codexAppServer", code: "CORE_TURN_FAILED",
    threadId, turnId: turn.id, message: diagnostic, automaticContinuation: "stopped",
    contextWindowExceeded: turn.error?.codexErrorInfo === "contextWindowExceeded" });
  const error = Object.assign(new Error(diagnostic), { code: failure.code });
  terminalFailures.set(error, failure);
  return error;
}

/** @param {unknown} error */
export function coreTurnFailure(error) {
  const seen = new Set();
  while (error && typeof error === "object" && !seen.has(error)) {
    seen.add(error);
    const failure = terminalFailures.get(error);
    if (failure) return failure;
    error = "cause" in error ? error.cause : null;
  }
  return null;
}

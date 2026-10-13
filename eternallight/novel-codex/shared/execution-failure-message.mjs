/** Read a serialized Host failure without requiring an Error prototype across IPC. */
export function executionFailureMessage(error, fallback = "执行未完成") {
  const failure = error?.coreTurnFailure;
  if (failure?.issuer === "codexAppServer" && failure.code === "CORE_TURN_FAILED"
    && failure.automaticContinuation === "stopped" && typeof failure.message === "string") return failure.message;
  return error instanceof Error ? error.message : fallback;
}

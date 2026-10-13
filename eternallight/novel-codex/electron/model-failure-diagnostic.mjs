// @ts-check
// Consume the Engine's public failure observation without logging request content.
export function modelFailureDiagnostic(observation, secrets = []) {
  if (observation?.status !== "failed") return null;
  const redact = value => {
    if (typeof value !== "string") return null;
    for (const secret of secrets.filter(value => typeof value === "string" && value).sort((a, b) => b.length - a.length)) {
      value = value.replaceAll(secret, "[REDACTED]");
    }
    return Array.from(value).slice(0, 1000).join("");
  };
  const capacity = observation.errorCode === "context_input_budget_exceeded" ? observation.contextCapacity : null;
  const counts = ["estimatedInputTokens", "inputLimitTokens", "excessTokens"];
  const contextCapacity = capacity && ["connection", "route"].includes(capacity.boundary)
    && counts.every(key => Number.isSafeInteger(capacity[key]) && capacity[key] >= 0)
    && capacity.excessTokens > 0 && capacity.estimatedInputTokens - capacity.inputLimitTokens === capacity.excessTokens
    ? Object.fromEntries([["boundary", capacity.boundary], ...counts.map(key => [key, capacity[key]])]) : null;
  return Object.fromEntries([
    ...["runId", "requestId", "connectionId", "logicalModel", "errorCategory", "errorCode", "errorMessage"]
      .map(key => [key, redact(observation[key])]),
    ["errorStatusCode", Number.isInteger(observation.errorStatusCode) ? observation.errorStatusCode : null],
    ["contextCapacity", contextCapacity],
    ["retryable", typeof observation.retryable === "boolean" ? observation.retryable : null],
  ]);
}

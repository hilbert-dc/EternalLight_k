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
  return Object.fromEntries([
    ...["runId", "requestId", "connectionId", "logicalModel", "errorCategory", "errorCode", "errorMessage"]
      .map(key => [key, redact(observation[key])]),
    ["errorStatusCode", Number.isInteger(observation.errorStatusCode) ? observation.errorStatusCode : null],
    ["retryable", typeof observation.retryable === "boolean" ? observation.retryable : null],
  ]);
}

import { digest } from "../contracts/capabilities.mjs";
import { nativeCall } from "../native.mjs";

export const normalizeEndpoint = (endpoint) => nativeCall("connection.endpoint", endpoint);
export function defineConnection(input) {
  const connection = nativeCall("connection.define", input);
  for (const value of Object.values(connection)) if (value && typeof value === "object") Object.freeze(value);
  return Object.freeze(connection);
}

export function connectionEnvironmentDigest(connection, { codecVersion = "1", credentialScope = "default", engineContractHash, protocol, qualificationContextDigest = "default" } = {}) {
  return digest({
    codecVersion,
    credentialScope,
    endpoint: connection.endpoint,
    engineContractHash,
    model: connection.model,
    protocol: protocol ?? connection.protocol,
    protocolOptions: connection.protocolOptions,
    capabilitySupport: connection.capabilitySupport,
    qualificationContextDigest,
  });
}

export function connectionDefinitionDigest(connection) {
  return digest({
    id: connection.id,
    provider: connection.provider,
    endpoint: connection.endpoint,
    model: connection.model,
    credentialRef: connection.credentialRef,
    protocol: connection.protocol,
    protocolOptions: connection.protocolOptions,
    capabilitySupport: connection.capabilitySupport,
    declaredLimits: connection.declaredLimits,
  });
}

export function credentialRealmId(credentialScope) {
  if (typeof credentialScope !== "string" || !credentialScope) throw new Error("credential scope is required");
  return digest({ credentialScope });
}

import { nativeCall } from "../native.mjs";
export const normalizeEndpoint = endpoint => nativeCall("connection.endpoint", endpoint);
export function defineConnection(input) {
  const connection = nativeCall("connection.define", input);
  for (const value of Object.values(connection)) if (value && typeof value === "object") Object.freeze(value);
  return Object.freeze(connection);
}
export const connectionEnvironmentDigest = (connection, options = {}) => nativeCall("connection.environmentDigest", { connection, options });
export const connectionDefinitionDigest = connection => nativeCall("connection.definitionDigest", connection);
export const credentialRealmId = credentialScope => nativeCall("connection.credentialRealm", credentialScope);

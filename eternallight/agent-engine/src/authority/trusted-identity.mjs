import { nativeCall } from "../native.mjs";
export const trustedIdentityFromMcpMeta = meta => Object.freeze(nativeCall("hostIdentity.meta", meta));
export const authorizeMcpOperation = (identity, operation, requestedThreadId) => Object.freeze(nativeCall("hostIdentity.authorize", { identity, operation, requestedThreadId }));

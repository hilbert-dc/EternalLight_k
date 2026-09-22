import { nativeCall } from "../native.mjs";
export const directedRouteDependencyDigest = input => nativeCall("connection.directedDigest", input);

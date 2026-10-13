import { nativeCall } from "../src/native.mjs";
export const followupDiagnostics = (actual, expected) => nativeCall("delegation.followupDiagnostics", { actual, expected });

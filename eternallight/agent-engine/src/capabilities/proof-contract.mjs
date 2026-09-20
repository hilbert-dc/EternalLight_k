import { nativeCall } from "../native.mjs";

function freezeDeep(value) {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) freezeDeep(child);
    Object.freeze(value);
  }
  return value;
}
export const CAPABILITY_PROOF_CONTRACTS = freezeDeep(nativeCall("proof.contracts"));
export const requiredAssertionIds = (record) => nativeCall("proof.assertions", record);
export const evaluateCapabilityProof = (record, { expectedDependencies = {} } = {}) =>
  freezeDeep(nativeCall("proof.evaluate", { record, expectedDependencies }));

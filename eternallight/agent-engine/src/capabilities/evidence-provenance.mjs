import { defineEvidence } from "./evidence-store.mjs";

const VALIDATED_RECORDS = new WeakSet();

function freezeDeep(value) {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) freezeDeep(child);
    Object.freeze(value);
  }
  return value;
}

// Internal authority boundary: call only after a probe, certification validator,
// or report repository has verified the evidence. Not part of the public API.
export function registerValidatedEvidence(records) {
  return records.map((raw) => {
    const record = freezeDeep(defineEvidence(structuredClone(raw)));
    VALIDATED_RECORDS.add(record);
    return record;
  });
}

export function hasValidatedProvenance(records) {
  return Array.isArray(records) && records.every((record) => VALIDATED_RECORDS.has(record));
}

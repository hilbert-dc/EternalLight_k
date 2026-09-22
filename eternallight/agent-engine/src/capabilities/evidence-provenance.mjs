import { nativeBinding } from "../native.mjs";

const VALIDATED_RECORDS = new WeakMap();

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
    const native = new (nativeBinding().NativeEvidenceSnapshot)(JSON.stringify([raw]));
    const record = freezeDeep(JSON.parse(native.apply("snapshot", '{"createdAt":"1970-01-01T00:00:00.000Z"}')).records[0]);
    VALIDATED_RECORDS.set(record, native);
    return record;
  });
}

// WeakMap only associates JS exports with native owners; serialized or cloned
// objects cannot reconstruct a native provenance handle.
export function validatedRecords(records) {
  if (!hasValidatedProvenance(records)) throw new Error("records have no validated provenance");
  return records.map(record => JSON.parse(VALIDATED_RECORDS.get(record).apply("snapshot", '{"createdAt":"1970-01-01T00:00:00.000Z"}')).records[0]);
}

export function hasValidatedProvenance(records) {
  return Array.isArray(records) && records.every((record) => VALIDATED_RECORDS.has(record));
}

import { nativeBinding, nativeCall } from "../native.mjs";
import { hasValidatedProvenance, registerValidatedEvidence, validatedRecords } from "./evidence-provenance.mjs";

const TRUSTED_SNAPSHOTS = new WeakMap();
const TRUST = Symbol("native evidence owner");

function freezeSelection(selection) {
  return Object.freeze({
    records: Object.freeze(selection.records),
    summary: selection.summary,
    decisions: Object.freeze(selection.decisions),
  });
}

export class TrustedEvidenceRepository {
  #native;
  constructor(records = [], trust = null, native = null) {
    if (!Array.isArray(records)) throw new Error("evidence repository records must be an array");
    if (trust !== TRUST) throw new Error("trusted evidence must be loaded from reports or explicit test support");
    this.#native = native ?? new (nativeBinding().NativeEvidenceSnapshot)(JSON.stringify(records));
    this.records = Object.freeze(registerValidatedEvidence(records));
  }

  static async load({ records = [], reportStore }) {
    if (!Array.isArray(records)) throw new Error("evidence repository records must be an array");
    if (!reportStore || typeof reportStore.read !== "function") throw new Error("trusted evidence loading requires a report store");
    const reports = Object.create(null);
    for (const reference of nativeCall("evidence.reportRefs", records)) reports[reference] = await reportStore.read(reference);
    const native = nativeBinding().NativeEvidenceSnapshot.fromReports(JSON.stringify({ records, reports }));
    return new TrustedEvidenceRepository(records, TRUST, native);
  }

  resolveSnapshot({ now = new Date() } = {}) {
    if (!Number.isFinite(now.getTime())) throw new Error("evidence snapshot time is invalid");
    const exported = JSON.parse(this.#native.apply("snapshot", JSON.stringify({ createdAt: now.toISOString() })));
    const snapshot = Object.freeze({ ...exported, records: Object.freeze(registerValidatedEvidence(exported.records)) });
    TRUSTED_SNAPSHOTS.set(snapshot, this.#native);
    return snapshot;
  }
}

export function isTrustedEvidenceSnapshot(value) {
  return Boolean(value && TRUSTED_SNAPSHOTS.has(value));
}

export function compileEvidenceSession(snapshot, method, input) {
  if (!isTrustedEvidenceSnapshot(snapshot)) throw new Error("session compilation requires a trusted evidence snapshot");
  return JSON.parse(TRUSTED_SNAPSHOTS.get(snapshot).apply(method, JSON.stringify(input)));
}

export function ensureTrustedEvidenceSnapshot(value, { now = new Date() } = {}) {
  if (isTrustedEvidenceSnapshot(value)) return value;
  if (!hasValidatedProvenance(value)) throw new Error("resolver requires a trusted evidence snapshot or records with validated provenance");
  return new TrustedEvidenceRepository(validatedRecords(value), TRUST).resolveSnapshot({ now });
}

export function createUnverifiedEvidenceSnapshotForTesting(records, { now = new Date() } = {}) {
  return new TrustedEvidenceRepository(records, TRUST).resolveSnapshot({ now });
}

export function selectQualifiedEvidence(snapshot, {
  environmentDigest, expectedDependencies = {}, now = new Date(snapshot?.createdAt), requiredScope = null,
} = {}) {
  if (!isTrustedEvidenceSnapshot(snapshot)) throw new Error("evidence selection requires a trusted snapshot");
  const selection = JSON.parse(TRUSTED_SNAPSHOTS.get(snapshot).apply("select", JSON.stringify({ environmentDigest, expectedDependencies, now: now.getTime(), requiredScope })));
  return freezeSelection({ ...selection, summary: new Map(Object.entries(selection.summary)) });
}

export function hasActiveCredentialFailure(snapshot, credentialRealm, now = new Date(snapshot?.createdAt)) {
  if (!isTrustedEvidenceSnapshot(snapshot)) throw new Error("credential evidence lookup requires a trusted snapshot");
  return JSON.parse(TRUSTED_SNAPSHOTS.get(snapshot).apply("credentialFailure", JSON.stringify({ credentialRealm, now: now.getTime() })));
}

import { digest, stableJson } from "../contracts/capabilities.mjs";
import { defineEvidence, effectiveEvidence } from "./evidence-store.mjs";
import { nativeCall } from "../native.mjs";
import { hasValidatedProvenance, registerValidatedEvidence } from "./evidence-provenance.mjs";

const TRUSTED_SNAPSHOTS = new WeakSet();

function freezeSelection(selection) {
  return Object.freeze({
    records: Object.freeze(selection.records),
    summary: selection.summary,
    decisions: Object.freeze(selection.decisions),
  });
}

export class TrustedEvidenceRepository {
  constructor(records = [], trust = null) {
    if (!Array.isArray(records)) throw new Error("evidence repository records must be an array");
    if (trust !== TRUSTED_SNAPSHOTS) throw new Error("trusted evidence must be loaded from reports or explicit test support");
    this.records = Object.freeze(registerValidatedEvidence(records));
  }

  static async load({ records = [], reportStore }) {
    if (!Array.isArray(records)) throw new Error("evidence repository records must be an array");
    if (!reportStore || typeof reportStore.read !== "function") throw new Error("trusted evidence loading requires a report store");
    const reports = new Map();
    const comparable = ({ reportRef: _reportRef, reportDigest: _reportDigest, ...value }) => value;
    for (const raw of records) {
      const record = defineEvidence(raw);
      if (record.status !== "verified" && record.source !== "conformance-suite") continue;
      if (!record.reportRef || record.reportRef !== record.reportDigest) throw new Error(`evidence report binding is invalid: ${record.evidenceRef}`);
      if (!reports.has(record.reportRef)) reports.set(record.reportRef, await reportStore.read(record.reportRef));
      const report = reports.get(record.reportRef);
      if (report.reportId !== record.reportDigest) throw new Error(`evidence report digest mismatch: ${record.evidenceRef}`);
      const candidates = report.records?.filter((candidate) => candidate.evidenceRef === record.evidenceRef) ?? [];
      if (candidates.length !== 1 || stableJson(comparable(defineEvidence({ ...candidates[0], reportRef: record.reportRef, reportDigest: record.reportDigest }))) !== stableJson(comparable(record))) {
        throw new Error(`evidence is not bound to its report or fields have been tampered: ${record.evidenceRef}`);
      }
    }
    return new TrustedEvidenceRepository(records, TRUSTED_SNAPSHOTS);
  }

  resolveSnapshot({ now = new Date() } = {}) {
    if (!Number.isFinite(now.getTime())) throw new Error("evidence snapshot time is invalid");
    const records = Object.freeze(registerValidatedEvidence(effectiveEvidence(this.records)));
    const snapshot = Object.freeze({
      createdAt: now.toISOString(),
      digest: digest(records),
      records,
    });
    TRUSTED_SNAPSHOTS.add(snapshot);
    return snapshot;
  }
}

export function isTrustedEvidenceSnapshot(value) {
  return Boolean(value && TRUSTED_SNAPSHOTS.has(value));
}

export function ensureTrustedEvidenceSnapshot(value, { now = new Date() } = {}) {
  if (isTrustedEvidenceSnapshot(value)) return value;
  if (!hasValidatedProvenance(value)) throw new Error("resolver requires a trusted evidence snapshot or records with validated provenance");
  return new TrustedEvidenceRepository(value, TRUSTED_SNAPSHOTS).resolveSnapshot({ now });
}

export function createUnverifiedEvidenceSnapshotForTesting(records, { now = new Date() } = {}) {
  return new TrustedEvidenceRepository(records, TRUSTED_SNAPSHOTS).resolveSnapshot({ now });
}

export function selectQualifiedEvidence(snapshot, {
  environmentDigest, expectedDependencies = {}, now = new Date(snapshot?.createdAt), requiredScope = null,
} = {}) {
  if (!isTrustedEvidenceSnapshot(snapshot)) throw new Error("evidence selection requires a trusted snapshot");
  const selection = nativeCall("evidence.select", { records: snapshot.records, environmentDigest, expectedDependencies, now: now.getTime(), requiredScope });
  return freezeSelection({ ...selection, summary: new Map(Object.entries(selection.summary)) });
}

export function hasActiveCredentialFailure(snapshot, credentialRealm, now = new Date(snapshot?.createdAt)) {
  if (!isTrustedEvidenceSnapshot(snapshot)) throw new Error("credential evidence lookup requires a trusted snapshot");
  return nativeCall("evidence.credentialFailure", { records: snapshot.records, credentialRealm, now: now.getTime() });
}

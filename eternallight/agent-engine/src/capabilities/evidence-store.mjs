import { digest } from "../contracts/capabilities.mjs";
import { nativeBinding, nativeCall } from "../native.mjs";


function freezeDeep(value) {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) freezeDeep(child);
    Object.freeze(value);
  }
  return value;
}
export const defineEvidenceAssertion = (input) => freezeDeep(nativeCall("evidence.assertion", input));
export const defineEvidenceDependencies = (input) => freezeDeep(nativeCall("evidence.dependencies", input));
export const defineEvidence = (input) => freezeDeep(nativeCall("evidence.normalize", input));
export const normalizeEvidenceDraft = (input) => freezeDeep(nativeCall("evidence.draft", input));
export const scopeCovers = (actual, required) => nativeCall("evidence.covers", { actual, required });
export const effectiveEvidence = (records) => nativeCall("evidence.effective", records).map(freezeDeep);
export const summarizeEvidence = (records) => new Map(Object.entries(freezeDeep(nativeCall("evidence.summary", records))));
export const activeEvidence = (records, { environmentDigest, now = new Date() }) =>
  nativeCall("evidence.active", { records, environmentDigest, now: now.getTime() }).map(freezeDeep);
export const evidenceAdoptionStatus = (record, { expectedDependencies = null, now = new Date() } = {}) =>
  nativeCall("evidence.adoption", { record, expectedDependencies, now: now.getTime() });

export class CapabilityEvidenceStore {
  #native;
  #pending = Promise.resolve();
  constructor(path) { this.#native = new (nativeBinding().NativeEvidenceStore)(path); }
  #apply(method, records = []) {
    const result = this.#pending.then(async () => JSON.parse(await this.#native.apply(method, JSON.stringify(records))));
    this.#pending = result.catch(() => undefined);
    return result;
  }
  async read() { return (await this.#apply("read")).map(freezeDeep); }
  async replace(records) { return digest(await this.#apply("replace", records)); }
  async append(records) { return digest(await this.#apply("append", records)); }
}

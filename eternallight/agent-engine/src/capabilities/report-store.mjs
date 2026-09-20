import { contractHash, digest } from "../contracts/capabilities.mjs";
import { nativeBinding, nativeCall } from "../native.mjs";

export function defineCapabilityReport({ connection, result = null, error = null, redactions = [], startedAt, completedAt }) {
  // Error.message is non-enumerable, so explicitly marshal the public diagnostic fields.
  const failure = error ? { message: error.message, attempts: error.attempts, records: error.records } : null;
  const body = nativeCall("report.define", { connection, result, error: failure, redactions, startedAt, completedAt, contractDigest: contractHash() });
  return Object.freeze({ reportId: digest(body), ...body });
}

export class CapabilityReportStore {
  #native;
  constructor(directory, { maxReportBytes = 4 * 1024 * 1024 } = {}) {
    if (!Number.isInteger(maxReportBytes) || maxReportBytes < 1 || maxReportBytes > 32 * 1024 * 1024) throw new Error("capability report size limit is invalid");
    this.directory = directory;
    this.maxReportBytes = maxReportBytes;
    this.#native = new (nativeBinding().NativeReportStore)(directory);
  }
  async write(report) {
    const { reportId, ...body } = report ?? {};
    if (digest(body) !== reportId) throw new Error("capability report digest mismatch");
    return JSON.parse(await this.#native.apply("write", JSON.stringify({ report, maxReportBytes: this.maxReportBytes })));
  }
  async read(reportId) {
    const report = JSON.parse(await this.#native.apply("read", JSON.stringify({ reportId, maxReportBytes: this.maxReportBytes })));
    const { reportId: storedId, ...body } = report;
    if (storedId !== reportId || digest(body) !== reportId) throw new Error(`capability report digest mismatch: ${reportId}`);
    return Object.freeze(report);
  }
}

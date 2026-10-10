import { nativeBinding } from "../native.mjs";

/** A thin binding: SQLite, ownership fences and budget admission are native. */
export class PersistentMediaLedger {
  #native;
  constructor(databasePath, { clock = Date.now } = {}) {
    this.#native = new (nativeBinding().NativeMediaLedger)(databasePath);
    this.clock = clock;
  }
  #apply(method, input = {}) { return JSON.parse(this.#native.apply(method, JSON.stringify({ ...input, now: this.clock() }))); }
  prepare(grant, operation) { return this.#apply("prepare", { grant, operation }); }
  get(id) { return this.#apply("get", { id }); }
  find(id) { return this.#apply("find", { id }); }
  list(options = {}) { return this.#apply("list", options); }
  claim(id, ownerId, leaseMs = 300_000) { return this.#apply("claim", { id, ownerId, leaseMs }); }
  release(id) { return this.#apply("release", { id }); }
  requestCancel(id) { return this.#apply("cancel", { id }); }
  admit(record, {globalLimit=2,poolLimit=1}={}) {return this.#apply("admit",{id:record.id,ownerId:record.ownerId,fence:record.fence,globalLimit,poolLimit});}
  releaseAdmission(record) {return this.#apply("releaseAdmission",{id:record.id,ownerId:record.ownerId,fence:record.fence});}
  defer(record, {nextPollAt,errorCode}={}) {return this.#apply("defer",{id:record.id,ownerId:record.ownerId,fence:record.fence,expectedVersion:record.version,nextPollAt,errorCode});}
  transition(record, state, patch = {}) { return this.#apply("transition", { id: record.id, ownerId: record.ownerId, fence: record.fence, expectedVersion: record.version, state, patch }); }
  renew(record, leaseMs = 300_000) { return this.#apply("renew", { id: record.id, ownerId: record.ownerId, fence: record.fence, expectedVersion: record.version, leaseMs }); }
  settle(record, settlement) { return this.#apply("settle", { ...settlement, id: record.id, ownerId: record.ownerId, fence: record.fence, expectedVersion: record.version }); }
}

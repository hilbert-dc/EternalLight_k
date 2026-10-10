import { randomUUID } from "node:crypto";

const identifier = value => typeof value === "string" && value.length > 0 && value.trim() === value
  && Buffer.byteLength(value) <= 160 && !/[\p{Cc}]/u.test(value);
const integer = value => Number.isSafeInteger(value) && value >= 0;
const root = projectId => `/api/projects/${encodeURIComponent(projectId)}`;
const workPath = (projectId, workId) => `${root(projectId)}/work-tasks/${encodeURIComponent(workId)}`;
const eventPath = (projectId, eventId) => `${root(projectId)}/work-execution-events/${encodeURIComponent(eventId)}`;
const post = (request, path, body) => request(path, { method: "POST", body: JSON.stringify(body) });
const invalid = () => Object.assign(new Error("执行归属已失效；已保存结果仍保留，请核对原任务。"), { code: "WORK_EXECUTION_OWNER_INVALID" });

function validateWork(work, projectId, workId, threadId = work?.threadId) {
  if (!identifier(projectId) || !identifier(workId) || !identifier(threadId) || work?.id !== workId
    || work.projectId !== projectId || work.threadId !== threadId || !integer(work.version)
    || !["conversation", "direct", "generation"].includes(work.kind)
    || !["running", "completed", "paused", "cancelled", "blocked"].includes(work.status)) throw invalid();
  return work;
}

function validateLease(lease, hostSessionId, work) {
  if (!identifier(lease?.leaseId) || lease.hostSessionId !== hostSessionId || lease.workTaskId !== work.id
    || lease.rootThreadId !== work.threadId || !integer(lease.expiresAt) || lease.expiresAt * 1000 <= Date.now()) throw invalid();
  return lease;
}

// These in-memory records only renew Native leases. They are neither another
// Work state machine nor proof of provider completion or a not-sent request.
export class WorkExecutionOwners {
  #leases = new Map();
  #stopped = false;
  constructor({ request, invalidated = () => {}, hostSessionId = randomUUID(), renewalMs = 10_000 }) {
    if (!identifier(hostSessionId) || !integer(renewalMs) || renewalMs < 1 || renewalMs > 10_000) throw invalid();
    Object.assign(this, { request, invalidated, hostSessionId, renewalMs });
  }
  start() { this.#stopped = false; }
  #key(projectId, workId) { return JSON.stringify([projectId, workId]); }
  #track(projectId, work, lease, claim) {
    validateLease(lease, this.hostSessionId, work);
    const key = this.#key(projectId, work.id);
    const existing = this.#leases.get(key);
    if (existing && existing.lease.leaseId !== lease.leaseId) throw invalid();
    if (existing) return existing;
    const record = { key, projectId, work, lease, claim, failed: false, released: false, timer: null, renewing: null };
    this.#leases.set(key, record);
    this.#schedule(record);
    return record;
  }
  #schedule(record) {
    if (this.#stopped || record.failed || record.released) return;
    record.timer = setTimeout(() => {
      record.timer = null;
      void this.verify(record).catch(() => {}).finally(() => this.#schedule(record));
    }, this.renewalMs);
    record.timer.unref?.();
  }
  async acquire(projectId, work) {
    if (this.#stopped) throw invalid();
    validateWork(work, projectId, work?.id);
    const existing = this.#leases.get(this.#key(projectId, work.id));
    if (existing) {
      await this.verify(existing);
      return { record: existing, borrowed: true };
    }
    const lease = await post(this.request, `${workPath(projectId, work.id)}/execution-owner`, {
      hostSessionId: this.hostSessionId, expectedWorkVersion: work.version,
    });
    if (this.#stopped) {
      validateLease(lease, this.hostSessionId, work);
      await post(this.request, `${workPath(projectId, work.id)}/execution-owner/release`, {
        hostSessionId: this.hostSessionId, leaseId: lease.leaseId,
      });
      throw invalid();
    }
    return { record: this.#track(projectId, work, lease, false), borrowed: false };
  }
  adoptClaim(projectId, work, lease, event) {
    if (this.#stopped) throw invalid();
    validateWork(work, projectId, work?.id);
    if (event?.workTaskId !== work.id || event.rootThreadId !== work.threadId || !identifier(event.claimId)
      || event.hostSessionId !== this.hostSessionId) throw invalid();
    return this.#track(projectId, work, lease, Object.freeze({ eventId: event.eventId, claimId: event.claimId,
      sourceStepId: event.sourceStepId, sourceAttemptId: event.sourceAttemptId }));
  }
  async verify(record) {
    if (this.#stopped || !record || record.failed || record.released
      || this.#leases.get(record.key) !== record) throw invalid();
    if (record.renewing) return record.renewing;
    record.renewing = (async () => {
      try {
        const lease = await post(this.request, `${workPath(record.projectId, record.work.id)}/execution-owner/renew`, {
          hostSessionId: this.hostSessionId, leaseId: record.lease.leaseId,
        });
        validateLease(lease, this.hostSessionId, record.work);
        if (lease.leaseId !== record.lease.leaseId || record.released) throw invalid();
        record.lease = lease;
      } catch (error) {
        record.failed = true;
        clearTimeout(record.timer);
        if (!record.released) await this.invalidated(record, error);
        throw invalid();
      }
    })().finally(() => { record.renewing = null; });
    return record.renewing;
  }
  async release(record, { native = true } = {}) {
    if (!record || record.released) return;
    record.released = true;
    clearTimeout(record.timer);
    if (this.#leases.get(record.key) === record) this.#leases.delete(record.key);
    await record.renewing?.catch(() => {});
    if (native) await post(this.request, `${workPath(record.projectId, record.work.id)}/execution-owner/release`, {
      hostSessionId: this.hostSessionId, leaseId: record.lease.leaseId,
    });
  }
  async stop() {
    this.#stopped = true;
    return Promise.allSettled([...this.#leases.values()].map(record => this.release(record)));
  }
}

function validateEvent(event, projectId) {
  if (!identifier(projectId) || !identifier(event?.eventId) || !identifier(event.workTaskId)
    || !identifier(event.sourceStepId) || !identifier(event.sourceAttemptId) || !identifier(event.rootThreadId)
    || event.kind !== "attemptSettled" || !integer(event.workVersion) || !integer(event.createdAt)
    || !["pending", "claimed", "consumed"].includes(event.state)) throw invalid();
  return event;
}

// A wake delegates to the existing original-Work executor. No literary action,
// new author message, budget or retry authority is chosen by this consumer.
export class WorkExecutionEventConsumer {
  #running = null;
  #active = new Map();
  #timer = null;
  #started = false;
  #epoch = 0;
  #cursors = new Map();
  #projectOffset = 0;
  #health = new Map();
  constructor({ request, owners, available, readThread, resume, runInContext = operation => operation(), prepareConditions = async () => {}, canResume = () => true, drainMessages = async () => false, updated = () => {}, failed = () => {}, healthChanged = () => {}, now = Date.now, intervalMs = 1_000, maxActive = 4 }) {
    if (!Number.isSafeInteger(maxActive) || maxActive < 1 || maxActive > 16) throw invalid();
    Object.assign(this, { request, owners, available, readThread, resume, runInContext, prepareConditions, canResume, drainMessages, updated, failed, healthChanged, now, intervalMs, maxActive });
  }
  get busy() { return this.#running !== null || this.#active.size > 0; }
  async settled() {
    await this.#running;
    await Promise.allSettled([...this.#active.values()].map(record => record.completion));
  }
  health() { return [...this.#health.values()].map(value => ({ ...value })); }
  #healthKey(projectId, event) { return JSON.stringify([projectId, event?.eventId ?? null]); }
  #due(projectId, event) { return (this.#health.get(this.#healthKey(projectId, event))?.nextCheckAt ?? 0) <= this.now(); }
  async #recordFailure(projectId, event, error) {
    const key = this.#healthKey(projectId, event), previous = this.#health.get(key);
    const failures = (previous?.failures ?? 0) + 1;
    const value = { projectId, workTaskId: event?.workTaskId ?? null, eventId: event?.eventId ?? null,
      phase: event ? "resume" : "scan", state: "waiting", failures,
      code: typeof error?.code === "string" && /^[a-zA-Z][a-zA-Z0-9_]{0,79}$/.test(error.code) ? error.code : null,
      observedAt: this.now(), nextCheckAt: this.now() + Math.min(60_000, this.intervalMs * 2 ** Math.min(failures, 6)) };
    this.#health.set(key, value);
    await this.healthChanged(this.health());
    await this.failed(projectId, event, error);
  }
  async #clearFailure(projectId, event) {
    if (this.#health.delete(this.#healthKey(projectId, event))) await this.healthChanged(this.health());
  }
  start() {
    if (this.#started) return;
    this.#started = true;
    this.#cursors.clear(); this.#projectOffset = 0;
    this.poke();
  }
  poke() {
    if (!this.#started || this.#timer) return;
    this.#timer = setTimeout(() => {
      this.#timer = null;
      void this.tick().finally(() => this.poke());
    }, this.intervalMs);
    this.#timer.unref?.();
  }
  async stop() {
    this.#started = false;
    this.#epoch += 1;
    clearTimeout(this.#timer); this.#timer = null;
    // An active provider request remains governed by the Engine/recovery
    // ledger; shutdown must not invent a completed consumer receipt.
  }
  async tick() {
    if (this.#running) return this.#running;
    const epoch = this.#epoch;
    this.#running = this.runInContext(() => this.#drain(epoch)).catch(error => this.#recordFailure(null, null, error))
      .finally(() => { this.#running = null; });
    return this.#running;
  }
  #current(epoch) { return epoch === this.#epoch; }
  async #ready(epoch) { return this.#current(epoch) && await this.available() && this.#current(epoch); }
  async #drain(epoch) {
    if (!this.#due(null, null) || !await this.#ready(epoch)) return;
    const directory = await this.request("/api/projects");
    if (!Array.isArray(directory?.projects) || directory.projects.some(project => !identifier(project?.id))) throw invalid();
    const projects = directory.projects;
    // One bounded page per project per pass; a stopped oldest event must not
    // starve newer events or other projects. Every completed scan returns head.
    const scanned = Math.min(4, projects.length);
    const offset = this.#projectOffset;
    for (let index = 0; index < scanned && this.#current(epoch); index += 1) {
      const projectId = projects[(offset + index) % projects.length].id;
      if (!await this.#ready(epoch)) { this.#projectOffset = (offset + index) % projects.length; return; }
      if (await this.drainMessages(projectId)) {
        this.#projectOffset=(offset+index+1)%projects.length;
        return;
      }
      const cursor = this.#cursors.get(projectId) ?? null;
      const query = new URLSearchParams({ limit: "20" });
      if (cursor !== null) query.set("cursor", cursor);
      const page = await this.request(`${root(projectId)}/work-execution-events?${query}`);
      if (page?.projectId !== projectId || !Array.isArray(page.items) || page.items.length > 20
        || !(page.nextCursor === null || typeof page.nextCursor === "string" && page.nextCursor.length > 0
          && Buffer.byteLength(page.nextCursor) <= 512 && !/[\p{Cc}]/u.test(page.nextCursor))
        || page.nextCursor !== null && page.nextCursor === cursor) throw invalid();
      this.#cursors.set(projectId, page.nextCursor);
      for (const item of page.items) {
        const event = validateEvent(item, projectId);
        if (event.state === "consumed") continue;
        if (!await this.#ready(epoch)) { this.#projectOffset = (offset + index + 1) % projects.length; return; }
        if (!this.#due(projectId, event)) continue;
        const key = JSON.stringify([projectId, event.workTaskId]);
        if (this.#active.has(key) || [...this.#active.values()].some(record => record.projectId === projectId
          && record.threadId === event.rootThreadId) || this.#active.size >= this.maxActive) continue;
        // Reserve before the first asynchronous preparation. Native claims and
        // ExecutionDomains still decide authority and actual model capacity.
        const record = { projectId, threadId: event.rootThreadId, completion: null };
        this.#active.set(key, record);
        record.completion = this.#consume(projectId, event, epoch)
          .then(() => this.#clearFailure(projectId, event))
          .catch(error => this.#recordFailure(projectId, event, error))
          .finally(() => { this.#active.delete(key); this.poke(); });
      }
    }
    await this.#clearFailure(null, null);
    if (projects.length) this.#projectOffset = (offset + scanned) % projects.length;
  }
  async #consume(projectId, event, epoch) {
    if (!this.#current(epoch) || !await this.canResume(projectId,event.workTaskId) || !this.#current(epoch)) return;
    const path = workPath(projectId, event.workTaskId);
    await this.prepareConditions(projectId, event.workTaskId);
    const work = validateWork(await this.request(path), projectId, event.workTaskId, event.rootThreadId);
    if (!["running", "completed", "blocked"].includes(work.status)) return;
    if (!Array.isArray(work.steps)) throw invalid();
    const priorAttempts = new Set(work.steps.flatMap(step => step.attempts?.map(attempt => attempt.id) ?? []));
    const thread = await this.readThread(projectId, work.threadId);
    if (thread?.id !== work.threadId) throw invalid();
    const workspace = await this.request(`${root(projectId)}/creative-workspace`);
    if (!integer(workspace?.revision) || !identifier(workspace.sourceDigest) || !await this.#ready(epoch)) return;
    const admission = await post(this.request, `${eventPath(projectId, event.eventId)}/claim`, {
      hostSessionId: this.owners.hostSessionId, expectedWorkVersion: work.version,
      expectedProjectRevision: workspace.revision, expectedSourceDigest: workspace.sourceDigest,
    });
    const claimed = validateEvent(admission?.event, projectId);
    validateWork(admission.work, projectId, work.id, work.threadId);
    if (claimed.eventId !== event.eventId || claimed.sourceAttemptId !== event.sourceAttemptId
      || claimed.sourceStepId !== event.sourceStepId || claimed.workTaskId !== work.id || claimed.rootThreadId !== work.threadId
      || !["blocked", "claimed", "settled"].includes(admission.admission)) throw invalid();
    if (admission.admission !== "claimed") {
      if (admission.lease !== null || admission.admission === "settled" && claimed.state !== "consumed") throw invalid();
      return;
    }
    if (claimed.state !== "claimed" || !identifier(claimed.claimId) || claimed.hostSessionId !== this.owners.hostSessionId) throw invalid();
    const lease = validateLease(admission.lease, this.owners.hostSessionId, work);
    const identity = { hostSessionId: this.owners.hostSessionId, claimId: claimed.claimId, leaseId: lease.leaseId };
    let record = null;
    let acknowledged = false;
    try {
      record = this.owners.adoptClaim(projectId, work, lease, claimed);
      if (!await this.#ready(epoch) || !await this.canResume(projectId,work.id)) return;
      await this.resume(projectId, work.id, { origin: "automatic", expectedThreadId: work.threadId,
        durableWake: { eventId: event.eventId, sourceStepId: event.sourceStepId, sourceAttemptId: event.sourceAttemptId,
          claimId: claimed.claimId, leaseId: lease.leaseId, hostSessionId: this.owners.hostSessionId } });
      if (!this.#current(epoch)) return;
      await this.owners.verify(record);
      const current = validateWork(await this.request(path), projectId, work.id, work.threadId);
      if (!["running", "completed"].includes(current.status)) return;
      if (!Array.isArray(current.steps)) throw invalid();
      const source = current.steps.find(step => step.id === event.sourceStepId
        && step.attempts?.some(attempt => attempt.id === event.sourceAttemptId));
      if (!source) throw invalid();
      const subsequent = current.steps.find(step => step.attempts?.some(attempt => attempt.id === step.currentAttemptId
        && !priorAttempts.has(attempt.id)));
      // Native checks owner/claim, author controls and a same-Work Attempt
      // attached after the source, or existing final/handoff evidence. Event
      // consumption proves that handoff, not successor completion or settled
      // usage; unknown outcomes remain in the original execution ledger.
      if (!subsequent && current.status !== "completed") return;
      const receipt = await post(this.request, `${eventPath(projectId, event.eventId)}/ack`, {
        ...identity, consumerStepId: subsequent?.id ?? null,
      });
      const settled = validateEvent(receipt, projectId);
      if (settled.eventId !== event.eventId || settled.workTaskId !== work.id || settled.state !== "consumed") throw invalid();
      acknowledged = true;
      await this.updated(projectId, settled, current);
    } finally {
      try {
        if (!acknowledged) await post(this.request, `${eventPath(projectId, event.eventId)}/release`, identity);
      } finally {
        // Release deletes the Native claim lease; ack keeps it long enough to
        // persist consumption, so release that exact owner after success.
        if (record) await this.owners.release(record, { native: acknowledged });
      }
    }
  }
}

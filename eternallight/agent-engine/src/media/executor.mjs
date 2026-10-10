import { randomUUID } from "node:crypto";

const TERMINAL = new Set(["ready", "failed", "cancelled"]);
const transientCode = error => /^[A-Z0-9_:-]{1,100}$/.test(error?.code ?? "") ? error.code : "MEDIA_TRANSPORT_UNCERTAIN";

/** One bounded execution/reconciliation step. The existing Host owns wakeups. */
export class MediaOperationExecutor {
  constructor({ ledger, adapters, authorize, admitResource, collectOutputs, lookupOutputs, clock = Date.now, ownerId = randomUUID(), leaseMs = 300_000 }) {
    for (const [name, fn] of Object.entries({ authorize, admitResource, collectOutputs, lookupOutputs })) {
      if (typeof fn !== "function") throw new Error(`media executor requires ${name}`);
    }
    Object.assign(this, { ledger, adapters: new Map(adapters.map(adapter => [adapter.id, adapter])), authorize, admitResource, collectOutputs, lookupOutputs, clock, ownerId, leaseMs });
    this.active = new Map();
  }
  async cancel(id) {
    const record = this.ledger.requestCancel(id);
    // Persist intent before signalling IO; abort cannot prove remote cancellation.
    this.active.get(id)?.abort();
    return record;
  }
  async step(id, { signal } = {}) {
    let current = this.ledger.get(id);
    if (TERMINAL.has(current.state)) return current;
    if (this.active.has(id)) throw new Error("media operation is already executing");
    current = this.ledger.claim(id, this.ownerId, this.leaseMs);
    const controller = new AbortController();
    const abort = () => controller.abort();
    signal?.addEventListener("abort", abort, { once: true });
    if (signal?.aborted) controller.abort();
    this.active.set(id, controller);
    const read = () => this.ledger.get(id);
    const move = (state, patch = {}) => (current = this.ledger.transition(read(), state, patch));
    const defer = patch => (current = this.ledger.defer(read(), patch));
    let ownershipError = null;
    const renewal = setInterval(() => {
      try { this.ledger.renew(read(), this.leaseMs); }
      catch(error) { ownershipError=error; controller.abort(); }
    }, Math.max(10,Math.floor(this.leaseMs/3)));
    renewal.unref?.();
    let release = null;
    try {
      if (current.state === "prepared" && read().cancelRequested) {
        current = move("cancelled", { errorCode: "CANCELLED_BEFORE_DISPATCH" });
        return this.ledger.settle(current, { receiptId: `not-sent:${id}`, outcome: "not_sent", usage: { units: {}, costMinor: 0 } });
      }
      if (controller.signal.aborted) return defer({nextPollAt:this.clock()+5000,errorCode:"MEDIA_IO_INTERRUPTED"});
      const adapter = this.adapters.get(current.providerId);
      if (!adapter) throw Object.assign(new Error("media service is not configured"), { code: "MEDIA_SERVICE_UNAVAILABLE", notSent: true });
      const capability = adapter.inspect().capabilities.find(item => item.kind === current.kind);
      if (!capability?.available) throw Object.assign(new Error(capability?.reason ?? "media capability is unavailable"), { code: "MEDIA_CAPABILITY_UNAVAILABLE", notSent: true });
      // Domain authority allows read-only reconciliation after cancellation or
      // expiry, while denying any new submission or application to current draft.
      await this.authorize(current, { phase: current.state === "prepared" ? "dispatch" : "reconcile" });
      if (current.state === "collecting" || current.state === "outcome_unknown") {
        const saved = await this.lookupOutputs(current);
        if (saved) return move("ready", { outputAssetRevisionIds: saved.assetRevisionIds, resultReceipt: saved.receipt });
      }
      if (current.nextPollAt > this.clock() && !read().cancelRequested) return current;
      if (current.state === "prepared") await adapter.validate(current.request, current.kind);
      // The shared Host admission covers provider/account/local resource limits.
      const admission = await this.admitResource(current, { signal: controller.signal });
      if(admission?.admitted===false) return defer({nextPollAt:Math.max(this.clock()+1000,admission.retryAt??this.clock()+5000)});
      release=admission;
      if (typeof release !== "function") throw new Error("media admission did not return a release function");
      let result;
      if (current.state === "prepared") {
        await this.authorize(read(), { phase: "dispatch" });
        controller.signal.throwIfAborted();
        current = move("submitting");
        result = await adapter.submit(current.request, { kind: current.kind, operationId: id, idempotencyKey: current.idempotencyKey, signal: controller.signal });
      } else if (current.state === "submitting" || current.state === "outcome_unknown" || (current.state === "collecting" && !current.providerJobId)) {
        if (current.state !== "outcome_unknown") current = move("outcome_unknown", { errorCode: "SUBMISSION_OUTCOME_UNKNOWN" });
        result = await adapter.lookup({ jobId: current.providerJobId, idempotencyKey: current.idempotencyKey, kind: current.kind }, { signal: controller.signal });
      } else {
        result = await adapter.poll(current.providerJobId, { signal: controller.signal });
      }
      if (ownershipError) throw ownershipError;
      if (!result || result.state === "outcome_unknown") return move("outcome_unknown", { errorCode: "SUBMISSION_OUTCOME_UNKNOWN", nextPollAt:this.clock()+Math.max(5000,result?.pollAfterMs??30000) });
      if (result.jobId) current = move(result.state === "collecting" ? "collecting" : result.state, { providerJobId: result.jobId, receipt: result.receipt ?? null });
      else current = move(result.state, { receipt: result.receipt ?? null });
      // User cancellation can arrive during submit, including before jobId exists.
      if (read().cancelRequested && ["submitted", "running"].includes(current.state)) {
        const cancelled = await adapter.cancel(current.providerJobId, {});
        if (["cancelled","failed"].includes(cancelled?.state)) { result=cancelled;current = move(cancelled.state, { receipt: cancelled.receipt ?? current.receipt }); }
        else if (cancelled?.state === "collecting") { result = cancelled; current = move("collecting", { receipt: result.receipt ?? current.receipt }); }
      }
      if (current.state === "collecting") {
        // A cancelled remote job may already have produced output. Save its
        // original version; collectOutputs has no authority to advance a draft.
        const outputs = await adapter.fetchOutputs(result, {signal:controller.signal});
        const saved = await this.collectOutputs(current, outputs);
        current = move("ready", { outputAssetRevisionIds: saved.assetRevisionIds, resultReceipt: saved.receipt });
      } else if (["submitted", "running"].includes(current.state)) {
        current = move(current.state, { nextPollAt: this.clock() + Math.max(5000, result.pollAfterMs ?? 5000) });
      }
      if (TERMINAL.has(current.state)) {
        current = this.ledger.settle(read(), { receiptId: result.receipt?.id ?? `remote:${id}`, outcome: result.usage?.costMinor == null ? "unknown" : "known", usage: result.usage ?? { units: {}, costMinor: null } });
      }
      return current;
    } catch (error) {
      current = read();
      if (current.state === "prepared") {
        if(controller.signal.aborted&&!current.cancelRequested)return defer({nextPollAt:this.clock()+5000,errorCode:"MEDIA_IO_INTERRUPTED"});
        const cancelled = current.cancelRequested;
        current = move(cancelled ? "cancelled" : "failed", { errorCode: cancelled ? "CANCELLED_BEFORE_DISPATCH" : transientCode(error) });
        return this.ledger.settle(current, { receiptId: `not-sent:${id}`, outcome: "not_sent", usage: { units: {}, costMinor: 0 } });
      }
      if (!TERMINAL.has(current.state)) {
        if (error?.rejected === true && current.state === "submitting") return move("failed", { errorCode: transientCode(error) });
        return move("outcome_unknown", { errorCode: transientCode(error), nextPollAt: this.clock() + 5000 });
      }
      throw error;
    } finally {
      clearInterval(renewal);
      if (release) release();
      signal?.removeEventListener("abort", abort);
      this.active.delete(id);
      this.ledger.release(id);
    }
  }
}

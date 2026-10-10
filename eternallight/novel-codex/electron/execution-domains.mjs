import { AsyncLocalStorage } from "node:async_hooks";
import { randomUUID } from "node:crypto";

// Bound at trusted entry points. Never chooses an owner from the visible UI.
export class ExecutionDomains {
  #storage = new AsyncLocalStorage();
  #domains = new Set();
  #closing = false;
  constructor({ base, prepare, dispose, maxActive = 4 }) {
    if (!Number.isSafeInteger(maxActive) || maxActive<1 || maxActive>16) throw Error("执行资源上限无效");
    Object.assign(this, { base, prepare, dispose, maxActive });
  }
  hasCapacity() { return !this.#closing && this.#domains.size<this.maxActive; }
  current() { return this.#storage.getStore() ?? this.base; }
  active() { return [...this.#domains]; }
  find({ projectId, threadId, workId, taskId, requestId }) {
    if (![requestId, workId, threadId, taskId].some(Boolean)) return null;
    return this.active().find(domain => domain.projectId === projectId
      && (!requestId || domain.requestId === requestId)
      && (!workId || domain.workId === workId || domain.activeWorkFlow?.id === workId)
      && (!threadId || domain.threadId === threadId)
      && (!taskId || domain.activeRuntimeTask?.id === taskId)) ?? null;
  }
  enter(domain, callback) {
    if (domain !== this.base && !this.#domains.has(domain)) throw Error("原执行域已结束");
    return this.#storage.run(domain, callback);
  }
  bind(callback) { const owner = this.current(); return (...args) => owner.closed ? undefined : this.enter(owner, () => callback(...args)); }
  async run(identity, callback) {
    if (this.#closing) throw Error("应用正在关闭，不能启动新执行");
    if (this.current() !== this.base) return callback();
    if (identity.threadId && this.find({projectId: identity.projectId, threadId: identity.threadId})) throw Error("该对话已有任务正在运行");
    if (!this.hasCapacity()) throw Object.assign(Error("并行任务已达上限，请等待一个任务结束；输入已保留。"),{code:"EXECUTION_CAPACITY"});
    const domain = { ...identity, id: randomUUID(), closed: false, activeAgentExecution: null,
      activeRuntimeTask: null, activeWorkFlow: null, activeGenerationRunId: null,
      activeAgentProjectId: null, activeAgentPurpose: null, activeAgentTargetKey: null,
      advisorySteerInFlight: false, advisorySteerCompletion: null, recoveryRefreshPromise: null,
      agentWebContents: null, removeAgentWebContentsListener: null, recoveryJournal: null };
    this.#domains.add(domain);
    const completion = this.#storage.run(domain, async () => {
      try {
        await this.prepare(domain);
        domain.prepared = true;
        if(this.#closing)throw Object.assign(Error("应用正在关闭，准备中的执行已停止"),{code:"TASK_CANCELLED"});
        return await callback();
      }
      catch (error) { domain.failure = {message: error?.message ?? "执行未完成", code: error?.code ?? null}; throw error; }
      finally {
        domain.settled = true;
        try { await this.dispose(domain); }
        finally { domain.closed = true; this.#domains.delete(domain); }
      }
    });
    domain.completion = completion;
    return completion;
  }
  reopen() {
    if (this.#domains.size) throw Error("原执行尚未清理完成");
    this.#closing = false;
  }
  async closeAll() {
    this.#closing = true;
    await Promise.allSettled(this.active().map(domain => this.enter(domain, async () => {
      // A preparing domain cleans up after its preparation settles. Disposing it
      // earlier could leave resources created after disposal permanently alive.
      if(domain.prepared) await this.dispose(domain);
      await domain.completion;
    })));
  }
}

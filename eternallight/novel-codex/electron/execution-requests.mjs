// @ts-check
import { AsyncLocalStorage } from "node:async_hooks";

// Request identity belongs to Host lifecycle. Work/Attempt permissions remain Native facts.
export const EXECUTION_CHANNELS = new Set([
  "agent:send-prompt", "agent:approve-action", "agent:confirm-execution-scope", "agent:generate-planning", "agent:generate-work",
  "agent:continue-work", "agent:retry-work", "agent:resolve-generation-feedback",
  "agent:generate-chapter", "agent:resume-knowledge", "agent:resume-work-task",
  "agent:recover-work-task", "agent:review-planning", "agent:review-quality-packet",
  "agent:review-thread", "agent:compact-thread",
]);

const validId = value => typeof value === "string" && value.length > 0 && value.length <= 160 && !/[\p{Cc}]/u.test(value);
const stopped = () => Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });

export class ExecutionRequests {
  #context = new AsyncLocalStorage();
  #active = new Map();

  current() { return this.#context.getStore() ?? null; }

  detached(operation) { return this.#context.exit(operation); }

  assertActive() {
    const request = this.current();
    if (request?.stopRequested) throw stopped();
    return request;
  }

  wrap(channel, listener) {
    if (!EXECUTION_CHANNELS.has(channel)) return listener;
    return async (event, envelope) => {
      if (!envelope || Object.keys(envelope).sort().join(",") !== "args,requestId"
        || !validId(envelope.requestId) || !Array.isArray(envelope.args)) {
        throw new Error("执行请求身份无效");
      }
      if (!validId(envelope.args[0])) throw new Error("作品 ID 无效");
      if (this.#active.has(envelope.requestId)) throw new Error("执行请求已提交");
      const request = { channel, requestId: envelope.requestId, projectId: envelope.args[0], sender: event.sender,
        stopRequested: false, stopPromise: null, flow: null, execution: null };
      this.#active.set(request.requestId, request);
      try { return await this.#context.run(request, () => listener(event, ...envelope.args)); }
      finally { this.#active.delete(request.requestId); }
    };
  }

  async stop(event, target, interrupt) {
    if (!target || Object.keys(target).sort().join(",") !== "projectId,requestId"
      || !validId(target.requestId) || !validId(target.projectId)) throw new Error("停止请求必须指定原执行");
    const request = this.#active.get(target.requestId);
    if (!request) return { interrupted: false };
    if (request.projectId !== target.projectId || request.sender !== event.sender) throw new Error("停止目标不属于原执行");
    if (request.stopPromise) return request.stopPromise;
    request.stopRequested = true;
    request.stopPromise = Promise.resolve().then(() => interrupt(request));
    try { return await request.stopPromise; }
    catch (error) { request.stopPromise = null; throw error; }
  }
}

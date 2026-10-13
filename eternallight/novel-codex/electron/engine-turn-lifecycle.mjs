// @ts-check
import { terminateEngineProcess } from "./engine-process-lifecycle.mjs";
import { isBoundedOutputFailure } from "./bounded-output-correction.mjs";
import { coreTurnFailure } from "./core-turn-failure.mjs";

// Ownership spans preparation, the stream, and follow-up RPCs. Stream completion
// alone must not unlock another turn while the previous caller is still running.
export async function executeManagedTurn(adapter, { prepare, start, afterStart = undefined, maintenance = undefined, continueAfterCompaction = undefined }) {
  if (adapter.turnOperation || adapter.turnWaiter) throw new Error("Novel Agent Engine 正在处理上一轮请求");
  const operation = {};
  adapter.turnOperation = operation;
  let child;
  /** @type {{resolve: (value: unknown) => void, reject: (error: unknown) => void, threadId: string, turnId: string | null, agentMessageItemId: string | null, agentMessage: string, timer?: ReturnType<typeof setTimeout>} | undefined} */
  let waiter;
  const assertCurrent = () => {
    if (adapter.turnOperation !== operation || adapter.process !== child) throw new Error("创作引擎回合已结束或会话已重新绑定");
  };
  try {
    const preparation = prepare();
    child = adapter.process;
    const threadId = await preparation;
    assertCurrent();
    const runStage = async (begin, after = undefined) => {
      const completion = new Promise((resolve, reject) => {
        waiter = { resolve, reject, threadId, turnId: null, agentMessageItemId: null, agentMessage: "" };
        adapter.turnWaiter = waiter;
      });
      void completion.catch(() => {});
      const started = await begin(threadId);
      assertCurrent();
      // A complete stream can arrive before the start RPC continuation. Never
      // resurrect a completed turn ID or mutate a replacement waiter's state.
      if (waiter && adapter.turnWaiter === waiter) {
        adapter.turnId = started?.turn?.id ?? waiter.turnId;
        waiter.turnId = adapter.turnId;
      }
      if (after) {
        await after(threadId);
        assertCurrent();
      }
      const result = await completion;
      assertCurrent();
      if (result?.turn?.status === "completed") adapter.lastError = null;
      return result;
    };
    if (maintenance?.due(threadId)) {
      const result = await runStage(id => adapter.request("thread/compact/start", { threadId: id }));
      if (result?.turn?.status !== "completed") return result;
      maintenance.completed(threadId);
      assertCurrent();
      if (adapter.executionLease?.revoked) throw new Error("创作引擎执行授权已撤销");
    }
    try {
      return await runStage(start, afterStart);
    } catch (error) {
      const failure = coreTurnFailure(error);
      if (!continueAfterCompaction || !failure?.contextWindowExceeded || failure.threadId !== threadId) throw error;
      assertCurrent();
      if (adapter.executionLease?.revoked) throw error;
      // Official Core has ended the failed turn. Continue the same operation,
      // thread and lease through public RPCs, never replay the author's input or
      // a tool call. This is a new Core turn, not a resumed sampling step.
      adapter.emit({ method: "agent/contextRecovery", params: {
        threadId, failedTurnId: failure.turnId, phase: "compacting" } });
      const compacted = await runStage(id => adapter.request("thread/compact/start", { threadId: id }));
      if (compacted?.turn?.status !== "completed") return compacted;
      maintenance?.completed(threadId);
      assertCurrent();
      if (adapter.executionLease?.revoked) throw new Error("创作引擎执行授权已撤销");
      // No loop: another capacity rejection or failed compaction is terminal.
      return await runStage(continueAfterCompaction);
    }
  } catch (error) {
    const terminal = coreTurnFailure(error);
    if (terminal && adapter.turnOperation === operation && adapter.process === child) adapter.lastError = error.message;
    if (!terminal && !isBoundedOutputFailure(error) && adapter.turnOperation === operation && child && adapter.process === child) {
      adapter.reader?.close();
      adapter.process = null;
      adapter.reader = null;
      adapter.threadId = null;
      if (adapter.lastError !== error.message) adapter.fail(error);
      await terminateEngineProcess(child);
    }
    throw error;
  } finally {
    if (waiter) clearTimeout(waiter.timer);
    if (adapter.turnWaiter === waiter) adapter.turnWaiter = null;
    if (adapter.turnOperation === operation) adapter.turnOperation = null;
  }
}

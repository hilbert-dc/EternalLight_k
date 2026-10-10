import { terminateEngineProcess } from "./engine-process-lifecycle.mjs";
import { isBoundedOutputFailure } from "./bounded-output-correction.mjs";

// Ownership spans preparation, the stream, and follow-up RPCs. Stream completion
// alone must not unlock another turn while the previous caller is still running.
export async function executeManagedTurn(adapter, { prepare, start, afterStart }) {
  if (adapter.turnOperation || adapter.turnWaiter) throw new Error("Novel Agent Engine 正在处理上一轮请求");
  const operation = {};
  adapter.turnOperation = operation;
  let child, waiter;
  const assertCurrent = () => {
    if (adapter.turnOperation !== operation || adapter.process !== child) throw new Error("创作引擎回合已结束或会话已重新绑定");
  };
  try {
    const preparation = prepare();
    child = adapter.process;
    const threadId = await preparation;
    assertCurrent();
    const completion = new Promise((resolve, reject) => {
      waiter = { resolve, reject, threadId, turnId: null, agentMessageItemId: null, agentMessage: "" };
      adapter.turnWaiter = waiter;
    });
    void completion.catch(() => {});
    const started = await start(threadId);
    assertCurrent();
    // A complete stream can arrive before the start RPC continuation. Never
    // resurrect a completed turn ID or mutate a replacement waiter's state.
    if (adapter.turnWaiter === waiter) {
      adapter.turnId = started?.turn?.id ?? waiter.turnId;
      waiter.turnId = adapter.turnId;
    }
    if (afterStart) {
      await afterStart(threadId);
      assertCurrent();
    }
    const result = await completion;
    assertCurrent();
    return result;
  } catch (error) {
    if (!isBoundedOutputFailure(error) && adapter.turnOperation === operation && child && adapter.process === child) {
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

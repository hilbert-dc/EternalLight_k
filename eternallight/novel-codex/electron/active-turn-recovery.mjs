// @ts-check
import { selectFinalReply } from "./agent-final-reply.mjs";
const pending = new WeakMap();

// Reconcile an existing waiter; never start or resume a second model execution.
export async function readActiveTurn(adapter, threadId, turnId, isCurrent, onInterrupted) {
  if (pending.has(adapter)) throw new Error("正在核对当前回合连接，请稍候");
  const process = adapter.process, waiter = adapter.turnWaiter, operation = adapter.turnOperation;
  const assertCurrent = () => {
    if (!turnId || !waiter || !operation || !process || !isCurrent()
      || adapter.process !== process || adapter.turnOperation !== operation || adapter.turnWaiter !== waiter
      || waiter.threadId !== threadId || waiter.turnId !== turnId) throw new Error("当前回合已变化，请重新读取任务状态");
  };
  assertCurrent();
  const owner = {}; pending.set(adapter, owner);
  try {
    const result = await adapter.request("thread/read", { threadId, includeTurns: true });
    assertCurrent();
    const turn = result?.thread?.turns?.find(turn => turn.id === turnId);
    if (result?.thread?.id !== threadId || !["inProgress", "completed", "failed", "interrupted"].includes(turn?.status)) {
      throw new Error("尚未核实当前回合状态；原任务仍保留，请等待或停止");
    }
    const state = turn.status === "inProgress"
      ? adapter.userInputs.size > 0 || result.thread.status?.activeFlags?.includes("waitingOnUserInput") ? "waiting" : "running"
      : turn.status === "interrupted" ? "cancelled" : turn.status === "failed" ? "error" : "completed";
    if (state === "cancelled") {
      onInterrupted();
      adapter.turnWaiter = null; adapter.turnId = null;
      clearTimeout(waiter.timer);
      waiter.reject(Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" }));
    } else if (state === "completed" || state === "error") {
      const reply = state === "completed" ? (Array.isArray(turn.items) ? turn.items : []).reduce(selectFinalReply, waiter.finalReply) : null;
      adapter.handleLine(JSON.stringify({ method: "turn/completed", params: { threadId,
        turn: { id: turnId, status: turn.status, items: reply ? [{ id: reply.itemId, type: "agentMessage", phase: reply.phase,
          status: "completed", text: reply.text, displayTruncated: reply.truncated }] : [],
        error: turn.error ? { message: String(turn.error.message ?? "回合失败").slice(0, 2048),
          codexErrorInfo: turn.error.codexErrorInfo } : null } } }));
      if (adapter.protocolFailure) throw new Error(adapter.lastError ?? "模型工具协议校验失败");
    }
    adapter.lastError = null;
    return { turnId, state };
  } finally { if (pending.get(adapter) === owner) pending.delete(adapter); }
}

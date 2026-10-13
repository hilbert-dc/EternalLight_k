// @ts-check
import { assertNovelTurnBudget } from "./codex-runtime.mjs";
const admittedFailures = new WeakSet();

// Only the adapter's matched terminal event can admit this error. Arbitrary
// exceptions, provider HTTP bodies and transient events cannot request replay.
export function admitBoundedOutputFailure(error, turn) {
  // The current App Server projects Core InvalidPrompt as CodexErrorInfo.other.
  if (turn?.status !== "failed" || turn.error?.codexErrorInfo !== "other"
    || typeof error?.message !== "string" || error.message.length > 1024) return;
  const prefix = "provider_output_item_limit; outputItem=";
  if (!error.message.startsWith(prefix)) return;
  let diagnostic;
  try { diagnostic = JSON.parse(error.message.slice(prefix.length)); } catch { return; }
  if (!diagnostic || !["function_call", "message", "reasoning"].includes(diagnostic.type)
    || !Number.isSafeInteger(diagnostic.utf8Bytes) || diagnostic.utf8Bytes < 0) return;
  if (!(diagnostic.limitKind === "conservativeTokens" && diagnostic.limit === 10000
      && Number.isSafeInteger(diagnostic.conservativeTokens) && diagnostic.conservativeTokens > 10000)
    && !(diagnostic.limitKind === "utf8Bytes" && diagnostic.limit === 262144
      && diagnostic.utf8Bytes > 262144 && diagnostic.conservativeTokens === null)) return;
  admittedFailures.add(error);
}

export function isBoundedOutputFailure(error) {
  return error !== null && typeof error === "object" && admittedFailures.has(error);
}

// One sender is shared by all protocol turns inside an existing Engine lease.
// The model, never the host, reconstructs and resubmits an admissible result.
export function createBoundedOutputSender(adapter, task, runtimeRequest, assertCurrent) {
  let corrections = 0;
  return async (input, options) => {
    for (;;) {
      assertCurrent();
      try { return await adapter.sendTurn(input, options); }
      catch (error) {
        if (!isBoundedOutputFailure(error) || corrections >= 2) throw error;
        assertCurrent();
        const current = await runtimeRequest(`/api/projects/${encodeURIComponent(task.projectId)}/tasks/${encodeURIComponent(task.id)}`);
        assertCurrent();
        if (current.id !== task.id || current.projectId !== task.projectId || current.threadId !== task.threadId
          || !["queued", "running", "waiting"].includes(current.status) || adapter.threadId !== task.threadId) throw error;
        corrections++;
        const correction = "上一回合有单个模型输出超过原有10K保守token或256KiB上限，桥接拒绝了该输出，不能把它当作已执行的工具调用。请在同一任务、线程、权限和剩余额度内纠正，先核对已有保存回执，避免重做已保存成果。正文保存只能提交body或完整sceneGroups之一，绝不能重复全文；优先完整sceneGroups并省略body，篇幅遵守作者要求。不要把全文复制进总结、自检或其他工具字段。保留冻结目标、CAS、场景身份与保护范围，不自动截断或编造保存结果。若仍无法在上限内表达，停止并如实说明。";
        const turn = assertNovelTurnBudget({ input: [{ type: "text", text: "应用内部输出纠正触发；保持原作者要求。" }],
          additionalContext: { ...options?.additionalContext, novelOutputCorrection: { kind: "application", value: correction } } });
        input = turn.input;
        options = { disableEnvironment: true, additionalContext: turn.additionalContext };
      }
    }
  };
}

import type { AgentConnectionState } from "./agent-events";
import { describeAgentError, isModelConnectionUnavailable } from "./agent-error-description";

export type ExecutionHealth = { projectId: string | null; workTaskId: string | null; eventId: string | null; phase: "scan" | "resume"; state: "waiting"; failures: number; code: string | null; observedAt: number; nextCheckAt: number };

export function AgentStatusBar({ projectId, status, connectionState, operationError, threadId, onReconnect }: {
  projectId?: string;
  status: { executionHealth?: ExecutionHealth[]; configured: boolean; state: string; error: string | null; provider?: string; model?: string };
  connectionState: AgentConnectionState;
  operationError: string | null;
  threadId: string | null;
  onReconnect: () => void;
}) {
  const qualificationUnavailable = Boolean(status.error && isModelConnectionUnavailable(status.error));
  const online = !qualificationUnavailable && status.configured && connectionState === "ready";
  const error = operationError ?? status.error;
  const providerLabel = status.provider && status.model ? `${status.provider} · ${status.model}` : "Novel Agent Engine";
  const connectionLabels: Record<AgentConnectionState, string> = {
    offline: "创作引擎未连接，编辑和本地命令仍可用",
    connecting: "正在连接创作引擎…",
    reconnecting: "任务连接已断开，等待恢复…",
    error: "创作引擎连接失败，编辑和本地命令仍可用",
    ready: !status.configured ? "请配置创作引擎" : error ? "本轮任务未完成；创作引擎已连接。"
      : status.state === "running" ? "正在生成事件…" : status.state === "waiting" ? "等待作者回答…" : "创作引擎已就绪",
  };
  return <div className={`agent-status-bar ${online ? "agent-ready" : "agent-local"}`}>
    <span className={`online-dot ${online ? "" : "offline"}`} />
    <strong>{online ? providerLabel : "本地 Runtime"}</strong>
    <span role="status">{qualificationUnavailable ? "模型需要重新验证，编辑内容仍可保存" : connectionLabels[connectionState]}</span>
    {status.executionHealth?.filter(item => !item.projectId || item.projectId === projectId).map(item => <div role="status" key={item.eventId ?? "scan"}>
      {item.phase === "scan" ? "暂时无法核对后台任务状态。" : "原任务的自动接续暂未完成。"}已保存内容仍保留，应用将在 {new Date(item.nextCheckAt).toLocaleTimeString()} 再次核对，无需重复输入。
    </div>)}
    {error && <div role="alert">{describeAgentError(error)}<details><summary>查看原始错误</summary><pre>{error}</pre></details></div>}
    {threadId && (connectionState === "reconnecting" || connectionState === "error") && <button className="status-reconnect" onClick={onReconnect}>恢复连接</button>}
  </div>;
}

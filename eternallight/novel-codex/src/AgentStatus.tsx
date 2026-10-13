import { continuationWaiting } from "../shared/work-recovery-presentation.mjs";
import type { AgentConnectionState } from "./agent-events";
import { describeAgentError, isModelConnectionUnavailable } from "./agent-error-description";

export type ExecutionHealth = { projectId: string | null; workTaskId: string | null; eventId: string | null; phase: "scan" | "resume"; state: "waiting"; failures: number; reason: string | null; category: string; workVersion: number | null; code: string | null; observedAt: number; nextCheckAt: number };

export function executionHealthGroups(health: ExecutionHealth[], projectId?: string) {
  const groups = new Map<string, { item: ExecutionHealth; count: number }>();
  for (const item of health.filter(value => !value.projectId || value.projectId === projectId)) {
    const key = JSON.stringify([item.projectId, item.phase, item.reason, item.code]);
    const group = groups.get(key);
    if (group) {
      group.count += 1;
      if (item.nextCheckAt < group.item.nextCheckAt) group.item = item;
    } else groups.set(key, { item, count: 1 });
  }
  return [...groups.entries()].map(([key, value]) => ({ key, ...value }));
}

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
      : status.state === "running" ? "创作任务正在执行" : status.state === "waiting" ? "等待作者回答…" : "创作引擎已就绪",
  };
  return <div className={`agent-status-bar ${online ? "agent-ready" : "agent-local"}`}>
    <span className={`online-dot ${online ? "" : "offline"}`} />
    <strong>{online ? providerLabel : "本地 Runtime"}</strong>
    <span role="status">{qualificationUnavailable ? "模型需要重新验证，编辑内容仍可保存" : connectionLabels[connectionState]}</span>
    {executionHealthGroups(status.executionHealth ?? [], projectId).map(({ key, item, count }) => <div role="status" key={key}>
      {item.reason ? continuationWaiting(item.reason).label : item.phase === "scan" ? "暂时无法核对后台任务状态。" : "后台接续检查遇到异常，已保存内容仍保留。"}
      {count > 1 && <span>（{count} 项接续记录）</span>}
      {item.category !== "author" && <>系统会在条件允许时自动核对，无需重复输入。</>}
    </div>)}
    {error && <div role="alert">{describeAgentError(error)}<details><summary>查看原始错误</summary><pre>{error}</pre></details></div>}
    {threadId && (connectionState === "reconnecting" || connectionState === "error") && <button className="status-reconnect" onClick={onReconnect}>恢复连接</button>}
  </div>;
}

import { useEffect, useState } from "react";
import type { AgentRun, AgentConnectionState } from "./agent-events";
import type { WorkTaskSummary } from "./types";
import type { ExecutionHealth } from "./AgentStatus";
import { workStepName } from "./work-step-presentation";
import { conversationActivity } from "./work-conversation";
import { workStopDescription } from "../shared/work-recovery-presentation.mjs";

type Props = { run: AgentRun; busy: boolean; connection: AgentConnectionState; works: WorkTaskSummary[]; health: ExecutionHealth[]; readError?: string | null };
export function executionPresentation({ run, busy, connection, works, health, readError }: Props): { label: string; active: boolean; startedAt?: number | null } | null {
  if (readError) return { label: "暂时无法核对任务进度，已保存内容仍保留", active: false };
  if (health.length) return { label: "自动接续暂未完成，正在等待重新核对", active: false };
  if (busy && (connection === "reconnecting" || connection === "error" || connection === "offline")) return { label: "执行连接中断，等待恢复", active: false };
  if (busy && connection === "connecting") return { label: "正在连接创作引擎", active: false };
  if (run.state === "waiting") return { label: "等待你的回答或授权", active: false };
  if (works.some(work => work.status === "running" && work.pauseAfterStep)) return { label: "已安排暂停，等待当前步骤结束；不会启动后续步骤", active: false };
  const live = works.flatMap(work => work.status === "running" ? (work.steps ?? []).filter(step => step.status === "running").map(step => ({ work, step })) : []);
  if (live.length === 1) {
    const { work, step } = live[0];
    const attempt = step.attempts?.find(item => item.id === step.currentAttemptId);
    return { label: `${workStepName(work, step).label} · 执行中`, active: true,
      startedAt: attempt?.startedAt ? attempt.startedAt * 1000 : null };
  }
  if (busy && run.state === "running") return { label: conversationActivity(run).title, active: true, startedAt: run.startedAt };
  if (busy) return { label: "正在接续任务", active: true };
  const stopped = works.find(work => work.status === "blocked" || work.status === "paused");
  if (stopped) return { label: stopped.status === "paused" ? "任务已暂停" : workStopDescription(stopped.resumeBlockReason), active: false };
  if (live.length > 1) return { label: `${live.length}项创作步骤正在执行`, active: true };
  if (works.some(work => work.status === "running")) return { label: "任务尚未结束，等待执行状态更新", active: false };
  if (run.state === "error") return { label: "本轮未完成，请查看失败原因", active: false };
  if (run.state === "cancelled") return { label: "本轮已停止", active: false };
  if (run.state === "completed") return { label: "本轮已结束", active: false };
  return null;
}

export function ConversationExecutionStatus(props: Props) {
  const state = executionPresentation(props);
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    if (!state?.active) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [state?.active, state?.startedAt]);
  if (!state) return null;
  const seconds = state.active && state.startedAt
    ? Math.max(0, Math.floor((now - state.startedAt) / 1000)) : null;
  return <div className={`conversation-live-status${state.active ? " is-active" : ""}`}>
    <span className="conversation-live-dot" aria-hidden="true" />
    <span role="status" aria-live="polite">{state.label}</span>
    {seconds !== null && <span className="conversation-live-time" aria-label="当前步骤耗时">{seconds < 60 ? `${seconds}秒` : `${Math.floor(seconds / 60)}分 ${seconds % 60}秒`}</span>}
  </div>;
}

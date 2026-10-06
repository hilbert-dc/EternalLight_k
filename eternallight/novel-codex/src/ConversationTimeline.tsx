import { useEffect, useState } from "react";
import type { AgentStep } from "./agent-events";
import type { DisplayExecution, TimelineRun } from "./conversation-process";
import { conversationActivity, conversationWorkSteps } from "./work-conversation";
import "./conversation-presentation.css";
import { CollaborationProgress } from "./CollaborationProgress";

function WorkItem({ step }: { step: AgentStep }) {
  const content = step.text || step.detail;
  const status = step.status === "cancelled" ? "已停止"
    : step.status === "running" ? "进行中" : step.status === "waiting" ? "等待中"
      : step.status === "failed" ? "失败" : null;
  const heading = <><span className={`conversation-work-marker ${step.kind}`} aria-hidden="true">{step.kind === "tool" ? ">" : "·"}</span>
    <span className="conversation-work-copy"><strong>{step.title}</strong>{status && <small>{status}</small>}</span></>;
  if (!content || content === step.title) return <div className={`conversation-work-item ${step.kind} ${step.status}`}>{heading}</div>;
  return <details className={`conversation-work-item ${step.kind} ${step.status}`}><summary>{heading}</summary>
    <div className="conversation-work-detail">{step.text && <p>{step.text}</p>}{step.detail && <pre>{step.detail}</pre>}</div></details>;
}

export function ConversationTimeline({ run, retained }: { run: TimelineRun; retained?: DisplayExecution }) {
  const steps = conversationWorkSteps(run);
  const [expanded, setExpanded] = useState(false);
  const [, setClock] = useState(0);
  useEffect(() => {
    if (run.state !== "running" || !expanded) return;
    const timer = window.setInterval(() => setClock(value => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [run.state, expanded]);
  if (!steps.length && !retained?.totalPublicSteps && !["running", "waiting", "error", "cancelled"].includes(run.state)) return null;
  const end = run.completedAt ?? (["running", "waiting"].includes(run.state) ? Date.now() : null);
  const seconds = run.startedAt && end !== null ? Math.max(0, Math.round((end - run.startedAt) / 1000)) : null;
  const elapsed = seconds === null ? null : seconds < 60 ? `${seconds}秒` : `${Math.floor(seconds / 60)}分钟 ${seconds % 60}秒`;
  const activity = conversationActivity(run);
  const headline = run.state === "running" ? activity.title : run.state === "waiting" ? "等待你的回答"
    : run.state === "error" ? "本轮未完成" : run.state === "cancelled" ? "已停止" : "本轮执行过程";
  const scope = `${run.threadId ?? ""}:${run.turnId ?? run.startedAt ?? ""}`;
  return <><CollaborationProgress run={run} /><details className={`conversation-timeline is-${run.state}`} data-conversation-detail={`turn:${scope}`} open={expanded}
    onToggle={event => setExpanded(event.currentTarget.open)}>
    <summary><span className="conversation-process-headline"><i className={`conversation-state-marker is-${run.state}`} aria-hidden="true" /><span className="conversation-activity-label">{headline}</span></span><small>{expanded ? "收起过程" : "查看过程"}</small></summary>
    <p className="conversation-process-meta">{steps.length ? `${steps.length} 项执行记录` : "等待执行反馈"}{elapsed && ` · 用时 ${elapsed}`}</p>
    {retained && (retained.totalPublicSteps > steps.length || retained.detailsTruncated) && <p className="conversation-work-detail">部分执行记录已省略{retained.totalPublicSteps > steps.length ? `，保留 ${steps.length}/${retained.totalPublicSteps} 步` : ""}。</p>}
    {steps.length > 0 && <div className="transcript-steps" aria-label="执行步骤">{steps.map(step =>
      <WorkItem key={step.id} step={step} />)}</div>}
  </details></>;
}

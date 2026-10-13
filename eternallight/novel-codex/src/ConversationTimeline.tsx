import { useEffect, useState } from "react";
import type { AgentStep } from "./agent-events";
import type { DisplayExecution, TimelineRun } from "./conversation-process";
import { conversationActivity, conversationWorkSteps } from "./work-conversation";
import "./conversation-presentation.css";
import { MarkdownContent } from "./MarkdownContent";
import { CollaborationProgress } from "./CollaborationProgress";
import { meaningfulProgress, operationIcon, operationSummary, readingRecovery, type ConversationOperationIcon } from "./conversation-value";
import { describeAgentError } from "./agent-error-description";

function ProcessIcon({ kind }: { kind: ConversationOperationIcon | "context" | "chevron" }) {
  const paths = {
    read: <><path d="M3 3.5h5l2 1.5 2-1.5h5v13h-5l-2 1.5-2-1.5H3z" /><path d="M10 5v13" /></>,
    search: <><circle cx="8.5" cy="8.5" r="5.5" /><path d="m12.5 12.5 4.5 4.5" /></>,
    write: <><path d="m12 3 5 5-9 9H3v-5z" /><path d="m10 5 5 5" /></>,
    analysis: <><path d="M6 3H4v14h12V3h-2M8 3h4v3H8z" /><path d="m7 11 2 2 4-4" /></>,
    arrange: <><circle cx="10" cy="4" r="2" /><circle cx="4" cy="16" r="2" /><circle cx="16" cy="16" r="2" /><path d="M10 6v4H4v4m6-4h6v4" /></>,
    operation: <><rect x="3" y="3" width="14" height="14" rx="3" /><path d="m6 7 3 3-3 3m6 0h2" /></>,
    context: <><path d="M6 3H4v14h3m7-14h2v14h-3M8 7h4M7 10h6m-5 3h4" /></>,
    chevron: <path d="m7 4 6 6-6 6" />,
  };
  return <svg className={`conversation-process-icon is-${kind}`} viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[kind]}</svg>;
}

function ContextNotice({ step }: { step: AgentStep }) {
  const text = step.status === "completed" ? "对话上下文已自动整理"
    : step.status === "failed" ? "对话整理未完成" : step.status === "cancelled" ? "对话整理已停止" : "正在整理对话上下文";
  return <div className={`conversation-context-notice is-${step.status}`}><ProcessIcon kind="context" /><span>{text}</span></div>;
}

function WorkItem({ step, recovered = false }: { step: AgentStep; recovered?: boolean }) {
  const description = step.text || (step.id.startsWith("user-input:") || step.id.startsWith("steer:") || step.kind === "approval" ? step.detail : undefined);
  const errorDetail = step.status === "failed" ? step.detail : undefined;
  const status = step.status === "cancelled" ? "已停止"
    : step.status === "running" ? "进行中" : step.status === "waiting" ? "等待中"
      : step.status === "failed" ? "未完成" : recovered ? "已重试恢复"
        : step.operation?.category === "write" && step.status === "completed"
          && !["saved", "isolatedCandidate"].includes(step.operation.outcome ?? "") ? "保存结果待核实" : null;
  const heading = <><span className={`conversation-work-marker ${step.kind}`} aria-hidden="true">{step.status === "failed" ? "!" : step.status === "completed" ? "✓" : "·"}</span>
    <span className="conversation-work-copy"><strong>{step.title}</strong>{status && <small>{status}</small>}</span></>;
  return <div className={`conversation-work-item ${step.kind} ${step.status}`}>
    <div className="conversation-work-heading">{heading}</div>
    {description && description !== step.title && <p className="conversation-work-description">{description}</p>}
    {step.status === "failed" && step.kind !== "error" && <p className="conversation-work-description">这一步未完成，请查看任务状态及后续安排。</p>}
    {errorDetail && <details className="conversation-error-detail"><summary>查看错误详情</summary><pre>{errorDetail}</pre></details>}
  </div>;
}

function OperationItems({ steps, allSteps }: { steps: AgentStep[]; allSteps: AgentStep[] }) {
  const groups: AgentStep[][] = [];
  for (const step of steps) {
    const previous = groups.at(-1);
    if (step.operation?.category === "read" && step.status === "completed"
      && previous?.[0].operation?.category === "read" && previous[0].status === "completed"
      && previous[0].operation.sourceId === step.operation.sourceId && previous[0].title === step.title) previous.push(step);
    else groups.push([step]);
  }
  return <>{groups.map(group => <WorkItem key={group[0].id} step={group.at(-1)!}
    recovered={group.some(step => readingRecovery(allSteps, step) === "recovery")} />)}</>;
}

export function ConversationTimeline({ run, retained, showFailure = true }: { run: TimelineRun; retained?: DisplayExecution; showFailure?: boolean }) {
  // Child status and reports already have one identity-bound collaboration row.
  // Keep failed or unidentified routing visible instead of hiding an obstacle.
  const allSteps = conversationWorkSteps(run).filter(step => !step.collaborators?.length || step.status !== "completed");
  const steps = meaningfulProgress(allSteps).filter(step => readingRecovery(allSteps, step) !== "recovered");
  // Group only adjacent operations: a public progress message is a chronology boundary.
  const blocks: { id: string; progress?: AgentStep; notice?: AgentStep; failure?: AgentStep; operations: AgentStep[] }[] = [];
  for (const step of steps) {
    if (step.sourceType === "contextCompaction") {
      blocks.push({ id: step.id, notice: step, operations: [] });
    } else if (step.messagePhase === "commentary") {
      if (step.text?.trim()) blocks.push({ id: step.id, progress: step, operations: [] });
    } else if (step.kind === "error" && step.status === "failed") {
      if (showFailure) blocks.push({ id: step.id, failure: step, operations: [] });
    } else {
      const previous = blocks.at(-1);
      if (previous && !previous.progress && !previous.notice && !previous.failure) previous.operations.push(step);
      else blocks.push({ id: step.id, operations: [step] });
    }
  }
  const [, setClock] = useState(0);
  useEffect(() => {
    if (run.state !== "running") return;
    const timer = window.setInterval(() => setClock(value => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [run.state]);
  const truncated = retained && (retained.totalPublicSteps > retained.steps.length || retained.detailsTruncated);
  if (!steps.length && !truncated && !["running", "waiting", "error", "cancelled"].includes(run.state)) return <CollaborationProgress run={run} />;
  const end = run.completedAt ?? (["running", "waiting"].includes(run.state) ? Date.now() : null);
  const seconds = run.startedAt && end !== null ? Math.max(0, Math.round((end - run.startedAt) / 1000)) : null;
  const elapsed = seconds === null ? null : seconds < 60 ? `${seconds}秒` : `${Math.floor(seconds / 60)}分钟 ${seconds % 60}秒`;
  const activity = conversationActivity(run);
  const headline = run.state === "running" ? activity.title : run.state === "waiting" ? "等待你的回答"
    : run.state === "error" ? "本轮未完成" : run.state === "cancelled" ? "已停止" : "已处理";
  const scope = `${run.threadId ?? ""}:${run.turnId ?? run.startedAt ?? ""}`;
  return <><CollaborationProgress run={run} /><section className={`conversation-timeline is-${run.state}`} data-conversation-detail={`turn:${scope}`} aria-label="创作过程">
    <div className="conversation-process-header"><span className="conversation-process-headline"><i className={`conversation-state-marker is-${run.state}`} aria-hidden="true" /><span className="conversation-activity-label">{`${headline}${elapsed ? ` · ${elapsed}` : ""}`}</span></span></div>
    <div className="conversation-process-flow" aria-label="创作进展">
      {blocks.map(block => block.progress
        ? <div key={block.id} className="conversation-progress-notes"><MarkdownContent text={block.progress.text!} /></div>
        : block.notice ? <ContextNotice key={block.id} step={block.notice} />
        : block.failure ? <div key={block.id} className="transcript-error" role="alert">
          <strong>本轮未完成</strong><span>{describeAgentError(block.failure.detail ?? "")}</span>
          {block.failure.detail && <details><summary>查看错误详情</summary><pre>{block.failure.detail}</pre></details>}
        </div>
        : <details key={block.id} className={`conversation-operation-details${block.operations.some(step => step.status === "failed") ? " has-failure" : ""}`} data-conversation-detail={`turn:${scope}:operations:${block.id}`}>
          <summary><ProcessIcon kind={operationIcon(block.operations)} /><span>{operationSummary(block.operations)}</span><span className="conversation-operation-chevron"><ProcessIcon kind="chevron" /></span></summary>
          <div className="transcript-steps" aria-label="操作记录"><OperationItems steps={block.operations} allSteps={allSteps} /></div>
        </details>)}
    </div>
    {truncated && <p className="conversation-process-meta">部分较早的过程记录未保留。</p>}

  </section></>;
}

import { useEffect, useRef } from "react";
import type { NovelCodexApi, RevisionPredecessor, RevisionRepairPreparation, WorkTaskSummary } from "./types";
import { ReadingProgress } from "./ReadingProgress";
import { ActionPermissionDetails } from "./ActionPermissionDetails";
import { TaskFinalReplyDetails } from "./TaskFinalReplyDetails";
import { RevisionOutcomeDetails } from "./RevisionOutcomeDetails";

const statusLabels: Record<string, string> = {
  running: "进行中", paused: "已暂停", blocked: "需处理", completed: "已完成",
  cancelled: "已取消", queued: "待执行",
};
const operationLabels: Record<string, string> = {
  advisory: "讨论与规划", generateBrief: "生成简报", generateOutline: "生成大纲",
  generatePropertyRules: "生成属性规则提案",
  generatePlan: "生成近章规划", generateChapter: "生成章节", modelKnowledge: "分析章节知识",
  assessRevisionOutcome: "评估修改结果",
};
const deliveryLabels: Record<WorkTaskSummary["deliveryStatus"], string> = {
  pending: "交付待核验", partial: "已保存部分产物",
  ready_for_review: "候选产物待审阅", discussion_ended: "讨论已结束",
};

const resumeBlockLabels: Record<NonNullable<WorkTaskSummary["resumeBlockReason"]>, string> = {
  pendingRecovery: "有尚未核对的写入结果，请先完成执行记录核对，避免重复写入。",
  partialOutput: "已有未确认的部分写入，请先核对产物后重新发起任务。",
  inputChanged: "作品版本已变化，请根据当前内容重新发起任务。",
  attemptBudget: "本次自动分析已用完三次尝试，请检查模型连接，取消此任务并调整正文后重新分析。",
};

interface Props {
  api: NovelCodexApi | undefined;
  projectId: string;
  tasks: WorkTaskSummary[];
  activeCount: number;
  nextCursor: string | null;
  loadingMore: boolean;
  loaded: boolean;
  reading: boolean;
  readError: string | null;
  onRetry: () => void;
  revision: number | undefined;
  busy: boolean;
  interruptionBusy: boolean;
  agentRunning: boolean;
  agentAvailable: boolean;
  dirty: boolean;
  onControl: (id: string, command: "pause" | "cancel" | "resume") => void;
  onUndo: (commitId: string) => void;
  onLoadMore: () => void;
  onPrepareRevisionContinuation?: (predecessor: RevisionPredecessor, intent: string) => void;
  onPrepareRevisionRepair?: (preparation: RevisionRepairPreparation) => void;
}

export function WorkTaskPanel({ api, projectId, tasks, activeCount, nextCursor, loadingMore, loaded, reading, readError, onRetry, revision, busy, interruptionBusy, agentRunning, agentAvailable, dirty, onControl, onUndo, onLoadMore, onPrepareRevisionContinuation, onPrepareRevisionRepair }: Props) {
  const panelRef = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    if ((activeCount > 0 || readError) && panelRef.current) panelRef.current.open = true;
  }, [activeCount, readError]);
  if (loaded && tasks.length === 0 && !readError) return null;
  const controlsBusy = busy || !!readError || !loaded;
  const interruptionDisabled = interruptionBusy || !!readError || !loaded || dirty;
  return <details ref={panelRef} className="work-task-panel">
    <summary>创作任务 · {readError ? "读取失败" : !loaded ? "读取中" : `${activeCount} 项执行未结束`}</summary>
    {readError && <div role="alert"><p>{readError}。{loaded ? "保留上次成功读取的任务，状态可能已变化。" : "尚未读取到任务，不能判断是否有待处理任务。"}</p>
      <button type="button" className="secondary-button" disabled={reading} onClick={onRetry}>重试读取任务</button></div>}
    <div className="work-task-list">{tasks.map((task) => {
      const resumeBlocked = task.resumeBlockReason ? resumeBlockLabels[task.resumeBlockReason] : null;
      return <article key={task.id} className="work-task-row">
        <header><strong>{task.goal.slice(0, 90) || "创作任务"}</strong><span>{task.status === "completed" ? "执行结束" : statusLabels[task.status] ?? task.status}
          {task.status === "completed" || task.deliveryStatus === "partial" ? ` · ${deliveryLabels[task.deliveryStatus]}` : ""}</span></header>
        {task.deliveryReport.pendingActionIds.length > 0 && <p>有 {task.deliveryReport.pendingActionIds.length} 项行动提案待确认；尚未执行。</p>}
        {task.deliveryReport.unsettledAttemptIds.length > 0 && <p>执行用量尚未结算，交付判定待更新。</p>}
        <ol>{task.steps.map((step) => <li key={step.id}>
          <span>{operationLabels[step.operation] ?? step.operation} · {statusLabels[step.status] ?? step.status}</span>
          {task.deliveryReport.evidence.filter((item) => item.stepId === step.id && item.kind !== "completionReport").map((item) =>
            item.state === "recorded" ? <small key={item.stepId}>{item.kind === "knowledgeBatch" ? "已发布知识批次" : "候选产物证据已核验"}</small> :
              item.state === "superseded" ? <small key={item.stepId}>原产物已被后续版本替换</small> :
              item.state === "missing" ? <small key={item.stepId}>未找到符合任务契约的产物证据</small> : null)}
          {step.attempts.length > 1 && <small>尝试 {step.attempts.length} 次</small>}
          {step.attempts.at(-1)?.error && <small>{step.attempts.at(-1)?.error}</small>}
          {step.attempts.filter((attempt) => ["completed", "failed", "cancelled"].includes(attempt.status)).map((attempt) =>
            <TaskFinalReplyDetails key={attempt.id} api={api} projectId={projectId} taskId={attempt.id} />)}
          {step.operation === "generateChapter" && step.target.desiredOutcomes?.length && typeof step.target.chapterId === "string"
            ? step.attempts.filter(attempt => ["completed", "failed", "cancelled"].includes(attempt.status)).map(attempt =>
              <RevisionOutcomeDetails key={attempt.id} api={api} projectId={projectId} writerTaskId={attempt.id}
                chapterId={step.target.chapterId as string} desiredOutcomes={step.target.desiredOutcomes!}
                version={`${task.version}:${revision ?? ""}`} disabled={controlsBusy || agentRunning || dirty}
                onPrepareRepair={onPrepareRevisionRepair}
                onPrepareContinuation={onPrepareRevisionContinuation ? predecessor => onPrepareRevisionContinuation(predecessor, task.goal) : undefined} />) : null}
          <ReadingProgress progress={step.readingProgress} />
          {step.sourceTaskId && <ActionPermissionDetails api={api} projectId={projectId} workTaskId={task.id}
            sourceTaskId={step.sourceTaskId} actionId={step.id} version={task.version} />}
          {step.changes.filter((change) => change.operation === "accept").map((change) =>
            <span key={change.commitId} className="work-task-change">已采用 v{change.resultRevision}
              {change.undoneByCommitId ? " · 已撤销" : <button type="button" className="secondary-button"
                disabled={controlsBusy || agentRunning || dirty || revision !== change.resultRevision}
                title={revision !== change.resultRevision ? "作品已有后续修改，请先检查依赖" : "撤销这次采用"}
                onClick={() => onUndo(change.commitId)}>撤销</button>}
            </span>)}
        </li>)}</ol>
        {task.status === "completed" && <small>执行结束不代表创作目标已达成，请核对结果。</small>}
        <div className="work-task-controls">
          {task.status === "running" && <button type="button" className="secondary-button" disabled={interruptionDisabled} onClick={() => onControl(task.id, "pause")}>暂停后续步骤</button>}
          {["paused", "blocked"].includes(task.status) && <button type="button" className="secondary-button" disabled={controlsBusy || agentRunning || dirty || !agentAvailable || !!resumeBlocked} title={resumeBlocked ?? undefined} onClick={() => onControl(task.id, "resume")}>继续任务</button>}
          {["running", "paused", "blocked"].includes(task.status) && <button type="button" className="secondary-button" disabled={interruptionDisabled} onClick={() => onControl(task.id, "cancel")}>取消任务</button>}
          {resumeBlocked && ["paused", "blocked"].includes(task.status) && <small>{resumeBlocked}</small>}
        </div>
      </article>; })}
      {nextCursor && <button type="button" className="secondary-button" disabled={reading || !!readError} onClick={onLoadMore}>{loadingMore ? "加载中…" : "加载更多任务"}</button>}
    </div>
  </details>;
}

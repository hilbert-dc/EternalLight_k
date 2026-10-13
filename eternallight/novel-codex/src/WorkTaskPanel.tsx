import { ReadFailureNotice } from "./ReadFailureNotice";
import { workStopLabels } from "../shared/work-recovery-presentation.mjs";
import { type TaskResultDiscussion } from "./task-result-excerpt";
import { ConversationPrompt } from "./ConversationPrompt";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useConversationReading } from "./useConversationReading";
import "./conversation-presentation.css";
import type { CommitmentDetail, NovelCodexApi, RevisionPredecessor, RevisionRepairPreparation, WorkTaskSummary } from "./types";
import { CommitmentTaskResult } from "./CommitmentTaskResult";
import { RevisionOutcomeDetails } from "./RevisionOutcomeDetails";
import { FailedTaskReply, SavedTaskReply } from "./SavedTaskReply";
import { isUnfinishedConversationTurn, type FinalReplyDisplayState } from "./ConversationOutput";
import { describeAgentError } from "./agent-error-description";
import { attemptContainsRun, currentWorkError, completedDeliveryPresentation, conversationItems, generatedReplyDescription, hasOriginalAuthorMessage, primaryReply, sameConversationFailure, type WorkConversationContent } from "./work-conversation";
import type { AgentRunSummary } from "./agent-events";
import "./WorkTaskPanel.css";
import { WorkChapterAcceptance } from "./WorkChapterAcceptance";
import { CreationIssueReference, WorkIssueScope } from "./CreationIssues";
import { getCommitmentDecisionResults, getTaskResults, taskResultLabel, taskResultSummary, type CommitmentDecisionResult, type TaskResultDestination } from "./task-results";
import "./task-results.css";

const statusLabels: Record<string, string> = {
  running: "进行中", paused: "已暂停", blocked: "需处理", completed: "已完成",
  cancelled: "已取消", queued: "待执行", superseded: "已改用后续步骤",
};
const deliveryLabels: Record<WorkTaskSummary["deliveryStatus"], string> = {
  pending: "保存结果待核对", partial: "已保存部分内容",
  ready_for_review: "结果已保存", discussion_ended: "讨论已结束",
};
const resumeBlockLabels = workStopLabels;

interface Props {
  conversation?: WorkConversationContent | null;
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
  agentRunning: boolean;
  dirty: boolean;
  onUndo?: (commitId: string) => void;
  onLoadMore: () => void;
  onPrepareRevisionContinuation?: (predecessor: RevisionPredecessor, intent: string) => void;
  onPrepareRevisionRepair?: (preparation: RevisionRepairPreparation) => void;
  onDiscussResult?: (request: TaskResultDiscussion) => void;
  onOpenResult?: (result: TaskResultDestination) => void;
  renderCommitmentResult?: (result: CommitmentDecisionResult) => ReactNode;
  onAdjustCommitment?: (result: CommitmentDecisionResult, commitment: CommitmentDetail) => void;
  onFormalChange?: () => Promise<void>;
  readOnly?: boolean;
}

export function WorkTaskPanel({ conversation = null, api, projectId, tasks, activeCount, nextCursor, loadingMore, loaded, reading, readError, onRetry, revision, busy, agentRunning, dirty, onUndo, onLoadMore, onPrepareRevisionContinuation, onPrepareRevisionRepair, onOpenResult, onDiscussResult, renderCommitmentResult, onAdjustCommitment, onFormalChange, readOnly = false }: Props) {
  const panelRef = useRef<HTMLElement>(null);
  const interaction = useRef<string | null>(null);
  const [activeInteraction, setActiveInteraction] = useState<string | null>(null);
  useEffect(() => { interaction.current = null; setActiveInteraction(null); }, [projectId, conversation?.threadId]);
  const scopedTasks = conversation ? tasks.filter(task => task.projectId === projectId && (
    task.threadId === (conversation.threadId ?? "")
    || task.steps.some(step => step.attempts.some(attempt => attemptContainsRun(attempt, conversation.run)
      || conversation.run.history.some(entry => attemptContainsRun(attempt, entry)))))) : tasks;
  const items = conversationItems(scopedTasks, conversation);
  const panelVisible = !loaded || items.length > 0 || Boolean(readError);
  const readingPosition = useConversationReading(api, JSON.stringify([projectId, conversation?.threadId ?? null]), panelRef, loaded && panelVisible);
  if (loaded && !items.length && !readError) return null;
  const controlsBusy = busy || !!readError || !loaded;
  const renderTask = (task: WorkTaskSummary, current: boolean, history: AgentRunSummary[]) => {
      const results = onOpenResult && task.projectId === projectId ? getTaskResults(task) : [];
      const decisions = task.projectId === projectId ? getCommitmentDecisionResults(task) : [];
      const displayResults = task.steps.flatMap(step => [...results.filter(result => result.stepId === step.id && result.kind !== "commitmentExtraction"),
        ...decisions.filter(result => result.stepId === step.id)]);
      const recentResults = displayResults.slice(-3);
      const earlierResults = displayResults.slice(0, -3);
      const savedChapters = new Set(results.filter(result => result.kind === "chapter").map(result => result.kind === "chapter" ? result.chapterId : "")).size;
      const reply = primaryReply(task);
      const presentation = task.projectId === projectId ? completedDeliveryPresentation(task) : null;
      const description = task.projectId === projectId ? generatedReplyDescription(task, results) : null;
      const deliveryLabel = task.deliveryReport.deliveryReview?.goalStatus === "partial" ? "已保存成果，目标尚未全部完成" : task.deliveryReport.deliveryReview?.goalStatus === "needsAuthorDecision" ? "已保存成果，需你决定后续安排" : presentation === "updated" ? "结果已保存，后续已有更新" : deliveryLabels[task.deliveryStatus];
      const renderHistory = (entry: AgentRunSummary) => {
        const render = (finalReplyState?: FinalReplyDisplayState) => conversation?.renderHistory(entry, { taskBound: true, finalReplyState });
        const matches = task.steps.flatMap(step => step.attempts.filter(attempt => attemptContainsRun(attempt, entry)));
        return task.projectId === projectId && isUnfinishedConversationTurn(entry.state)
          && entry.threadId && entry.turnId && matches.length === 1
          ? <FailedTaskReply key={`${matches[0].id}:${entry.turnId}`} api={api} projectId={projectId}
            attempt={{ ...matches[0], turnId: entry.turnId }} render={render} /> : render();
      };
      const renderCurrent = () => {
        const run = conversation?.run;
        const render = (finalReplyState?: FinalReplyDisplayState) => conversation?.renderCurrent({ taskBound: true, finalReplyState });
        const matches = run ? task.steps.flatMap(step => step.attempts.filter(attempt => attemptContainsRun(attempt, run))) : [];
        return task.projectId === projectId && run && isUnfinishedConversationTurn(run.state)
          && run.threadId && run.turnId && matches.length === 1
          ? <FailedTaskReply key={`${matches[0].id}:${run.turnId}`} api={api} projectId={projectId}
            attempt={{ ...matches[0], turnId: run.turnId }} render={render} /> : render();
      };
      const liveAtTail = current && ["running", "waiting"].includes(conversation?.run.state ?? "");
      const taskError = currentWorkError(task);
      const replyInCurrent = current && conversation && reply && attemptContainsRun(reply.attempt, conversation.run);
      const replyInHistory = reply && history.some(entry => attemptContainsRun(reply.attempt, entry)
        && (entry.answer.trim() || (entry.state === "cancelled" && entry.displayExecution)));
      const savedReply = reply && !replyInCurrent && !replyInHistory
        ? <SavedTaskReply key={reply.attempt.id} api={api} projectId={projectId} attempt={reply.attempt} foldDescription={Boolean(description)} /> : null;
      const duplicateError = current && sameConversationFailure(taskError ?? null, conversation?.run.error ?? null);
      const resumeBlocked = task.resumeBlockReason ? resumeBlockLabels[task.resumeBlockReason] : null;
      const automatic = task.steps[0]?.operation === "assessChapterStructure";
      const renderRevisionOutcome = (step: WorkTaskSummary["steps"][number], attempt: WorkTaskSummary["steps"][number]["attempts"][number]) => {
        if (step.operation !== "generateChapter" || !step.target.desiredOutcomes?.length || typeof step.target.chapterId !== "string") return null;
        if (!step.target.chapterAcceptance || !Array.isArray(step.target.chapterAcceptance.requirements)) return <p key={attempt.id} role="alert">修改结果的章节标准无法核对。<button type="button" className="text-button" onClick={onRetry}>重新读取任务</button></p>;
        return <RevisionOutcomeDetails key={attempt.id} api={api} projectId={projectId} writerTaskId={attempt.id}
            chapterId={step.target.chapterId} chapterAcceptance={step.target.chapterAcceptance} desiredOutcomes={step.target.desiredOutcomes}
            version={`${task.version}:${revision ?? ""}`} disabled={readOnly || controlsBusy || agentRunning || dirty}
            onPrepareRepair={onPrepareRevisionRepair}
            onPrepareContinuation={onPrepareRevisionContinuation ? predecessor => onPrepareRevisionContinuation(predecessor, task.goal) : undefined} />;
      };
      const renderResult = (result: TaskResultDestination | CommitmentDecisionResult) => {
        const key = `${result.stepId}:${result.attemptId}:${result.referenceId}`;
        if (result.kind === "commitmentExtraction" && "deliveryState" in result) {
          const owner = JSON.stringify([projectId, conversation?.threadId ?? null, task.threadId, task.id, key]);
          return <div key={owner} className="task-result-card commitment-result-card" data-commitment-result={result.referenceId}>
            {renderCommitmentResult ? renderCommitmentResult(result) : <CommitmentTaskResult api={api} result={result}
              refreshToken={`${task.version}:${revision ?? ""}`} disabled={readOnly || controlsBusy || agentRunning || ["running", "queued"].includes(task.status) || dirty || !!activeInteraction && activeInteraction !== owner}
              onFormalChange={onFormalChange} onAdjust={onAdjustCommitment} onDiscuss={onDiscussResult}
              acquireInteraction={() => { if (interaction.current !== null) return false; interaction.current = owner; setActiveInteraction(owner); return true; }}
              releaseInteraction={() => { if (interaction.current === owner) { interaction.current = null; setActiveInteraction(null); } }} />}
          </div>;
        }
        const step = task.steps.find(item => item.id === result.stepId);
        const attempt = step?.attempts.find(item => item.id === result.attemptId);
        return <div className="task-result-card" key={key}><strong>{taskResultSummary(result)}</strong>
          {onOpenResult && <button type="button" className="secondary-button" disabled={controlsBusy} onClick={() => onOpenResult(result)}>{taskResultLabel(result)}</button>}
          {step && attempt && renderRevisionOutcome(step, attempt)}
        </div>;
      };
      return <WorkIssueScope key={task.id} api={api} projectId={projectId} taskId={task.id} version={task.version}
        summary={task.deliveryReport.issues} onRetry={onRetry}>{issuePanel => <div className="work-task-row conversation-turn" data-work-task-id={task.id} data-conversation-row={`task:${task.id}`}>
        {!hasOriginalAuthorMessage(task, scopedTasks) && <article className={`conversation-message ${automatic ? "system-message" : "user-message"}`}>
          <div className="conversation-message-body"><div className="conversation-message-meta"><strong>{automatic ? "自动检查" : "你的创作指令"}</strong></div><ConversationPrompt text={task.goal} detailKey={`task:${task.id}:prompt`} /></div>
        </article>}
        <header className="work-task-status"><span>{task.status === "completed" ? deliveryLabel : task.resumeBlockReason === "unknownUsage" ? "用量待核对" : statusLabels[task.status] ?? "处理中"}
          {task.kind === "generation" && savedChapters > 0 ? ` · 本次已保存 ${savedChapters} 章`
            : task.status !== "completed" && task.deliveryStatus === "partial" ? ` · ${deliveryLabel}` : ""}</span></header>
        {task.resumeBlockReason === "unknownUsage" && <p className="conversation-process-meta" role="status">{resumeBlocked}</p>}
        {task.status === "running" && task.pauseAfterStep && <p role="status">已安排暂停，等待当前步骤结束；不会启动后续步骤。仍可立即暂停。</p>}
        {task.controlDelivery?.state === "deliveryUnknown" && <p className="conversation-process-meta" role="status">任务控制待核对：原命令已保留，尚未确认是否生效。<button type="button" onClick={onRetry}>核对控制结果</button></p>}
        <WorkChapterAcceptance task={task} onRetry={onRetry} />
        {issuePanel}
        {task.deliveryReport.pendingActionIds.length > 0 && <p>{task.resumeBlockReason === "unknownUsage" ? "后续行动尚未执行。"
          : task.status === "running" ? "正在安排后续创作步骤…" : <>有 {task.deliveryReport.pendingActionIds.length} 项行动提案待确认；尚未执行。</>}</p>}
        {task.deliveryReport.unsettledAttemptIds.length > 0 && !["running", "queued"].includes(task.status)
          && <p className="conversation-process-meta">用量记录待核对。</p>}
        {history.map(entry => <div key={entry.id}>{renderHistory(entry)}</div>)}
        {current && !liveAtTail && renderCurrent()}
        {taskError && !duplicateError && <div role="alert"><p>{describeAgentError(taskError)}</p>{describeAgentError(taskError) !== taskError && <details><summary>查看错误详情</summary><pre>{taskError}</pre></details>}</div>}
        {task.steps.flatMap(step => step.attempts).filter(attempt => attempt.failureObservations?.length).map(attempt => <details key={`failures:${attempt.id}`}><summary>查看请求失败记录</summary>{attempt.failureObservations!.map((fact, index) => <p key={fact.requestId}>{index === 0 ? "首次观察到的失败" : "后续失败"}：{fact.errorStatusCode ? `模型请求未成功（HTTP ${fact.errorStatusCode}）` : "模型请求未完成"}<br /><small>{fact.errorCode ?? "原因待核对"} · {fact.requestId}</small></p>)}</details>)}
        {savedReply}
        {displayResults.length > 0 && <div className="task-result-actions" aria-label="本轮保存的结果">
          {recentResults.map(renderResult)}
          {earlierResults.length > 0 && <details className="task-earlier-results"><summary>更早的 {earlierResults.length} 项结果</summary>
            <div>{earlierResults.map(renderResult)}</div>
          </details>}
        </div>}
        {task.projectId === projectId && task.steps.flatMap(step => step.attempts
          .filter(attempt => ["completed", "failed", "cancelled"].includes(attempt.status)
            && !displayResults.some(result => result.stepId === step.id && result.attemptId === attempt.id))
          .map(attempt => renderRevisionOutcome(step, attempt)))}
        {task.deliveryReport.deliveryReview && task.deliveryReport.deliveryReview.goalStatus !== "fulfilled" && <section className="work-goal-assessment" aria-label="本轮目标核对">
          <p>{task.deliveryReport.deliveryReview.goalAssessment}</p>
          <p>检查范围：{task.deliveryReport.deliveryReview.checkedScope}</p>
          {task.deliveryReport.deliveryReview.unresolved.length > 0 && <><strong>尚未完成</strong><ul>{task.deliveryReport.deliveryReview.unresolved.map((item, index) => <li key={`${item.issue.issueId}:${index}`}>{item.reason} <CreationIssueReference issueId={item.issue.issueId} /></li>)}</ul></>}
          {task.deliveryReport.deliveryReview.authorDecisions.length > 0 && <><strong>需要你决定</strong><ul>{task.deliveryReport.deliveryReview.authorDecisions.map((item, index) => <li key={index}>{item}</li>)}</ul></>}
        </section>}
        {!reply && task.status === "completed" && <p className="conversation-answer-empty">本轮尚无回复。</p>}
        {task.steps.some(step => step.changes.some(change => change.operation === "accept")) && <section className="work-task-changes" aria-label="正文与设定变更">
          {task.steps.flatMap(step => step.changes.filter(change => change.operation === "accept")).map(change =>
            <span key={change.commitId} className="work-task-change">已采用 v{change.resultRevision}
              {change.undoneByCommitId ? " · 已撤销" : !readOnly && onUndo && <button type="button" className="secondary-button"
                disabled={controlsBusy || agentRunning || dirty || revision !== change.resultRevision}
                title={revision !== change.resultRevision ? "作品已有后续修改，请先检查依赖" : "撤销这次采用"}
                onClick={() => onUndo(change.commitId)}>撤销</button>}
            </span>)}
        </section>}
        {resumeBlocked && task.resumeBlockReason !== "unknownUsage" && ["paused", "blocked"].includes(task.status) && <p className="conversation-process-meta">{resumeBlocked}</p>}
        {liveAtTail && renderCurrent()}

      </div>}</WorkIssueScope>;
  };
  return <section ref={panelRef} className="work-task-panel conversation-panel" aria-label="创作对话">
    {!loaded && <p role="status">正在读取对话与执行记录…</p>}
    {activeCount > 1 && <p className="work-task-status" role="status">{activeCount} 项创作尚未结束</p>}
    {readError && <ReadFailureNotice message={`任务记录暂时无法读取。${loaded ? "保留上次成功读取的任务，状态可能已变化。" : "尚不能判断是否有待处理任务。"}`} error={readError} retryLabel="重试读取任务" busy={reading} onRetry={onRetry} />}
    {nextCursor && <button type="button" className="conversation-load-earlier" disabled={reading || !!readError} onClick={onLoadMore}>{loadingMore ? "加载中…" : "加载更早对话"}</button>}
    <div className="work-task-list">
      {items.map(item => item.kind === "task" ? renderTask(item.task, item.current, item.history)
        : item.kind === "history" ? <div key={`history:${item.entry.id}`} data-conversation-row={`history:${item.entry.id}`}>{conversation?.renderHistory(item.entry, { taskBound: false })}</div>
          : <div key="current" data-conversation-row={`turn:${conversation?.run.threadId}:${conversation?.run.turnId ?? conversation?.run.startedAt}`}>{conversation?.renderCurrent({ taskBound: false })}</div>)}
    </div>
    {readingPosition.reading && <button type="button" className="conversation-jump-latest" onClick={readingPosition.jumpToLatest}>回到最新</button>}
  </section>;
}

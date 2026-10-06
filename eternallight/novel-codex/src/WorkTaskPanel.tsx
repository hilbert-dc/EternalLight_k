import { ConversationPrompt } from "./ConversationPrompt";
import { useRef } from "react";
import { useConversationReading } from "./useConversationReading";
import "./conversation-presentation.css";
import type { NovelCodexApi, RevisionPredecessor, RevisionRepairPreparation, WorkTaskSummary } from "./types";
import { ReadingProgress } from "./ReadingProgress";
import { ActionPermissionDetails } from "./ActionPermissionDetails";
import { TaskFinalReplyDetails } from "./TaskFinalReplyDetails";
import { RevisionOutcomeDetails } from "./RevisionOutcomeDetails";
import { GeneratedReplyDescription, SavedTaskReply } from "./SavedTaskReply";
import { describeAgentError } from "./agent-error-description";
import { attemptContainsRun, intermediateReplyStep, completedDeliveryPresentation, conversationItems, generatedReplyDescription, hasOriginalAuthorMessage, primaryReply, sameConversationFailure, type WorkConversationContent } from "./work-conversation";
import type { AgentRunSummary } from "./agent-events";
import "./WorkTaskPanel.css";
import { getTaskResults, taskResultLabel, type TaskResultDestination } from "./task-results";
import "./task-results.css";

const statusLabels: Record<string, string> = {
  running: "进行中", paused: "已暂停", blocked: "需处理", completed: "已完成",
  cancelled: "已取消", queued: "待执行", superseded: "已改用后续步骤",
};
const operationLabels: Record<string, string> = {
  advisory: "讨论与规划", generateBrief: "生成简报", generateOutline: "生成大纲",
  generatePropertyRules: "生成属性规则提案",
  generatePlan: "生成近章规划", generateChapter: "生成章节", modelKnowledge: "分析章节知识",
  assessRevisionOutcome: "评估修改结果",
  reviewQuality: "独立读者审阅",
  assessChapterStructure: "检查章节衔接",
  extractCommitments: "提取伏笔候选",
  reviseCommitment: "修订伏笔候选", reviewPlanningSeat: "规划检查",
  assessSceneOutcome: "检查场景结果",
};
const deliveryLabels: Record<WorkTaskSummary["deliveryStatus"], string> = {
  pending: "保存结果待核对", partial: "已保存部分内容",
  ready_for_review: "结果已保存", discussion_ended: "讨论已结束",
};
// agent-events bounds local replies to 4096 characters and appends a marker
// when truncated. Longer local text stays exposed; no reply text is inspected.
const LOCAL_REPLY_DISPLAY_LIMIT = 4096;

const resumeBlockLabels: Record<NonNullable<WorkTaskSummary["resumeBlockReason"]>, string> = {
  pendingRecovery: "有尚未核对的写入结果，请先完成执行记录核对，避免重复写入。",
  readerRestart: "请在创作回看中检查已保存报告；尚无报告时可显式重开独立读者任务，不复用旧会话。",
  partialOutput: "已有未确认的部分写入，请先核对产物后重新发起任务。",
  inputChanged: "作品版本已变化，请根据当前内容重新发起任务。",
  attemptBudget: "本次自动分析已用完三次尝试，请检查模型连接，取消此任务并调整正文后重新分析。",
};
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
  interruptionBusy: boolean;
  agentRunning: boolean;
  agentAvailable: boolean;
  dirty: boolean;
  onControl: (id: string, command: "pause" | "cancel" | "resume" | "recoverSavedResult") => void;
  onUndo?: (commitId: string) => void;
  onLoadMore: () => void;
  onPrepareRevisionContinuation?: (predecessor: RevisionPredecessor, intent: string) => void;
  onPrepareRevisionRepair?: (preparation: RevisionRepairPreparation) => void;
  onOpenResult?: (result: TaskResultDestination) => void;
  readOnly?: boolean;
}

export function WorkTaskPanel({ conversation = null, api, projectId, tasks, activeCount, nextCursor, loadingMore, loaded, reading, readError, onRetry, revision, busy, interruptionBusy, agentRunning, agentAvailable, dirty, onControl, onUndo, onLoadMore, onPrepareRevisionContinuation, onPrepareRevisionRepair, onOpenResult, readOnly = false }: Props) {
  const panelRef = useRef<HTMLElement>(null);
  const scopedTasks = conversation ? tasks.filter(task => task.projectId === projectId && (
    Boolean(conversation.threadId) && task.threadId === conversation.threadId
    || task.steps.some(step => step.attempts.some(attempt => attemptContainsRun(attempt, conversation.run)
      || conversation.run.history.some(entry => attemptContainsRun(attempt, entry)))))) : tasks;
  const items = conversationItems(scopedTasks, conversation);
  const panelVisible = !loaded || items.length > 0 || Boolean(readError);
  const readingPosition = useConversationReading(api, JSON.stringify([projectId, conversation?.threadId ?? null]), panelRef, loaded && panelVisible);
  if (loaded && !items.length && !readError) return null;
  const controlsBusy = busy || !!readError || !loaded;
  const interruptionDisabled = interruptionBusy || !!readError || !loaded || dirty;
  const renderTask = (task: WorkTaskSummary, current: boolean, history: AgentRunSummary[]) => {
      const results = onOpenResult && task.projectId === projectId ? getTaskResults(task) : [];
      const reply = primaryReply(task);
      const presentation = task.projectId === projectId ? completedDeliveryPresentation(task) : null;
      const description = task.projectId === projectId ? generatedReplyDescription(task, results) : null;
      const deliveryLabel = task.deliveryReport.deliveryReview?.goalStatus === "partial" ? "已保存成果，目标尚未全部完成" : task.deliveryReport.deliveryReview?.goalStatus === "needsAuthorDecision" ? "已保存成果，需你决定后续安排" : presentation === "updated" ? "结果已保存，后续已有更新" : deliveryLabels[task.deliveryStatus];
      const renderHistory = (entry: AgentRunSummary) => {
        const content = conversation?.renderHistory(entry, { taskBound: true });
        return description && reply && entry.state === "completed" && !("error" in entry && entry.error) && entry.answer.trim() && entry.answer.length <= LOCAL_REPLY_DISPLAY_LIMIT
          && attemptContainsRun(reply.attempt, entry)
          ? <GeneratedReplyDescription projectId={projectId} taskId={description.attemptId} text={entry.answer}>{content}</GeneratedReplyDescription> : content;
      };
      const renderCurrent = () => {
        const run = conversation?.run;
        const content = conversation?.renderCurrent({ taskBound: true });
        return description && reply && run?.state === "completed" && !run.error && run.answer.trim() && run.answer.length <= LOCAL_REPLY_DISPLAY_LIMIT
          && attemptContainsRun(reply.attempt, run)
          ? <GeneratedReplyDescription projectId={projectId} taskId={description.attemptId} text={run.answer}>{content}</GeneratedReplyDescription> : content;
      };
      const historyPlacement = history.map(entry => ({ entry,
        binding: task.projectId === projectId ? intermediateReplyStep(task, entry) : null }));
      const currentPlacement = current && conversation && task.projectId === projectId
        ? intermediateReplyStep(task, conversation.run) : null;
      const localAdvisoryAnswers = new Set(historyPlacement.filter(item => item.binding && item.entry.answer.trim())
        .map(item => item.binding!.attemptId));
      if (currentPlacement && conversation?.run.answer.trim()) localAdvisoryAnswers.add(currentPlacement.attemptId);
      const taskError = task.error || task.steps.flatMap(step => step.attempts).at(-1)?.error;
      const replyInCurrent = current && conversation && reply && attemptContainsRun(reply.attempt, conversation.run);
      const replyInHistory = reply && history.some(entry => attemptContainsRun(reply.attempt, entry)
        && (entry.answer.trim() || (entry.state === "cancelled" && entry.displayExecution)));
      const savedReplyPlacement = reply ? intermediateReplyStep(task, {
        taskId: reply.attempt.id, threadId: reply.attempt.threadId, turnId: reply.attempt.turnId,
        state: reply.attempt.status === "completed" ? "completed" : "error",
      }) : null;
      const savedReply = reply && !replyInCurrent && !replyInHistory
        ? <SavedTaskReply key={reply.attempt.id} api={api} projectId={projectId} attempt={reply.attempt} foldDescription={Boolean(description)} /> : null;
      const duplicateError = current && sameConversationFailure(taskError ?? null, conversation?.run.error ?? null);
      const resumeBlocked = task.resumeBlockReason ? resumeBlockLabels[task.resumeBlockReason] : null;
      const localRecovery = task.resumeMode === "recoverSavedResult";
      const automatic = task.steps[0]?.operation === "assessChapterStructure";
      const resumeHelp = resumeBlocked ?? (localRecovery
        ? "只核验并恢复已保存产物，不调用模型；如有后续步骤，将保持暂停。"
        : !agentAvailable ? "需要可用模型才能继续执行；请先完成模型配置与验证。" : null);
      return <div key={task.id} className="work-task-row conversation-turn" data-work-task-id={task.id} data-conversation-row={`task:${task.id}`}>
        {!hasOriginalAuthorMessage(task, scopedTasks) && <article className={`conversation-message ${automatic ? "system-message" : "user-message"}`}>
          <div className="conversation-message-body"><div className="conversation-message-meta"><strong>{automatic ? "自动检查" : "你的创作指令"}</strong></div><ConversationPrompt text={task.goal} detailKey={`task:${task.id}:prompt`} /></div>
        </article>}
        <header className="work-task-status"><span>{task.status === "completed" ? "执行结束" : statusLabels[task.status] ?? task.status}
          {task.status === "completed" || task.deliveryStatus === "partial" ? ` · ${deliveryLabel}` : ""}</span></header>
        {task.deliveryReport.pendingActionIds.length > 0 && <p>{task.status === "running" ? "正在安排后续创作步骤…" : <>有 {task.deliveryReport.pendingActionIds.length} 项行动提案待确认；尚未执行。</>}</p>}
        {task.deliveryReport.unsettledAttemptIds.length > 0 && !["running", "queued"].includes(task.status)
          && <p className="conversation-process-meta">用量记录待核对。</p>}
        {historyPlacement.filter(item => !item.binding).map(({ entry }) => <div key={entry.id}>{renderHistory(entry)}</div>)}
        {current && !currentPlacement && renderCurrent()}
        {taskError && !duplicateError && <div role="alert"><p>{describeAgentError(taskError)}</p>{describeAgentError(taskError) !== taskError && <details><summary>查看错误详情</summary><pre>{taskError}</pre></details>}</div>}
        <details className="work-task-execution" data-conversation-detail={`task:${task.id}:flow`}><summary>创作流程 · {task.steps.length} 步</summary>
        <ol>{task.steps.map((step) => <li key={step.id} data-work-step-id={step.id}>
          <span>{operationLabels[step.operation] ?? step.operation} · {statusLabels[step.status] ?? step.status}</span>
          {step.operation === "reviewQuality" && <small>材料与报告可在「作品信息 → 创作回看」查看。</small>}
          {historyPlacement.filter(item => item.binding?.stepId === step.id).map(({ entry }) =>
            <div key={entry.id}>{renderHistory(entry)}</div>)}
          {currentPlacement?.stepId === step.id && renderCurrent()}
          {savedReplyPlacement?.stepId === step.id && savedReply}
          {task.deliveryReport.evidence.filter((item) => item.stepId === step.id && item.kind !== "completionReport").map((item) =>
            item.state === "recorded" ? <small key={item.stepId}>{item.kind === "knowledgeBatch" ? "已发布知识批次" : item.kind === "qualityReading" ? "阅读报告已保存（模型假设）" : "候选产物证据已核验"}</small> :
              item.state === "superseded" ? <small key={item.stepId}>原产物已被后续版本替换</small> :
              item.state === "missing" ? <small key={item.stepId}>未找到符合任务契约的产物证据</small> : null)}
          {step.attempts.length > 1 && <small>尝试 {step.attempts.length} 次</small>}
          {step.attempts.at(-1)?.error && <small>{step.attempts.at(-1)?.error}</small>}
          {step.attempts.filter((attempt) => attempt.id !== reply?.attempt.id && !localAdvisoryAnswers.has(attempt.id) && ["completed", "failed", "cancelled"].includes(attempt.status)).map((attempt) =>
            <TaskFinalReplyDetails key={attempt.id} api={api} projectId={projectId} taskId={attempt.id} />)}
          {step.operation === "generateChapter" && step.target.desiredOutcomes?.length && typeof step.target.chapterId === "string"
            ? step.attempts.filter(attempt => ["completed", "failed", "cancelled"].includes(attempt.status)).map(attempt =>
              <RevisionOutcomeDetails key={attempt.id} api={api} projectId={projectId} writerTaskId={attempt.id}
                chapterId={step.target.chapterId as string} desiredOutcomes={step.target.desiredOutcomes!}
                version={`${task.version}:${revision ?? ""}`} disabled={readOnly || controlsBusy || agentRunning || dirty}
                onPrepareRepair={onPrepareRevisionRepair}
                onPrepareContinuation={onPrepareRevisionContinuation ? predecessor => onPrepareRevisionContinuation(predecessor, task.goal) : undefined} />) : null}
          <ReadingProgress progress={step.readingProgress} />
          {step.sourceTaskId && <ActionPermissionDetails api={api} projectId={projectId} workTaskId={task.id}
            sourceTaskId={step.sourceTaskId} actionId={step.id} version={task.version} />}
        </li>)}</ol>
        {task.deliveryReport.deliveryReview && task.deliveryReport.deliveryReview.goalStatus !== "fulfilled" && <section aria-label="本轮目标核对">
          <p>{task.deliveryReport.deliveryReview.goalAssessment}</p>
          <p>检查范围：{task.deliveryReport.deliveryReview.checkedScope}</p>
          {task.deliveryReport.deliveryReview.unresolved.length > 0 && <><strong>尚未完成</strong><ul>{task.deliveryReport.deliveryReview.unresolved.map((item, index) => <li key={index}>{item}</li>)}</ul></>}
          {task.deliveryReport.deliveryReview.authorDecisions.length > 0 && <><strong>需要你决定</strong><ul>{task.deliveryReport.deliveryReview.authorDecisions.map((item, index) => <li key={index}>{item}</li>)}</ul></>}
        </section>}
        </details>
        {!savedReplyPlacement && savedReply}
        {!reply && task.status === "completed" && <p className="conversation-answer-empty">本轮没有保存的最终回复。</p>}
        {task.steps.some(step => step.changes.some(change => change.operation === "accept")) && <section className="work-task-changes" aria-label="正文与设定变更">
          {task.steps.flatMap(step => step.changes.filter(change => change.operation === "accept")).map(change =>
            <span key={change.commitId} className="work-task-change">已采用 v{change.resultRevision}
              {change.undoneByCommitId ? " · 已撤销" : !readOnly && onUndo && <button type="button" className="secondary-button"
                disabled={controlsBusy || agentRunning || dirty || revision !== change.resultRevision}
                title={revision !== change.resultRevision ? "作品已有后续修改，请先检查依赖" : "撤销这次采用"}
                onClick={() => onUndo(change.commitId)}>撤销</button>}
            </span>)}
        </section>}
        {onOpenResult && results.length > 0 && <div className="task-result-actions" aria-label="本轮保存的结果">
          {results.map(result => <button type="button" className="secondary-button"
            key={`${result.stepId}:${result.attemptId}:${result.referenceId}`} disabled={controlsBusy || agentRunning}
            onClick={() => onOpenResult(result)}>{taskResultLabel(result)}</button>)}
        </div>}
        {!readOnly && <div className="work-task-controls">
          {task.status === "running" && !task.steps.some(step => step.operation === "reviewQuality") && <button type="button" className="secondary-button" disabled={interruptionDisabled} onClick={() => onControl(task.id, "pause")}>暂停后续步骤</button>}
          {["paused", "blocked"].includes(task.status) && <button type="button" className="secondary-button" disabled={controlsBusy || agentRunning || dirty || (!localRecovery && !agentAvailable) || !!resumeBlocked} title={resumeHelp ?? undefined} onClick={() => onControl(task.id, localRecovery ? "recoverSavedResult" : "resume")}>{localRecovery ? "核验并恢复" : "继续任务"}</button>}
          {["running", "paused", "blocked"].includes(task.status) && <button type="button" className="secondary-button" disabled={interruptionDisabled} onClick={() => onControl(task.id, "cancel")}>取消任务</button>}
          {resumeHelp && ["paused", "blocked"].includes(task.status) && <small>{resumeHelp}</small>}
        </div>}
      </div>;
  };
  return <section ref={panelRef} className="work-task-panel conversation-panel" aria-label="创作对话">
    {!loaded && <p role="status">正在读取对话与执行记录…</p>}
    {activeCount > 0 && <p className="work-task-status" role="status">{activeCount} 项执行未结束</p>}
    {readError && <div role="alert"><p>{readError}。{loaded ? "保留上次成功读取的任务，状态可能已变化。" : "尚未读取到任务，不能判断是否有待处理任务。"}</p>
      <button type="button" className="secondary-button" disabled={reading} onClick={onRetry}>重试读取任务</button></div>}
    {nextCursor && <button type="button" className="conversation-load-earlier" disabled={reading || !!readError} onClick={onLoadMore}>{loadingMore ? "加载中…" : "加载更早对话"}</button>}
    <div className="work-task-list">
      {items.map(item => item.kind === "task" ? renderTask(item.task, item.current, item.history)
        : item.kind === "history" ? <div key={`history:${item.entry.id}`} data-conversation-row={`history:${item.entry.id}`}>{conversation?.renderHistory(item.entry, { taskBound: false })}</div>
          : <div key="current" data-conversation-row={`turn:${conversation?.run.threadId}:${conversation?.run.turnId ?? conversation?.run.startedAt}`}>{conversation?.renderCurrent({ taskBound: false })}</div>)}
    </div>
    {readingPosition.reading && <button type="button" className="conversation-jump-latest" onClick={readingPosition.jumpToLatest}>回到最新</button>}
  </section>;
}

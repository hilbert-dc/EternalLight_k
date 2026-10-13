import { useLayoutEffect, useMemo, type RefObject } from "react";
import { authorIntentError } from "../shared/author-intent-validation.mjs";
import type { AgentConnection } from './app-workspace-types';
import { type ChapterWritingRequest } from "./chapter-writing";
import { isComposerSubmit } from "./composer-keyboard";
import { ComposerDraftNotice } from "./ComposerDraftNotice";
import { ComposerModelPicker } from "./ComposerModelPicker";
import { ComposerRetryControl } from "./ComposerRetryControl";
import type { WorkTaskSummary } from "./types";

export function SendArrowIcon() { return <svg className="composer-icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M8 12V4m0 0L4.5 7.5M8 4l3.5 3.5" /></svg>; }

export function StopIcon() { return <svg className="composer-icon composer-stop-icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><rect x="5" y="5" width="6" height="6" rx="1" /></svg>; }

export type ComposerAvailability = {
  interludeHeld?: boolean;
  runtimeReady: boolean;
  modelReady: boolean;
  changingConversation: boolean;
  historyState: "ready" | "loading" | "error";
  archived: boolean;
  otherConversationRunning: boolean;
};

export function Composer({ retryChoices = [], retrySingleAction = true, retryScope = "", onRetryTask, onRetry, retryDisabled, retryLabel, onQueue, queueDisabled, projectId, planningSaving, planningSaveOnSend, choosingSource, canSteer, writing, writingPending, onCancelWriting, repairPrepared, checkingRepair, composer, setComposer, onSubmit, onInterrupt, composerRef, activity, agentStatus, availability, blockedByUnsavedChanges, projectReady, connections, agentConfigSaving, onSelectConnection, reasoningDisabled, onReasoningChange }: { retryChoices?: WorkTaskSummary[]; retrySingleAction?: boolean; retryScope?: string; onRetryTask?: (task: WorkTaskSummary) => void; onRetry?: () => void; retryDisabled: boolean; retryLabel: string; onQueue?: () => void; queueDisabled: boolean; projectId: string; planningSaving: boolean; planningSaveOnSend: boolean; choosingSource: boolean; canSteer: boolean; writing: ChapterWritingRequest | null; writingPending: boolean; onCancelWriting: () => void; repairPrepared: boolean; checkingRepair: boolean; composer: string; setComposer: (value: string) => void; onSubmit: () => void; onInterrupt: () => void; composerRef: RefObject<HTMLTextAreaElement | null>; activity: string[]; agentStatus: { configured: boolean; state: string; model?: string | null; provider?: string | null; reasoningEffort?: string | null }; availability: ComposerAvailability; blockedByUnsavedChanges: boolean; projectReady: boolean; connections: AgentConnection[]; agentConfigSaving: boolean; onSelectConnection: (connection: AgentConnection) => void; reasoningDisabled: boolean; onReasoningChange: (effort: string) => void }) {
  useLayoutEffect(() => {
    const node = composerRef.current;
    if (!node) return;
    const fit = () => {
      const top = node.scrollTop;
      node.style.height = "0px";
      node.style.height = `${Math.min(144, Math.max(48, node.scrollHeight))}px`;
      node.scrollTop = top;
    };
    fit();
    let width = node.clientWidth;
    const observer = new ResizeObserver(() => {
      if (node.clientWidth !== width) { width = node.clientWidth; fit(); }
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [composer, composerRef]);
  const inputError = useMemo(() => composer.trim() ? authorIntentError(composer) : null, [composer]);
  const submissionAvailable = availability.runtimeReady && availability.modelReady && !agentConfigSaving
    && !availability.changingConversation && availability.historyState === "ready"
    && !availability.archived && !availability.otherConversationRunning;
  const availabilityReason = availability.historyState === "error"
    ? "这条对话的历史暂时无法读取。请重试读取历史；对话草稿会保留。"
    : availability.archived ? "这条对话已归档。恢复对话后可发送；对话草稿会保留。"
    : availability.changingConversation ? "正在切换对话，完成后可发送；对话草稿会保留。"
    : availability.historyState === "loading" ? "正在读取这条对话，完成后可发送；对话草稿会保留。"
    : availability.otherConversationRunning ? "另一条对话正在执行。可返回正在执行的对话；当前草稿会保留。"
    : !availability.runtimeReady ? "创作环境暂未连接，恢复后可发送；对话草稿会保留。"
    : agentConfigSaving ? "正在应用模型配置，完成后可发送；对话草稿会保留。"
    : !availability.modelReady ? "模型连接尚不可用。请在设置中配置或验证连接；对话草稿可以先写。" : null;
  const active = agentStatus.state === "running";
  const waiting = agentStatus.state === "waiting";
  const disabled = availability.interludeHeld || planningSaving || choosingSource || (active && !canSteer) || !projectReady || (writingPending && !active) || waiting || blockedByUnsavedChanges || checkingRepair || (repairPrepared && active) || (!active && !submissionAvailable);
  const inputDisabled = !projectReady || checkingRepair || (repairPrepared && active);
  const draftReason = !projectReady ? null : availability.interludeHeld ? "原任务暂挂中：可再插入一个问题，或返回原任务。草稿会保留。" : planningSaving ? "正在保存并核对规划，完成后发送本条指令。" : choosingSource ? "请先选择保留或替换讨论来源，再发送。" : active && !canSteer ? "本轮结束后可发送；输入内容不会自动发送。" : blockedByUnsavedChanges ? "中央文档有未保存修改。保存后即可发送；可以先写下对话草稿。" : planningSaveOnSend ? "发送时先保存规划修改，再处理你的要求；不会自动采用。" : waiting ? "请先处理本轮需要回答的问题；可以先写下下一条指令。" : !submissionAvailable && !active ? availabilityReason : null;
  const latestActivity = activity[0];
  const modelLabel = agentStatus.model ?? (agentStatus.configured ? "已配置模型" : "未配置模型");
  const modelSelectionDisabled = active || waiting || writingPending || planningSaving || agentConfigSaving;
  const placeholder = !projectReady ? "请先创建或打开作品……" : active ? canSteer ? "追加指令，会在当前这一轮里处理……" : "先写下下一条指令，本轮结束后可发送……" : "想如何继续这个故事……";
  return <><div className="composer-card composer-global">{writing && <div className="chapter-writing-mode" role="status"><span>本章上下文 · 第 {writing.chapterNumber} 章 · {Array.from(writing.title).slice(0, 80).join("")}</span><button type="button" className="text-button" disabled={writingPending || active || waiting} onClick={onCancelWriting}>取消定位</button></div>}<ComposerDraftNotice reason={draftReason} /><div className="composer-body"><textarea ref={composerRef} aria-label="创作指令" aria-describedby={draftReason ? "composer-draft-notice composer-keyboard-hint" : "composer-keyboard-hint"} disabled={inputDisabled} value={composer} onChange={(event) => setComposer(event.target.value)} onKeyDown={(event) => { if (isComposerSubmit(event.nativeEvent,"enter")) { event.preventDefault(); if (!disabled && !inputError) onSubmit(); } }} placeholder={placeholder} /><div id="composer-keyboard-hint" className="composer-keyboard-hint">{!disabled && !inputError ? active && canSteer ? "Enter 追加指令 · " : "Enter 发送 · " : ""}Shift+Enter 换行</div><div className="composer-bottom"><div className="composer-choices"><ComposerModelPicker key={projectId} connections={connections} provider={agentStatus.provider} model={agentStatus.model} reasoningEffort={agentStatus.reasoningEffort} label={agentConfigSaving ? "正在配置…" : modelLabel} disabled={modelSelectionDisabled} onSelect={onSelectConnection} reasoningDisabled={reasoningDisabled} onReasoningChange={onReasoningChange} /></div><div className="composer-actions">{!active && !waiting && <ComposerRetryControl scope={retryScope} choices={retryChoices} singleAction={retrySingleAction} disabled={retryDisabled} agentAvailable={submissionAvailable} label={retryLabel} onRetry={onRetry} onSelect={onRetryTask} />}{onQueue && composer.trim() && <button type="button" className="composer-queue" disabled={queueDisabled || Boolean(inputError)} aria-label="当前任务结束后发送" title="当前任务完成后处理这条指令" onClick={onQueue}>稍后发送</button>}{active ? <><button className="composer-steer" disabled={disabled || Boolean(inputError) || !composer.trim()} aria-label={canSteer ? "追加创作指令" : "发送创作指令"} onClick={onSubmit}><SendArrowIcon /></button><button className="composer-stop" aria-label="停止当前执行" onClick={onInterrupt}><StopIcon /></button></> : waiting ? <><span className="composer-waiting">等待交互</span><button className="composer-stop" aria-label="停止当前执行" onClick={onInterrupt}><StopIcon /></button></> : <button className="composer-send" disabled={disabled || !composer.trim() || Boolean(inputError)} aria-label="发送创作指令" onClick={onSubmit}><SendArrowIcon /></button>}</div></div></div></div>{inputError && <p className="dialog-error" role="alert">{inputError}</p>}{latestActivity && <div className="activity-strip" role="status" aria-live="polite"><div><span className="activity-spark">✦</span>{latestActivity}</div></div>}</>;
}

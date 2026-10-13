import { useState } from "react";
import { userInputQuestions } from "../shared/author-questions.mjs";
import { describeAgentError } from "./agent-error-description";
import { type AgentRun } from "./agent-events";
import { artifactStatusLabel } from "./AgentInspector";
import type { UserInputRequest } from './app-workspace-types';
import { AuthorQuestionCard, type ResolveUserInput } from "./AuthorQuestionCard";
import { ConversationOutput, isUnfinishedConversationTurn, type FinalReplyDisplayState } from "./ConversationOutput";
import { ConversationPrompt } from "./ConversationPrompt";
import { ConversationReplyActions } from "./ConversationReplyActions";
import { ConversationTimeline } from "./ConversationTimeline";
import { MarkdownContent } from "./MarkdownContent";
import { DiscussionExcerptCards } from "./SelectionDiscussion";

export function messageClock(run: AgentRun): string { return new Date(run.startedAt ?? Date.now()).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" }); }

export function renderConversationAnswer(text: string) { return <MarkdownContent text={text}/>; }

export type ChangeStats = { added: number; removed: number; known: boolean };

export function changeStats(text: string): ChangeStats {
  let added = 0;
  let removed = 0;
  for (const line of text.split("\n")) {
    if (line.startsWith("+++") || line.startsWith("---")) continue;
    if (line.startsWith("+")) added += 1;
    if (line.startsWith("-")) removed += 1;
  }
  return { added, removed, known: added > 0 || removed > 0 };
}

export function totalChangeStats(run: AgentRun): ChangeStats {
  const source = run.diff || run.artifacts.map((artifact) => artifact.detail ?? "").filter(Boolean).join("\n");
  return changeStats(source);
}

export function ChangeStatsLabel({ stats }: { stats: ChangeStats }) {
  if (!stats.known) return null;
  return <span className="change-stats"><span className="change-stat-add">+{stats.added}</span><span className="change-stat-remove">−{stats.removed}</span></span>;
}

export function ConversationChanges({ run, onReview }: { run: AgentRun; onReview: () => void }) {
  const [open, setOpen] = useState(true);
  if (run.artifacts.length === 0 && !run.diff) return null;
  const pending = run.artifacts.some((artifact) => artifact.status === "proposed");
  const stats = totalChangeStats(run);
  const fileLabel = run.artifacts.length > 0 ? `${run.artifacts.length} 个文件已更改` : "内容已更改";
  return <section className={`conversation-changes ${pending ? "pending" : ""}`} aria-label="文件变更"><div className="conversation-change-header"><div className="conversation-change-title"><span className="conversation-change-icon" aria-hidden="true">▧</span><div><strong>{fileLabel}</strong><ChangeStatsLabel stats={stats} /></div></div><div className="conversation-change-actions"><button className="conversation-change-review" onClick={onReview}>查看章节</button></div></div>{open && <div className="conversation-change-list">{run.artifacts.map((artifact) => { const artifactStats = changeStats(artifact.detail ?? ""); return <article className="artifact-row" key={artifact.id}><span className="artifact-icon" aria-hidden="true">{artifact.kind === "chapter-change" ? "▤" : "◇"}</span><div className="artifact-copy"><div className="artifact-heading"><strong>{artifact.title}</strong><ChangeStatsLabel stats={artifactStats} /></div>{artifact.detail && <details className="change-detail"><summary>{artifactStatusLabel(artifact.status)} · 查看详情</summary><pre>{artifact.detail}</pre></details>}</div></article>; })}{run.diff && <details className="agent-diff"><summary>查看统一 diff</summary><pre>{run.diff}</pre></details>}</div>}<button className="conversation-change-toggle" aria-expanded={open} onClick={() => setOpen((value) => !value)}>{open ? "⌃" : "⌄"}</button></section>;
}

export function EarlierTurn({ projectId, entry, taskBound, finalReplyState }: { projectId: string; entry: AgentRun["history"][number] & { commentary?: string }; taskBound: boolean; finalReplyState?: FinalReplyDisplayState }) {
  const text = entry.answer.trim();
  const clock = new Date(entry.createdAt).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
  return <div className="conversation-turn is-earlier">{!taskBound && entry.prompt.trim().length > 0 && <article className={`conversation-message ${entry.origin === "chapterStructure" ? "system-message" : "user-message"}`}><div className="conversation-message-body">{entry.origin === "chapterStructure" && <strong>自动检查</strong>}{entry.prompt && <ConversationPrompt text={entry.prompt} detailKey={`turn:${entry.id}:prompt`} />}<div className="conversation-user-meta">{entry.createdAt > 0 && <span>{clock}</span>}<button className="conversation-copy" title={entry.origin === "chapterStructure" ? "复制检查说明" : "复制我的创作指令"} aria-label={entry.origin === "chapterStructure" ? "复制检查说明" : "复制我的创作指令"} onClick={() => { if (navigator.clipboard) void navigator.clipboard.writeText(entry.prompt).catch(() => undefined); }}>▢</button></div></div></article>}<DiscussionExcerptCards excerpts={entry.excerpts ?? []} disabled={false} readOnly={false} />{entry.displayExecution && <ConversationTimeline key={`${entry.threadId}:${entry.turnId ?? entry.id}`} run={{ ...entry, ...entry.displayExecution }} retained={entry.displayExecution} />}{entry.displayExecution === null && <small>较早的执行过程未保留。</small>}<article className="conversation-message assistant-message"><div className="conversation-message-body">{entry.commentary && !entry.displayExecution && <div className="conversation-progress-notes"><MarkdownContent text={entry.commentary} /></div>}{text ? <ConversationOutput state={entry.state} identity={`turn:${entry.threadId}:${entry.turnId ?? entry.id}:answer`} text={text} finalReplyState={finalReplyState}><div className="conversation-answer">{renderConversationAnswer(text)}</div><ConversationReplyActions identity={`${projectId}:${entry.threadId}:${entry.id}`} text={entry.answer} label={isUnfinishedConversationTurn(entry.state) ? "复制未完成内容" : "复制回复"} /></ConversationOutput> : <div className="conversation-answer-empty">{entry.summary}</div>}</div></article></div>;
}

export function EnhancedAgentTranscript({ projectId, run, taskBound, finalReplyState, userInputRequest, queuedUserInputCount, onResolveUserInput, onReview }: { projectId: string; run: AgentRun; taskBound: boolean; finalReplyState?: FinalReplyDisplayState; userInputRequest: UserInputRequest | null; queuedUserInputCount: number; onResolveUserInput: ResolveUserInput; onReview: () => void }) {
  if (run.state === "idle") return run.history.length ? <section className="agent-transcript conversation-panel" aria-label="恢复的创作对话">{[...run.history].reverse().map(entry => <EarlierTurn projectId={projectId} entry={entry} taskBound={false} key={entry.id} />)}</section> : null;
  const answer = run.answer.trim();
  const showAssistant = Boolean(answer) || (run.state !== "running" && !(run.state === "waiting" && userInputRequest));
  const earlier = [...run.history].reverse();
  return <section className="agent-transcript conversation-panel" id="agent-transcript" aria-label="创作引擎对话">{earlier.map((entry) => <EarlierTurn projectId={projectId} entry={entry} taskBound={false} key={entry.id} />)}<div className={`conversation-turn is-${run.state}`}>{!taskBound && run.prompt.trim().length > 0 && <article className={`conversation-message ${run.origin === "chapterStructure" ? "system-message" : "user-message"}`}><span className="transcript-avatar user">{run.origin === "chapterStructure" ? "·" : "你"}</span><div className="conversation-message-body"><div className="conversation-message-meta"><strong>{run.origin === "chapterStructure" ? "自动检查" : "你的创作指令"}</strong><span>{run.origin === "chapterStructure" ? "故事衔接" : "本轮输入"}</span></div><ConversationPrompt text={run.prompt} detailKey={`turn:${run.threadId}:${run.turnId ?? run.startedAt}:prompt`} /><div className="conversation-user-meta"><span>{messageClock(run)}</span><button className="conversation-copy" title={run.origin === "chapterStructure" ? "复制检查说明" : "复制我的创作指令"} aria-label={run.origin === "chapterStructure" ? "复制检查说明" : "复制我的创作指令"} onClick={() => { if (navigator.clipboard) void navigator.clipboard.writeText(run.prompt).catch(() => undefined); }}>▢</button></div></div></article>}<DiscussionExcerptCards excerpts={run.excerpts ?? []} disabled={false} readOnly={false} /><ConversationTimeline key={`${run.threadId}:${run.turnId ?? run.startedAt}`} run={run} showFailure={!run.error}/>{showAssistant && <article className="conversation-message assistant-message"><span className="conversation-avatar assistant" aria-hidden="true">✦</span><div className="conversation-message-body"><div className="conversation-message-meta"><strong>Novel Engine</strong><span>{answer ? isUnfinishedConversationTurn(run.state) ? "未完成内容" : "回复" : run.state === "running" ? "正在处理" : "暂无文本回复"}</span></div>{answer ? <ConversationOutput state={run.state} identity={`turn:${run.threadId}:${run.turnId ?? run.startedAt}:answer`} text={answer} finalReplyState={finalReplyState}><div className="conversation-answer">{renderConversationAnswer(answer)}{run.state === "running" && <span className="stream-cursor" aria-label="正在接收内容">▋</span>}</div><ConversationReplyActions identity={`${projectId}:${run.threadId}:${run.turnId ?? run.startedAt}`} text={run.answer} label={isUnfinishedConversationTurn(run.state) ? "复制未完成内容" : "复制回复"} /></ConversationOutput> : <div className="conversation-answer-empty" aria-live="polite"><span className="conversation-answer-pulse" aria-hidden="true">•</span>{run.state === "waiting" ? "等待你的回答后继续。" : run.state === "running" ? "正在读取故事上下文并生成回复……" : "本轮没有文本回复。"}</div>}</div></article>}{userInputRequest && <div className="conversation-interaction"><div className="conversation-interaction-heading"><span>需要你的回答</span><small>回答后创作引擎会继续当前回合{queuedUserInputCount > 0 ? ` · 之后还有 ${queuedUserInputCount} 个问题` : ""}</small></div><AuthorQuestionCard requestIdentity={JSON.stringify([projectId, userInputRequest.threadId ?? run.threadId, userInputRequest.turnId ?? run.turnId, userInputRequest.itemId ?? null, typeof userInputRequest.requestId, userInputRequest.requestId])} questions={userInputQuestions(userInputRequest.params)} onResolve={onResolveUserInput}/></div>}{run.error && <div className="transcript-error" role="alert"><strong>本轮未完成</strong><span>{describeAgentError(run.error)}</span>{describeAgentError(run.error) !== run.error && <details><summary>查看错误详情</summary><pre>{run.error}</pre></details>}</div>}<ConversationChanges run={run} onReview={onReview}/>{run.summary && !answer && <div className="conversation-result"><strong>{run.state === "completed" ? "本轮完成" : "本轮状态"}</strong><p>{run.summary}</p></div>}</div></section>;
}

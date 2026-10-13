import type { Dispatch, SetStateAction } from "react";
import { type RefObject } from "react";
import { workStopDescription } from "../shared/work-recovery-presentation.mjs";
import { createAgentRun, summarizeAgentRun, type AgentRun } from "./agent-events";
import type { UserInputRequest } from './app-workspace-types';
import { createConversationExecutionOwner, createRecoveredExecutionOwner, type ConversationExecutionOwner } from "./conversation-execution-events";
import { type ExecutionRetry } from "./execution-retry";
import { StoppedExecutionError, stoppedRun } from "./execution-stop";
import type { Chapter, GenerationHandoffProposal, NovelCodexApi, PlanningReviewStartRequest, RevisionPredecessor } from "./types";
import { useWorkTaskList } from "./useWorkTaskList";

type Input = {
  agentRunRef: RefObject<AgentRun>;
  beginExecutionScope: (execution?: ExecutionRetry) => {};
  promptScopeRef: RefObject<{}>;
  currentProjectRef: RefObject<string>;
  projectId: string;
  agentRun: AgentRun;
  setUserInputQueue: Dispatch<SetStateAction<UserInputRequest[]>>;
  setAgentRun: (update: AgentRun | ((run: AgentRun) => AgentRun)) => void;
  setActivity: Dispatch<SetStateAction<string[]>>;
  api: NovelCodexApi | undefined;
  syncExecutionWorkspace: (label: string, scope?: {}) => Promise<void>;
  reportExecutionFailure: (message: string, scope: object, label: string) => Promise<boolean>;
  creativeDirtyRef: RefObject<boolean>;
  promptSubmissionRef: RefObject<object | null>;
  generationHandoffRef: RefObject<{ proposal: GenerationHandoffProposal; scope: object; } | null>;
  setPromptPipeline: Dispatch<SetStateAction<{ projectId: string; scope: object; } | null>>;
  conversationExecutionRef: RefObject<{ api: NovelCodexApi | undefined; scope: object; owner: ConversationExecutionOwner; pendingStart?: { goal: string; originalRun: AgentRun; }; } | null>;
  setGenerationHandoff: Dispatch<SetStateAction<{ proposal: GenerationHandoffProposal; scope: object; } | null>>;
  setTaskControlMessage: Dispatch<SetStateAction<{ scope: object; error: { targetId: string; message: string; } | null; notice: { targetId: string; message: string; } | null; } | null>>;
  taskList: ReturnType<typeof useWorkTaskList>;
};

export function useGenerationActions({ agentRunRef, beginExecutionScope, promptScopeRef, currentProjectRef, projectId, agentRun, setUserInputQueue, setAgentRun, setActivity, api, syncExecutionWorkspace, reportExecutionFailure, creativeDirtyRef, promptSubmissionRef, generationHandoffRef, setPromptPipeline, conversationExecutionRef, setGenerationHandoff, setTaskControlMessage, taskList }: Input) {
  async function generateChapter(chapter: Chapter, intent: string, options: { povCharacterId: string | null; storyTime: number | null }) {
    if (agentRunRef.current.state === "running" || agentRunRef.current.state === "waiting") throw new Error("当前已有 Agent 任务正在执行");
    const prompt = `生成第 ${chapter.number} 章：${intent}`;
    const executionScope = beginExecutionScope({ kind: "taskRecovery", operation: "章节生成", view: "chapters", chapterId: chapter.id, intent });
    const executionIsCurrent = () => promptScopeRef.current === executionScope && currentProjectRef.current === projectId;
    const previous = summarizeAgentRun(agentRun);
    const history = previous ? [previous, ...agentRun.history] : agentRun.history;
    setUserInputQueue([]);
    setAgentRun(createAgentRun(prompt, history));
    setActivity((items) => [`开始生成第 ${chapter.number} 章：${chapter.title}`, ...items].slice(0, 4));
    try {
      if (!executionIsCurrent()) return;
      if (!api) throw new Error("桌面 Agent 未连接，当前只能手动编辑");
      const result = await api.agent.generateChapter(projectId, {
        chapterId: chapter.id,
        chapterNumber: chapter.number,
        povCharacterId: options.povCharacterId,
        storyTime: options.storyTime,
        intent,
      });
      if (!executionIsCurrent()) return;
      setActivity((items) => [`第 ${chapter.number} 章候选已保存，等待审阅。`, `创作引擎：${result.message}`, ...items].slice(0, 4));
      await syncExecutionWorkspace("章节候选已保存", executionScope);
    } catch (error) {
      if (!executionIsCurrent()) return;
      const message = error instanceof Error ? error.message : "未知错误";
      const failed = await reportExecutionFailure(message, executionScope, "章节生成未完成");
      await syncExecutionWorkspace("章节执行已返回", executionScope);
      if (!failed && executionIsCurrent() && agentRunRef.current.state === "cancelled") throw new StoppedExecutionError();
      if (failed) throw error;
    }
  }
  async function generateWork(intent: string, targetChapters: number, expectedProjectRevision: number,
    expectedBaseSnapshotId: string, expectedSourceDigest: string, revisionPredecessor: RevisionPredecessor | null, generationProposalId: string | null = null) {
    if (creativeDirtyRef.current || promptSubmissionRef.current || agentRunRef.current.state === "running" || agentRunRef.current.state === "waiting") throw new Error("请先保存当前修改并等待正在执行的任务结束");
    const handoff = generationHandoffRef.current;
    if (generationProposalId && (!handoff || handoff.scope !== promptScopeRef.current
      || handoff.proposal.proposalId !== generationProposalId || handoff.proposal.projectId !== projectId
      || handoff.proposal.authorIntent !== intent || handoff.proposal.targetChapters !== targetChapters
      || revisionPredecessor)) throw new Error("创作安排已变化，请从当前对话重新核对。");
    const executionScope = beginExecutionScope({ kind: "taskRecovery", operation: "持续创作", view: "chapters", intent });
    const executionIsCurrent = () => promptScopeRef.current === executionScope && currentProjectRef.current === projectId;
    const originalRun = agentRunRef.current;
    let hostNotStarted = false;
    const startingRun = createAgentRun(intent, summarizeAgentRun(originalRun) ? [summarizeAgentRun(originalRun)!, ...originalRun.history] : originalRun.history);
    promptSubmissionRef.current = executionScope;
    setPromptPipeline({ projectId, scope: executionScope });
    conversationExecutionRef.current = handoff && generationProposalId ? { api, scope: executionScope,
      owner: createConversationExecutionOwner(projectId, { taskId: handoff.proposal.sourceTaskId, threadId: handoff.proposal.threadId }),
      pendingStart: { goal: intent, originalRun } } : null;
    if (generationProposalId) setGenerationHandoff(null);
    setUserInputQueue([]);
    if (!generationProposalId) setAgentRun(startingRun);
    setActivity((items) => [`开始创作 ${targetChapters} 章工作稿`, ...items].slice(0, 4));
    try {
      if (!executionIsCurrent()) return;
      if (!api) throw new Error("桌面 Agent 未连接，无法自动创作");
      const run = await api.agent.generateWork(projectId, intent, targetChapters,
        expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest, revisionPredecessor, generationProposalId);
      if (!executionIsCurrent()) return;
      if (run.resumeFailure) {
        const proof = run.resumeFailure, paused = run.pausedWork;
        if (!run.id || run.status !== "paused" || proof.code !== "WORK_RESUME_NOT_STARTED"
          || proof.workTaskId !== `work-${run.id}` || typeof proof.message !== "string" || !proof.message.trim()
          || !paused || paused.id !== proof.workTaskId || paused.projectId !== projectId || paused.status !== "paused"
          || !Number.isInteger(paused.version) || paused.version < 0
          || (generationProposalId && paused.threadId !== handoff?.proposal.threadId)
          || (generationProposalId ? !conversationExecutionRef.current?.pendingStart : agentRunRef.current !== startingRun)) {
          throw new Error("新创作安排的启动结果未能核对，请重新读取任务。");
        }
        hostNotStarted = true;
        setAgentRun(originalRun);
        setTaskControlMessage({ scope: taskList.scope, error: { targetId: paused.id, message: `原任务尚未启动：${proof.message}` }, notice: null });
        throw new Error(`原任务尚未启动：${proof.message}`);
      }
      setGenerationHandoff((current) => current?.proposal.projectId === projectId ? null : current);
      setActivity((items) => [run.status === "completed" ? `${targetChapters} 章工作稿已生成` : run.status === "paused" ? "创作已暂停" : workStopDescription(run.blockedReason), ...items].slice(0, 4));
      await syncExecutionWorkspace("创作执行已返回，结果已保留", executionScope);
    } catch (error) {
      if (!executionIsCurrent()) return;
      const message = error instanceof Error ? error.message : "未知错误";
      if (hostNotStarted) {
        setActivity(items => [message, ...items].slice(0, 4));
        await syncExecutionWorkspace("创作任务未启动，原对话和已保存内容保留", executionScope);
        return;
      }
      const failed = await reportExecutionFailure(message, executionScope, "自动创作未完成");
      await syncExecutionWorkspace("创作执行已返回，可能有部分候选", executionScope);
      if (!failed && executionIsCurrent() && agentRunRef.current.state === "cancelled") throw new StoppedExecutionError();
      if (failed) throw error;
    } finally {
      if (promptSubmissionRef.current === executionScope) promptSubmissionRef.current = null;
      setPromptPipeline(current => current?.scope === executionScope ? null : current);
      if (conversationExecutionRef.current?.scope === executionScope) conversationExecutionRef.current = null;
    }
  }
  async function continueWork(runId: string, action: "continue" | "retry" | "feedbackContinue" | "feedbackRepair" = "continue", assessmentRevisionId = "", instruction = "") {
    if (creativeDirtyRef.current || promptSubmissionRef.current || agentRunRef.current.state === "running" || agentRunRef.current.state === "waiting") throw new Error("请先保存当前修改并等待正在执行的任务结束");
    const executionScope = beginExecutionScope({ kind: "taskRecovery", operation: "继续创作", view: "chapters", intent: instruction });
    const executionIsCurrent = () => promptScopeRef.current === executionScope && currentProjectRef.current === projectId;
    const originalRun = agentRunRef.current;
    let hostNotStarted = false;
    promptSubmissionRef.current = executionScope;
    setPromptPipeline({ projectId, scope: executionScope });
    setUserInputQueue([]);
    setActivity((items) => [action === "retry" ? "重试失败的创作步骤" : "继续已有创作运行", ...items].slice(0, 4));
    try {
      if (!executionIsCurrent()) return;
      if (!api) throw new Error("桌面 Agent 未连接，无法继续创作");
      const work = await api.runtime.getWorkTask(projectId, `work-${runId}`);
      if (!executionIsCurrent()) return;
      const owner = work.projectId === projectId && work.id === `work-${runId}` && work.kind === "generation"
        ? createRecoveredExecutionOwner(work) : null;
      if (!owner) throw new Error("原创作任务的来源未能核对，尚未继续执行。");
      conversationExecutionRef.current = { api, scope: executionScope, owner, pendingStart: { goal: work.goal, originalRun } };
      const run = action === "retry" ? await api.agent.retryWork(projectId, runId)
        : action === "feedbackContinue" || action === "feedbackRepair"
          ? await api.agent.resolveGenerationFeedback(projectId, runId, assessmentRevisionId, action === "feedbackRepair" ? "repairDraft" : "continue", instruction)
          : await api.agent.continueWork(projectId, runId);
      if (!executionIsCurrent()) return;
      if (run.resumeFailure) {
        const proof = run.resumeFailure, paused = run.pausedWork;
        if (run.id !== runId || run.status !== "paused" || proof.code !== "WORK_RESUME_NOT_STARTED"
          || proof.workTaskId !== work.id || typeof proof.message !== "string" || !proof.message.trim() || !paused || paused.id !== work.id
          || paused.projectId !== projectId || paused.threadId !== work.threadId || paused.status !== "paused"
          || !Number.isInteger(paused.version) || paused.version < work.version || !conversationExecutionRef.current?.pendingStart) {
          throw new Error("原创作任务恢复结果的来源无法核对，请重新读取任务。");
        }
        hostNotStarted = true;
        throw new Error(`原任务尚未启动：${proof.message}`);
      }
      setActivity((items) => [run.status === "completed" ? "自动创作已完成" : run.status === "running" ? "创作进行中" : run.status === "paused" ? "创作已暂停" : workStopDescription(run.blockedReason), ...items].slice(0, 4));
      await syncExecutionWorkspace("继续创作已返回，结果已保留", executionScope);
    } catch (error) {
      if (!executionIsCurrent()) return;
      const message = error instanceof Error ? error.message : "未知错误";
      if (hostNotStarted) {
        setTaskControlMessage({ scope: taskList.scope, error: { targetId: `work-${runId}`, message }, notice: null });
        setActivity(items => [message, ...items].slice(0, 4));
        await syncExecutionWorkspace("原任务未启动，已保存内容保留", executionScope);
        return;
      }
      const failed = await reportExecutionFailure(message, executionScope, "继续创作失败");
      await syncExecutionWorkspace("继续创作已返回，可能有部分候选", executionScope);
      if (!failed && executionIsCurrent() && agentRunRef.current.state === "cancelled") throw new StoppedExecutionError();
      if (failed) throw error;
    } finally {
      if (promptSubmissionRef.current === executionScope) promptSubmissionRef.current = null;
      setPromptPipeline(current => current?.scope === executionScope ? null : current);
      if (conversationExecutionRef.current?.scope === executionScope) conversationExecutionRef.current = null;
    }
  }
  async function reviewPlanning(request: PlanningReviewStartRequest) {
    if (creativeDirtyRef.current || promptSubmissionRef.current || agentRunRef.current.state === "running" || agentRunRef.current.state === "waiting") throw new Error("请先保存当前修改并等待正在执行的任务结束");
    const prompt = `规划深审：${request.authorIntent}`;
    const executionScope = beginExecutionScope({ kind: "taskRecovery", operation: "规划深审", view: request.artifactKind === "plan" ? "plans" : "outline", intent: request.authorIntent });
    const executionIsCurrent = () => promptScopeRef.current === executionScope && currentProjectRef.current === projectId;
    const previous = summarizeAgentRun(agentRun);
    const history = previous ? [previous, ...agentRun.history] : agentRun.history;
    setUserInputQueue([]);
    setAgentRun(createAgentRun(prompt, history));
    setActivity((items) => [`开始规划深审 · ${request.roles.length} 席`, ...items].slice(0, 4));
    try {
      if (!executionIsCurrent()) return;
      if (!api) throw new Error("桌面 Agent 未连接，无法启动规划深审");
      const result = await api.agent.reviewPlanning(projectId, request);
      if (!executionIsCurrent()) return;
      const resultLabel = result.run.status === "completed" ? "全部完成" : result.run.status === "partial" ? "部分完成" : "已停止";
      setActivity((items) => [`规划深审${resultLabel}，已保存 ${result.run.reports.length}/${result.run.roles.length} 席报告。`, ...items].slice(0, 4));
      await syncExecutionWorkspace(`规划深审${resultLabel}`, executionScope);
    } catch (error) {
      if (!executionIsCurrent()) return;
      const message = error instanceof Error ? error.message : "未知错误";
      const failed = await reportExecutionFailure(message, executionScope, "规划深审未完整结束");
      await syncExecutionWorkspace("规划深审已返回，已保存报告可重新读取", executionScope);
      if (!failed && executionIsCurrent() && agentRunRef.current.state === "cancelled") throw new StoppedExecutionError();
      if (failed) throw error;
    }
  }
  async function cancelPlanningReview(runId: string) {
    const scope = promptScopeRef.current;
    if (!api) throw new Error("Runtime 尚未连接，无法停止规划深审");
    const run = await api.runtime.cancelPlanningReview(projectId, runId);
    if (promptScopeRef.current !== scope || currentProjectRef.current !== projectId) return;
    setAgentRun(stoppedRun);
    setActivity((items) => [`规划深审已停止，已保存 ${run.reports.length}/${run.roles.length} 席报告。`, ...items].slice(0, 4));
    await syncExecutionWorkspace("规划深审已停止", scope);
  }

  return { generateWork, continueWork, reviewPlanning, cancelPlanningReview, generateChapter };
}

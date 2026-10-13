import type { Dispatch, SetStateAction } from "react";
import { useEffect, useState } from "react";
import type { GenerationPermitReview, WorkspaceView } from "./creative-workspace-types";
import type { CreativeWorkspace, GenerationRunSummary, NovelCodexApi, RevisionContinuationPreparation, SceneAssessmentSummary } from "./types";
import type { useWorkTaskControls } from "./useWorkTaskControls";
import type { useWorkTaskList } from "./useWorkTaskList";

type Input = {
  projectId: string;
  view: WorkspaceView;
  revisionContinuation: RevisionContinuationPreparation | null;
  api: NovelCodexApi | undefined;
  agentRunning: boolean;
  refreshToken: number;
  setError: Dispatch<SetStateAction<string | null>>;
  taskList: ReturnType<typeof useWorkTaskList>;
  workspace: CreativeWorkspace | null;
  taskControls: ReturnType<typeof useWorkTaskControls>;
  dirty: boolean;
  setNotice: Dispatch<SetStateAction<string | null>>;
  onFormalChange: () => Promise<void>;
};

export function useGenerationProgress({ projectId, view, revisionContinuation, api, agentRunning, refreshToken, setError, taskList, workspace, taskControls, dirty, setNotice, onFormalChange }: Input) {
  const [generationRuns, setGenerationRuns] = useState<GenerationRunSummary[]>([]);
  const [generationRunsProjectId, setGenerationRunsProjectId] = useState<string | null>(null);
  const [generationReadError, setGenerationReadError] = useState(false);
  const [generationReload, setGenerationReload] = useState(0);
  const [generationPermitReview, setGenerationPermitReview] = useState<GenerationPermitReview | null>(null);
  useEffect(() => { setGenerationPermitReview(null); }, [projectId, view]);
  useEffect(() => {
    if (revisionContinuation?.projectId === projectId) {
      setGenerationPermitReview({ ...revisionContinuation, targetChapters: 1 });
    }
  }, [revisionContinuation, projectId]);
  const [futureIntent, setFutureIntent] = useState("");
  const [futureIntentSaving, setFutureIntentSaving] = useState(false);
  const [feedbackAssessment, setFeedbackAssessment] = useState<SceneAssessmentSummary | null>(null);
  const [feedbackInstruction, setFeedbackInstruction] = useState("");
  useEffect(() => { setGenerationRuns([]); setGenerationRunsProjectId(null); setGenerationReadError(false); }, [projectId]);
  useEffect(() => { setFutureIntent(""); }, [projectId]);
  useEffect(() => {
    let cancelled = false;
    const refresh = () => { void api?.runtime.listGenerationRuns(projectId).then((result) => {
      if (!cancelled) { setGenerationRuns(result.runs); setGenerationRunsProjectId(projectId); setGenerationReadError(false); }
    }).catch(() => { if (!cancelled) setGenerationReadError(true); }); };
    refresh();
    const interval = agentRunning ? window.setInterval(refresh, 1500) : undefined;
    return () => { cancelled = true; if (interval !== undefined) window.clearInterval(interval); };
  }, [api, projectId, refreshToken, agentRunning, generationReload]);
  const currentGenerationRun = generationRunsProjectId === projectId
    ? generationRuns.find((run) => run.status === "running" || run.status === "paused" || run.status === "blocked")
    : undefined;
  const feedbackRepairBudgetUnavailable = currentGenerationRun
    ? currentGenerationRun.providerBudget.overrun
      || (currentGenerationRun.providerBudget.maxRequests !== null && currentGenerationRun.providerBudget.maxRequests <= currentGenerationRun.providerBudget.usedRequests)
      || (currentGenerationRun.providerBudget.maxTokens !== null && currentGenerationRun.providerBudget.maxTokens <= currentGenerationRun.providerBudget.usedTokens + currentGenerationRun.providerBudget.unknownTokens)
      || (currentGenerationRun.maxTaskAttempts !== null && currentGenerationRun.usedTaskAttempts >= currentGenerationRun.maxTaskAttempts)
    : true;
  const feedbackRepairUnavailable = feedbackRepairBudgetUnavailable || currentGenerationRun?.stage !== "coordinate"
    || currentGenerationRun.status !== "blocked" || Boolean(currentGenerationRun.activeTaskId);
  useEffect(() => {
    let cancelled = false;
    if (currentGenerationRun?.blockedReason !== "scene_feedback_needs_author" || !currentGenerationRun.currentChapterId || !api) {
      setFeedbackAssessment(null);
      return () => { cancelled = true; };
    }
    setFeedbackAssessment(null);
    void api.runtime.getSceneAssessment(projectId, currentGenerationRun.currentChapterId).then((result) => {
      if (!cancelled) setFeedbackAssessment(result);
    }).catch((reason) => { if (!cancelled) setError(reason instanceof Error ? reason.message : "读取场景评估失败"); });
    return () => { cancelled = true; };
  }, [api, projectId, currentGenerationRun?.id, currentGenerationRun?.blockedReason, currentGenerationRun?.currentChapterId, refreshToken]);
  const activeGenerationStep = currentGenerationRun && taskList.tasks.find(task => task.projectId === projectId
    && task.id === `work-${currentGenerationRun.id}` && task.kind === "generation")?.steps.find(step =>
      step.currentAttemptId === currentGenerationRun.activeTaskId);
  const futureIntentBlockedByActiveStep = !currentGenerationRun || currentGenerationRun.stage !== "coordinate"
    || currentGenerationRun.pendingCurrentIntentGeneration !== null
    || Boolean(currentGenerationRun.activeTaskId && !currentGenerationRun.currentChapterId)
    || currentGenerationRun.completedChapters + (currentGenerationRun.currentChapterId ? 2 : 1) > currentGenerationRun.targetChapters;
  const currentRevisionUnavailable = currentGenerationRun?.stage !== "coordinate"
    || !currentGenerationRun.activeTaskId || !currentGenerationRun.currentChapterId || !currentGenerationRun.currentChapterNumber
    || activeGenerationStep?.operation !== "generateChapter"
    || activeGenerationStep.target.chapterId !== currentGenerationRun.currentChapterId
    || currentGenerationRun.pendingCurrentIntentGeneration !== null
    || (currentGenerationRun.maxTaskAttempts !== null && currentGenerationRun.usedTaskAttempts >= currentGenerationRun.maxTaskAttempts);
  async function undoWorkChange(commitId: string) {
    if (!api || !workspace || agentRunning) return;
    await taskControls.run("undo", commitId, () => api.runtime.undoCommit(projectId, commitId, workspace.revision),
      "撤销失败：请检查后续修改是否依赖这个版本", "已撤销这次采用，作品版本已更新。");
  }
  async function applyGenerationIntent(target: "current" | "future") {
    if (!api || !currentGenerationRun || currentGenerationRun.status !== "running" || futureIntentSaving || dirty
      || (target === "current" ? currentRevisionUnavailable : futureIntentBlockedByActiveStep)) return;
    const instruction = futureIntent.trim();
    if (!instruction || Array.from(instruction).length > 1_000) return;
    setFutureIntentSaving(true);
    setError(null);
    try {
      const run = target === "current"
        ? await api.agent.reviseCurrentChapter(projectId, currentGenerationRun.id, instruction)
        : await api.agent.addFutureIntent(projectId, currentGenerationRun.id, instruction);
      setFutureIntent("");
      setNotice(target === "current"
        ? `当前章修订要求已保存为第 ${run.intentGeneration} 版；本步结束后将基于候选稿重写。`
        : `后续创作要求已保存为第 ${run.intentGeneration} 版；已有章节不会重写。`);
      await onFormalChange();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "保存创作要求失败");
    } finally {
      setFutureIntentSaving(false);
    }
  }

  return { currentGenerationRun, generationRunsProjectId, generationReadError, undoWorkChange, generationPermitReview, setGenerationReload, setGenerationRuns, feedbackAssessment, feedbackInstruction, setFeedbackInstruction, feedbackRepairUnavailable, feedbackRepairBudgetUnavailable, futureIntent, futureIntentSaving, futureIntentBlockedByActiveStep, currentRevisionUnavailable, setFutureIntent, applyGenerationIntent, setGenerationPermitReview };
}

import { validateAuthorIntent } from "../shared/author-input-budget.mjs";

// Same three-field provenance contract as RevisionPredecessor; no execution authority.
const sourceFields = ["writerTaskId", "assessmentRevisionId", "decisionId"];
const revisionId = value => typeof value === "string" && value.length > 0 && value.trim() === value
  && Buffer.byteLength(value, "utf8") <= 160 && !/[\p{Cc}]/u.test(value);

function validateProjectId(projectId) {
  if (!revisionId(projectId) || [".", ".."].includes(projectId)) throw new Error("作品 ID 无效");
}

export function validateRevisionRepairSource(source = null) {
  if (source === null) return null;
  if (!source || typeof source !== "object" || Array.isArray(source)
    || Reflect.ownKeys(source).length !== sourceFields.length
    || sourceFields.some(key => !Object.hasOwn(source, key))
    || typeof source.writerTaskId !== "string" || !/^task-[a-f0-9]{32}$/.test(source.writerTaskId)
    || !revisionId(source.assessmentRevisionId)
    || typeof source.decisionId !== "string" || !/^[A-Za-z0-9-]{1,128}$/.test(source.decisionId)) {
    throw new Error("修复讨论来源无效，必须提供精确的三字段来源");
  }
  return Object.freeze(Object.fromEntries(sourceFields.map(key => [key, source[key]])));
}

export function assertAdvisoryPromptRoute({ revisionRepairSource = null, resumeStep = null, steer = false,
  activeWorkFlow = null, activeAgentExecution = null, activeGenerationRunId = null }) {
  const source = validateRevisionRepairSource(revisionRepairSource);
  if (source && (steer || resumeStep)) {
    throw new Error("修复来源不能与 steer 或 resumeStep 同时使用");
  }
  if (!steer && (activeWorkFlow || activeAgentExecution || activeGenerationRunId)) {
    throw new Error("已有创作任务正在推进，不能开始新的讨论");
  }
}

export function advisoryTaskTarget({ resumeStep = null, revisionRepairSource = null } = {}) {
  const source = validateRevisionRepairSource(revisionRepairSource);
  assertAdvisoryPromptRoute({ resumeStep, revisionRepairSource: source });
  if (resumeStep && Object.hasOwn(resumeStep.target ?? {}, "revisionRepairSource")) {
    const resumedSource = validateRevisionRepairSource(resumeStep.target.revisionRepairSource);
    if (!resumedSource || resumeStep.target.type !== "project" || Object.keys(resumeStep.target).length !== 2) {
      throw new Error("已持久修复讨论的冻结 target 无效");
    }
    // The caller supplies no new source. Core verifies the actual persisted step
    // and same-Work predecessor; resumeStepId preserves its budget owner.
    return Object.freeze({ ...resumeStep.target, revisionRepairSource: resumedSource, resumeStepId: resumeStep.id });
  }
  return Object.freeze(resumeStep ? { ...resumeStep.target, resumeStepId: resumeStep.id }
    : source ? { type: "project", revisionRepairSource: source } : { type: "project" });
}

export async function preflightRevisionRepair({ runtimeRequest, projectId, source }) {
  if (source === null) return;
  validateProjectId(projectId);
  source = validateRevisionRepairSource(source);
  // GET is project-scoped and validates the owning Writer chain in Core.
  // Task creation still checks this exact edge again inside its transaction.
  const outcome = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/revision-outcomes/${source.writerTaskId}`);
  const assessment = outcome?.assessment;
  const decision = Array.isArray(outcome?.decisions) ? outcome.decisions[0] : null;
  if (outcome?.state !== "current" || outcome.authority !== "candidateAnalysis"
    || outcome.revisionId !== source.assessmentRevisionId || assessment?.writerTaskId !== source.writerTaskId
    || decision?.id !== source.decisionId || decision.writerTaskId !== source.writerTaskId
    || decision.assessmentRevisionId !== source.assessmentRevisionId || decision.choice !== "repairDraft"
    || !revisionId(assessment.draftRevisionId) || !revisionId(assessment.knowledgeBatchId)
    || decision.draftRevisionId !== assessment.draftRevisionId || decision.knowledgeBatchId !== assessment.knowledgeBatchId) {
    throw new Error("修复来源已失效或不是最新 repairDraft 决定，请刷新后重试");
  }
}

export function assertRevisionRepairTaskTarget(task, projectId, target) {
  if (!target.revisionRepairSource) return;
  const actual = task?.target;
  const source = validateRevisionRepairSource(actual?.revisionRepairSource);
  if (task?.projectId !== projectId || task.operation !== "advisory" || actual?.type !== "project"
    || Object.keys(actual).length !== Object.keys(target).length || !source
    || Object.keys(target).some(key => key !== "revisionRepairSource" && actual[key] !== target[key])
    || sourceFields.some(key => source[key] !== target.revisionRepairSource[key])) {
    throw new Error("Runtime 未保留精确的修复讨论 target");
  }
}

export const REVISION_REPAIR_INSTRUCTIONS = '本次是修改结果的修复讨论。read_context 中冻结的 operationContext.revisionRepairInput 仅是候选分析来源，不是已确认事实或写权限；source 与 authorDecision 只记录作者 repairDraft 决定，不继承原 Writer 的预算、目标或审批。先读取 operationContext.revisionRepairInput 并核对正文证据。若需登记修复候选，沿用 request_candidate_task(operation=generateChapter)，target.type=chapter，target.chapterId/chapterNumber 必须是 operationContext.revisionRepairInput 原章节；scope={type:"selectedScenes",sourceRevisionId:operationContext.revisionRepairInput.authorDecision.draftRevisionId,groupIds:[场景id]}，groupIds 必须是 operationContext.revisionRepairInput.selectedGroupIds 的非空子集，并提出非空的新 desiredOutcomes。可参考原 desiredOutcomes 与 assessment 细化标准，但不能自动复制旧目标或沿用旧审批；候选必须经独立人工审批后才能执行。此入口不支持其他章节或 planning task；范围不足时向作者说明。不得自动启动 Writer、扩大范围、采用候选或确认事实。';

export function registerAdvisoryPromptHandler(ipcMain, { withWorkFlow, runtimeRequest, runAdvisoryAgentTask, routingState }) {
  ipcMain.handle("agent:send-prompt", async (event, projectId, prompt, revisionRepairSource = null) => {
    validateProjectId(projectId);
    validateAuthorIntent(prompt);
    const source = validateRevisionRepairSource(revisionRepairSource);
    assertAdvisoryPromptRoute({ ...routingState(), revisionRepairSource: source });
    return withWorkFlow(projectId, async () => {
      const discussion = await runAdvisoryAgentTask(event, projectId, prompt, null, source);
      const path = `/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(discussion.workflow.task.id)}/actions`;
      const { actions } = await runtimeRequest(path);
      return { ...discussion, proposedActions: actions.filter(action => action.status === "proposed") };
    });
  });
}

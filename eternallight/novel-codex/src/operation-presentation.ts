export interface SourcePresentation { id: string; label: string; sectionPath: string }
export interface OperationPresentation {
  category: "read" | "write" | "analysis" | "arrange" | "diagnostic";
  tool: string;
  object: string;
  sourceId?: string;
  targetId?: string;
  reportWriterTaskId?: string;
  retryKey?: string;
  outcome?: "read" | "saved" | "isolatedCandidate" | "processed" | "paused";
  referenceId?: string;
  sources?: SourcePresentation[];
  contentScope?: "index" | "page";
}
type RecordValue = Record<string, unknown>;
const object = (value: unknown): RecordValue | null => value !== null && typeof value === "object" && !Array.isArray(value) ? value as RecordValue : null;
const str = (value: unknown): string | undefined => typeof value === "string" && value.trim() ? value : undefined;
const safeId = (value: unknown): string | undefined => typeof value === "string" && /^[^\s\p{Cc}]{1,160}$/u.test(value) ? value : undefined;
const chapterNumber = (value: unknown): number | undefined => Number.isSafeInteger(value) && Number(value) > 0 ? Number(value) : undefined;
function sourceLabel(path: unknown, numbers: unknown): string | undefined {
  const label = SOURCE_LABELS[str(path) ?? ""];
  if (!label || !Array.isArray(numbers) || !numbers.length || numbers.length > 64
    || numbers.some(value => !chapterNumber(value))) return label;
  const chapters = [...new Set(numbers as number[])].sort((a, b) => a - b);
  const scope = chapters.length > 1 && chapters.every((value, index) => !index || value === chapters[index - 1] + 1)
    ? `${chapters[0]}—${chapters.at(-1)}` : chapters.slice(0, 8).join("、");
  return `${label}（第 ${scope} 章${chapters.length > 8 && !chapters.every((value, index) => !index || value === chapters[index - 1] + 1) ? `等 ${chapters.length} 章` : ""}）`;
}

export const TOOL_TITLES: Record<string, string> = {
  read_media_workspace: "读取当前作品与素材",
  read_media_source: "读取改编原文与引用",
  save_media_document: "保存作品修改",
  patch_media_document: "调整作品选定内容",
  request_media_generation: "生成创作素材",
  read_media_generation: "核对生成结果",
  wait_media_generation: "等待生成结果",
  cancel_media_generation: "停止生成请求",
  read_context: "读取故事上下文",
  read_tool_guide: "查看工具说明",
  read_work_plan: "核对工作计划",
  update_work_plan: "更新工作计划",
  read_source_content: "读取写作资料",
  read_work_status: "核对创作进展",
  read_work_issues: "读取问题记录",
  read_revision_outcome: "读取章节检查报告",
  request_work_action: "安排下一步",
  request_generation_handoff: "准备续写安排",
  pause_generation_run: "暂停连续创作",
  read_chapter_structure_source: "读取删前章节",
  save_brief: "保存创作简报",
  save_property_rules: "保存作品规则",
  save_outline: "保存大纲",
  save_chapter_plans: "保存章纲",
  save_draft_checkpoint: "保存章节",
  save_delegated_draft: "保存候选稿",
  read_delegated_draft: "读取候选稿",
  authorize_delegated_writer: "安排协作起草",
  publish_knowledge_batch: "保存故事设定分析",
  query_story: "查询故事证据",
  read_creative_strategy: "参考创作方法",
  read_quality_notebook: "读取创作回看",
  read_quality_review: "核对阅读反馈",
  record_quality_note: "记录作品质量线索",
  read_quality_packet: "阅读匿名文本",
  submit_quality_reading: "保存读者体验报告",
  analyze_story_impact: "检查故事关联影响",
  resolve_story_evidence: "定位正文依据",
  simulate_story_hypothesis: "分析故事假设",
  request_planning_review: "安排多视角规划检查",
  request_candidate_task: "登记本轮创作任务",
  submit_commitment_extraction: "保存伏笔候选",
  search_story: "查找故事内容",
  read_commitment: "读取故事伏笔与承诺",
  search_relations: "查询人物关系",
  read_reading_progress: "读取阅读进度",
  checkpoint_reading: "保存阅读进度",
  submit_revision_outcome: "保存章节检查结果",
  submit_planning_review_report: "提交审阅报告",
  complete_task: "完成任务",
};

const ACTION_PREFIX = /^(读取|保存|核对|查询|查找|定位|参考|阅读|检查|分析|安排|准备|记录|提交|更新|登记|完成|暂停)/;
const toolTitle = (tool: string): string | undefined => Object.hasOwn(TOOL_TITLES, tool) ? TOOL_TITLES[tool] : undefined;


const SOURCE_LABELS: Record<string, string> = {
      "/authorRequirementsSource": "作者要求", "/taskWorkspace": "写作资料",
      "/narrativeGoals/brief": "创作简报", "/narrativeGoals/outline": "作品大纲",
      "/plannedEffects": "章节规划", "/relevantText": "相关正文",
      "/relevantCommitments": "伏笔与承诺", "/candidateCommitmentProposals": "伏笔候选",
      "/sceneOutcomeRefs": "场景结果", "/operationContext/boundCommitmentInputs": "本轮伏笔资料",
      "/operationContext/modeling": "故事设定", "/operationContext/planningReview": "规划审阅资料",
      "/operationContext/modelingBatch": "设定分析资料", "/operationContext/entityCatalog": "人物与设定目录",
      "/operationContext/workingKnowledge": "故事知识", "/operationContext/scenePlanDraftLinks": "场景规划与正文关联",
      "/operationContext/generationSceneFeedback": "场景写作反馈", "/operationContext/propertyRulesBase": "作品规则",
      "/operationContext/revisionOutcomeInput": "章节检查资料", "/operationContext/revisionOutcomeReport": "章节检查报告", "/operationContext/revisionRepairInput": "修改修复资料",
      "/operationContext/chapterStructureChange": "章节调整资料",
    };

/** Read the current protocol's structured tool output, never arbitrary nested prose. */
export function operationResult(item: RecordValue): RecordValue | null {
  const result = object(item.result);
  const structured = object(result?.structuredContent);
  if (structured) return structured;
  const parts = Array.isArray(result?.content) ? result.content : Array.isArray(item.contentItems) ? item.contentItems : [];
  for (const entry of parts.slice(0, 12)) {
    const text = str(object(entry)?.text);
    if (!text || text.length > 64 * 1024) continue;
    try { const parsed = object(JSON.parse(text)); if (parsed) return parsed; } catch { /* A text error has no structured receipt. */ }
  }
  return null;
}

export function operationPresentation(item: RecordValue, sources: SourcePresentation[] = []): OperationPresentation {
  const tool = (str(item.tool) ?? "").split(/\.|__/).filter(Boolean).at(-1) ?? "";
  const title = toolTitle(tool);
  const result = operationResult(item);
  const args = object(item.arguments) ?? {};
  const draft = tool === "save_draft_checkpoint" || tool === "save_delegated_draft" ? object(args.payload) ?? object(args.patch) : null;
  const requestedSource = safeId(args.sourceId), returnedSource = safeId(result?.sourceId);
  const sourceMatches = !requestedSource || !returnedSource || requestedSource === returnedSource;
  const sourceId = returnedSource ?? requestedSource;
  const sourceIndex = tool === "read_source_content" && (args.view === "index" || args.view === "workspace" || result?.representation === "sourceIndex");
  const internal = ["read_tool_guide", "read_context", "read_work_plan", "update_work_plan", "read_work_status", "request_candidate_task", "complete_task", "read_reading_progress", "checkpoint_reading", "read_creative_strategy", "authorize_delegated_writer"];
  const category: OperationPresentation["category"] = !title || internal.includes(tool) ? "diagnostic"
    : /^(read_|search_|query_|resolve_)/.test(tool) ? "read"
    : /^(save_|patch_|publish_|submit_|record_|checkpoint_)/.test(tool) ? "write"
    : /^(request_|update_|complete_|pause_)/.test(tool) ? "arrange" : "analysis";
  let label = title ? title.replace(ACTION_PREFIX, "") : "内部操作";
  const mediaDocument = object(result?.documentHeader);
  if ((tool === "read_media_workspace" || tool === "save_media_document" || tool === "patch_media_document")
    && ["comic", "animation"].includes(str(mediaDocument?.kind) ?? "")) label = mediaDocument?.kind === "comic" ? "漫画文档" : "动画时间线";
  if (tool.endsWith("_media_generation")) {
    const operation = object(result?.operation), request = object(args.request);
    const kind = str(operation?.kind) ?? str(request?.kind);
    const mediaLabels: Record<string, string> = { "image.generate": "图片素材", "image.edit": "图片修改", "video.generate": "视频片段", "audio.speech": "配音", "audio.transcribe": "音频转写" };
    if (kind && Object.hasOwn(mediaLabels, kind)) label = mediaLabels[kind];
  }
  if (tool === "read_source_content") {
    // chapterNumbers describes the source catalog, not the records delivered
    // by this page. Only Native's page projection may name returned chapters.
    const path = (sourceMatches ? result?.sectionPath : undefined) ?? sources.find(source => source.id === requestedSource)?.sectionPath;
    const returned = requestedSource && returnedSource === requestedSource && result?.contentRead === true ? result.returnedChapterNumbers : undefined;
    label = sourceLabel(path, returned) ?? "写作资料";
    if (args.view === "raw") label = "原始资料";
    label += sourceIndex ? "目录" : "片段";
  }
  const number = tool === "read_work_issues" ? undefined : tool === "read_revision_outcome" ? chapterNumber(result?.chapterNumber)
    : chapterNumber(draft?.chapterNumber) ?? chapterNumber(args.chapterNumber) ?? chapterNumber(result?.chapterNumber);
  if (tool === "read_delegated_draft") label = safeId(args.candidateId) ? "候选稿片段" : "候选稿目录";
  if (number && tool !== "read_source_content") label = tool === "save_draft_checkpoint" ? `第 ${number} 章正文`
    : tool === "save_delegated_draft" ? `第 ${number} 章候选稿` : `第 ${number} 章 · ${label}`;
  const presentation: OperationPresentation = { category, tool, object: label };
  if (tool === "read_source_content") presentation.contentScope = sourceIndex ? "index" : "page";
  if (tool === "read_delegated_draft") presentation.contentScope = safeId(args.candidateId) ? "page" : "index";
  if (sourceId) presentation.sourceId = sourceId;
  if (tool === "read_delegated_draft" && safeId(args.candidateId)) {
    presentation.sourceId = safeId(args.candidateId);
    if (args.cursor === undefined || Number.isSafeInteger(args.cursor) && Number(args.cursor) >= 0)
      presentation.retryKey = JSON.stringify([tool, args.candidateId, args.cursor ?? 0]);
  }
  if (safeId(draft?.chapterId)) presentation.targetId = safeId(draft?.chapterId);
  if (tool === "submit_revision_outcome" && safeId(args.writerTaskId)) presentation.reportWriterTaskId = safeId(args.writerTaskId);
  // Only an identical source page can establish recovery, never another page or a write.
  if ((tool === "read_source_content") && sourceId && sourceMatches
    && (args.cursor === undefined || typeof args.cursor === "string" && /^[a-f0-9]{64}:[0-9]{1,12}$/.test(args.cursor))) {
    presentation.retryKey = JSON.stringify([tool, sourceId, str(args.cursor) ?? null, args.view ?? "content",
      args.view === "workspace" ? [args.chapterNumber ?? null, args.objectKind ?? null, args.query ?? null] : args.view === "locate" ? [args.quote ?? null, args.groupId ?? null] : null]);
  }
  const directory = object(result?.taskSources);
  if (tool === "read_context" && Array.isArray(directory?.entries)) {
    presentation.sources = directory.entries.slice(0, 32).flatMap(entry => {
      const source = object(entry), id = safeId(source?.sourceId), label = sourceLabel(source?.sectionPath, source?.chapterNumbers);
      const sectionPath = str(source?.sectionPath);
      return id && label && sectionPath ? [{ id, label, sectionPath }] : [];
    });
  }
  const successful = item.status === "completed" && !item.error && item.success !== false && object(item.result)?.isError !== true;
  const sourceRead = tool === "read_source_content";
  const report = tool === "read_revision_outcome" ? object(result?.assessment) : null;
  const reportRead = tool !== "read_revision_outcome" || result?.authority === "candidateAnalysis"
    && safeId(args.writerTaskId) && report && report.writerTaskId === args.writerTaskId
    && safeId(result?.revisionId) && Array.isArray(report.outcomes) && report.outcomes.length > 0;
  const candidateRead = tool !== "read_delegated_draft" || result && safeId(args.candidateId)
    && result?.candidateId === args.candidateId && result.state === "isolated" && result.manuscriptChanged === false
    && typeof result.payloadPart === "string" && safeId(result.contentHash);
  const requestedIssue = str(args.issueId);
  const issueRead = tool !== "read_work_issues" || result && Array.isArray(result.entries) && result.entries.length > 0 && result.entries.length <= 16
    && Number.isSafeInteger(result.totalCount) && Number(result.totalCount) >= result.entries.length
    && (result.nextCursor === null || Number.isSafeInteger(result.nextCursor) && Number(result.nextCursor) >= 0)
    && (requestedIssue ? result.issueId === requestedIssue : result.issueId === undefined)
    && result.entries.every(entry => {
      const value = object(entry);
      return value && typeof value.issueId === "string" && /^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/.test(value.issueId)
        && (requestedIssue ? value.issueId === requestedIssue && str(value.observationId) && str(value.reason) : str(value.summary));
    });
  if (successful && category === "read" && result && sourceMatches && !sourceIndex && candidateRead
    && reportRead && issueRead && (!sourceRead || requestedSource && returnedSource === requestedSource && result.contentRead === true)) presentation.outcome = "read";
  if (successful && tool === "save_delegated_draft" && safeId(result?.candidateId)
    && result?.writeState === "committed" && result.state === "isolated" && result.manuscriptChanged === false) {
    presentation.outcome = "isolatedCandidate"; presentation.referenceId = safeId(result.candidateId);
  }
  if (successful && tool === "save_draft_checkpoint" && safeId(result?.chapterId) === safeId(draft?.chapterId)
    && safeId(result?.chapterId) && safeId(result?.checkpointId) && safeId(result?.revisionId)
    && Number.isSafeInteger(result?.version) && Number(result?.version) >= 0) {
    presentation.outcome = "saved"; presentation.referenceId = safeId(result?.revisionId);
  }
  const reportProof = tool === "submit_revision_outcome" ? object(result?.recoveryOperation) : null;
  if (successful && tool === "submit_revision_outcome" && item.type === "mcpToolCall" && item.server === "novel_workspace"
    && presentation.reportWriterTaskId && result?.writerTaskId === presentation.reportWriterTaskId
    && result?.status === "candidate" && safeId(result?.revisionId)
    && reportProof?.authority === "novelWorkspaceMcp" && reportProof.phase === "runtimeLedger"
    && reportProof.outcome === "committed" && safeId(reportProof.operationId)
    && safeId(item.id) && reportProof.callId === item.id && reportProof.toolName === tool
    && reportProof.receiptRef === `runtime:candidate:${result.revisionId}`) {
    presentation.outcome = "saved"; presentation.referenceId = safeId(result.revisionId);
  }
  if (successful && (tool === "save_media_document" || tool === "patch_media_document") && safeId(result?.revisionId)
    && object(result?.edition) && mediaDocument
    && result?.partialView === true && object(result?.documentSource)?.revisionId === result?.revisionId
    && (tool === "save_media_document" ? mediaDocument.kind === object(object(args.request)?.document)?.kind : Array.isArray(object(args.request)?.operations) && (object(args.request)?.operations as unknown[]).length > 0)
    && ["comic", "animation"].includes(str(mediaDocument.kind) ?? "")) {
    presentation.outcome = "saved"; presentation.referenceId = safeId(result?.revisionId);
  }
  const artifactKinds: Record<string, string> = { save_brief: "brief", save_outline: "outline", save_chapter_plans: "plan", save_property_rules: "propertyRules" };
  if (successful && Object.hasOwn(artifactKinds, tool) && result?.artifactKind === artifactKinds[tool]
    && safeId(result?.revisionId) && object(result?.payload) && Number.isSafeInteger(result?.version) && Number(result?.version) > 0) {
    presentation.outcome = "saved"; presentation.referenceId = safeId(result?.revisionId);
  }
  const pause = tool === "pause_generation_run" ? object(result?.receipt) : null;
  if (successful && pause?.authority === "committedGenerationControl" && pause.status === "paused"
    && safeId(args.runId) && pause.runId === args.runId && safeId(pause.sourceTaskId)
    && Number.isSafeInteger(args.expectedVersion) && Number(args.expectedVersion) >= 0
    && Number.isSafeInteger(pause.version) && pause.version === Number(args.expectedVersion) + 1) {
    presentation.outcome = "paused";
  }
  return presentation;
}

export function operationHeading(operation: OperationPresentation): string {
  if (operation.category === "diagnostic") return "内部操作";
  if (operation.outcome === "saved") return `已保存${operation.object}`;
  if (operation.outcome === "isolatedCandidate") return `已保存${operation.object}，待整合`;
  if (operation.outcome === "paused") return `已暂停${operation.object}`;
  if (operation.tool === "read_source_content") return `${operation.contentScope === "index" ? "查看" : "读取"}${operation.object}`;
  if (operation.tool === "read_delegated_draft") return `${operation.contentScope === "index" ? "查看" : "读取"}${operation.object}`;
  const title = toolTitle(operation.tool);
  if (title && operation.object.startsWith("第 ")) return `${title.match(ACTION_PREFIX)?.[0] ?? ""}${operation.object}`;
  return title ?? "处理创作内容";
}

import { validateAuthorIntent } from "../shared/author-input-budget.mjs";

export function chapterGenerationTask(projectId, request) {
  if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
  if (!request || typeof request !== "object" || Array.isArray(request)) throw new Error("章节生成参数无效");
  const { chapterId, chapterNumber, povCharacterId = null, storyTime = null, intent } = request;
  if (typeof chapterId !== "string" || !chapterId.trim() || /[\r\n]/.test(chapterId)) throw new Error("目标章节无效");
  if (!Number.isInteger(chapterNumber) || chapterNumber < 1) throw new Error("目标章节位置无效");
  if (povCharacterId !== null && (typeof povCharacterId !== "string" || !povCharacterId.trim())) throw new Error("视角人物无效");
  if (storyTime !== null && (!Number.isInteger(storyTime) || storyTime < 0)) throw new Error("故事时间无效");
  validateAuthorIntent(intent);
  return {
    prompt: `为当前选中的第 ${chapterNumber} 章生成完整候选草稿。${povCharacterId ? `限定视角人物为 ${povCharacterId}。` : ""}${storyTime !== null ? `限定故事时间位置为 ${storyTime}。` : ""}先按作者要求确定验收标准，再起草、独立检查并修订未满足项。\n\n作者意图：${intent.trim()}`,
    taskType: { operation: "generateChapter" },
    target: { type: "chapter", chapterId, chapterNumber, readingChapter: chapterNumber, sceneOrder: 1, scope: { type: "wholeChapter" }, ...(povCharacterId ? { povCharacterId } : {}), ...(storyTime !== null ? { storyTime } : {}) },
  };
}

export function planningGenerationTask(operation, intent) {
  if (!new Set(["generateBrief", "generateOutline", "generatePlan"]).has(operation)) throw new Error("规划生成 operation 无效");
  validateAuthorIntent(intent);
  return {
    prompt: operation === "generatePlan"
      ? `${intent.trim()}\n\n每个规划章节需要稳定 chapterPlanId；每个场景需要稳定 id、目标、节拍与非空 expectedEffects。每个预期效果包含稳定 id、dimension(world/knowledge/relationship/narrative) 和可在正文中核对的 description，以及 commitmentRefs 数组；未关联承诺时为空。引用独立承诺用 {type:commitment,commitmentId,revisionId}，引用提案用 {type:proposal,reportRevisionId,proposalId,sourceRevisionId}，身份从 Context Pack 原样复制。每项最多4条，整份计划最多64条，不引用未来、过期或已建立的旧提案。规划效果不是已发生事实。`
      : intent.trim(),
    taskType: { operation },
    target: { type: "artifact" },
  };
}

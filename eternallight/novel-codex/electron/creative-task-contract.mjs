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
    prompt: `为当前选中的第 ${chapterNumber} 章生成完整候选草稿。\n\n作者意图：${intent.trim()}`,
    taskType: { operation: "generateChapter" },
    target: { type: "chapter", chapterId, chapterNumber, readingChapter: chapterNumber, scope: { type: "wholeChapter" }, ...(povCharacterId ? { povCharacterId } : {}), ...(storyTime !== null ? { storyTime } : {}) },
  };
}

export function planningGenerationTask(operation, intent) {
  if (!new Set(["generateBrief", "generateOutline", "generatePlan"]).has(operation)) throw new Error("规划生成 operation 无效");
  validateAuthorIntent(intent);
  return {
    prompt: intent.trim(),
    taskType: { operation },
    target: { type: "artifact" },
  };
}

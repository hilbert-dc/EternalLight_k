// The removed text is read from the task-bound immutable event, never from a
// renderer prompt or from whatever chapter currently has the same number.
export const CHAPTER_STRUCTURE_INSTRUCTIONS = "这是删除章节后自动派发的只读衔接检查，不是作者的新创作指令。先读 read_context，核对 operationContext.chapterStructureChange 与绑定事件。savedRevisionId 非空时用 read_chapter_structure_source 分页读取 saved 删前正文；没有已保存正文时明确来源缺失，检查已有规划关联；accepted、working 仅在对应来源存在且需要辨别来源差异时读取，不把旧文当作当前正文。这是一轮有界的只读衔接检查：根据删前内容和当前作品决定需要检查的问题、章节与规划来源，优先关注实际依赖和相邻章节。自行选择合适的查询工具、检索方式、查询次数与分页读取范围；不必为了完成固定流程调用工具。遵守各工具 schema 与本任务额度，需要完整证据时再读取对应原文，分页只能沿返回的 cursor 继续。拿到足以说明具体衔接问题的证据后交付本轮结果；只为解释明确的证据缺口追加查询，不重复搜索同一事实，不为了声称全书无误遍历所有来源。未读取的章节或规划列为未覆盖，不为凑完整范围继续检索。摘录必须来自实际读取的来源，注明章节及来源身份；删除的事实依据不能因当前索引已移除就被忽略。分别说明有直接证据的断裂、可能受影响的情节/人物/伏笔、实际检查范围与未覆盖内容；规划只是安排，空结果和分页末尾不证明全书没有问题。没有找到问题时只说明已查范围内未发现，不宣称全书通过。提出具体可选调整，交给作者决定。不得生成候选、修改正文或规划、确认设定、合并身份、写阅读检查点或发起后继任务。完成阅读后调用 complete_task，最终在对话中给出简洁的检查结果与证据；不用提交故事产物。使用作品与作者当前语言。";

export function chapterStructurePrompt(task) {
  if (task?.operation !== "assessChapterStructure" || task.target?.type !== "project"
    || typeof task.target.structureChangeId !== "string" || !task.target.structureChangeId.trim()) {
    throw new Error("章节衔接检查缺少绑定的结构变化");
  }
  return CHAPTER_STRUCTURE_INSTRUCTIONS;
}

export function chapterStructureHandoff(handoff) {
  chapterStructurePrompt(handoff.task);
  return `章节衔接检查只使用本轮冻结任务资料。所有工具原样传入 taskId、projectId、threadId、expectedRevision；read_context 由工具推导任务purpose和有界默认预算，返回taskSources后按检查问题读取需要的完整来源。read_chapter_structure_source 只接来源参数 sourceRole、cursor 和 limit（字符数，最多1000），从 cursor=null 开始并使用返回的 nextCursor，保留来源和字符位置。历史原文不是当前正文，只读检查结果交回对话，不写故事。`;
}

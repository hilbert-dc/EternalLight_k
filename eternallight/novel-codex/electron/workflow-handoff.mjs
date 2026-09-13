export function createTaskBoundHandoff(projectId, workspace, task) {
  if (!task || typeof task !== "object" || !task.id || !task.threadId) throw new Error("Runtime 任务绑定无效");
  if (!workspace || workspace.revision !== task.inputRevision) throw new Error("任务输入版本与当前工作区不一致，请重新发起生成");
  return {
    task: {
      taskId: task.id,
      projectId,
      threadId: task.threadId,
      operation: task.operation ?? null,
      purpose: task.purpose,
      expectedRevision: task.inputRevision,
      requiredOutputKind: task.requiredOutputKind ?? null,
      target: task.target ?? null,
      inputManifest: task.inputManifest ?? {},
    },
  };
}

export function taskBoundInstructions(handoff) {
  return `Novel Runtime 任务绑定：\n${JSON.stringify(handoff, null, 2)}\n\n必须先调用 novel_workspace.read_context，并原样传入 taskId、projectId、threadId、expectedRevision，purpose 使用绑定中的 task.purpose，budget=24，byteBudget=60000；Writer 还必须从 task.target 原样传入 readingChapter、sceneOrder、povCharacterId 和 storyTime（后两项未指定时省略）。只可使用 read_context 返回的任务冻结上下文。第一次写入使用 inputManifest.expectedCandidateRevisionId 做候选 CAS，后续检查点使用上一次保存返回的 revisionId。每次逻辑写入生成一个 idempotencyKey，重试同一次写入时复用该键。写作任务只能保存到 target 指定章节和范围，最终检查点使用 completeness=chapterComplete。所有产物只能通过 novel_workspace MCP 保存为候选版本；结束前调用 complete_task，由 Runtime 验证已保存产物；不得声称候选已被作者采用。`;
}

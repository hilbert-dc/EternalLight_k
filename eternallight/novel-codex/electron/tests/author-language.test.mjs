import assert from "node:assert/strict";
import test from "node:test";
import { authorSteerInput } from "../author-steer.mjs";
import { LANGUAGE_CONSISTENCY_INSTRUCTIONS } from "../author-language.mjs";
import { composeNovelCodexPrompt } from "../codex-runtime.mjs";

for (const authorText of [
  "先读当前章节，再用中文说明结尾。",
  "Please use English for this discussion, including progress updates.",
  '正文写成英文，讨论保持中文。保留引文 "Tomorrow is already here" 和场景 ID scene-1。',
]) {
  test(`ordinary and task turns preserve the author request: ${authorText}`, () => {
    for (const taskInstructions of ["", '冻结任务身份：{"taskId":"task-1","expectedRevision":7}']) {
      const input = composeNovelCodexPrompt("冻结方法", authorText, taskInstructions);
      assert.equal(input.length, 1);
      assert.equal(input[0].text.split(LANGUAGE_CONSISTENCY_INSTRUCTIONS).length, 2);
      assert.ok(input[0].text.endsWith(`用户当前指令：\n${authorText}`));
      assert.ok(input[0].text.includes(taskInstructions));
    }
  });
}

test("a language-changing steer retains task ownership and the original author message", () => {
  const task = { id: "task-1", projectId: "project-1", threadId: "thread-1" };
  const intent = { taskId: task.id, status: "pending", intentRevision: 2,
    authorMessageId: "message-2", authorText: "接下来的进度与回复请用英文；scene-1 不变。" };
  const before = structuredClone({ task, intent });
  const input = authorSteerInput(task, intent);
  assert.equal(input[0].text.split(LANGUAGE_CONSISTENCY_INSTRUCTIONS).length, 2);
  assert.match(input[0].text, /taskId=task-1、projectId=project-1、threadId=thread-1/);
  assert.match(input[0].text, /expectedIntentRevision=2/);
  assert.ok(input[0].text.endsWith(`用户当前指令：\n${intent.authorText}`));
  assert.deepEqual({ task, intent }, before);
});

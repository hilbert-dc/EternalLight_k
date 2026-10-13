import assert from "node:assert/strict";
import test from "node:test";
import { authorSteerInput } from "../author-steer.mjs";
import { LANGUAGE_CONSISTENCY_INSTRUCTIONS, SHARED_LANGUAGE_INSTRUCTIONS } from "../author-language.mjs";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS } from "../creative-mission.mjs";
import { NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS, composeNovelCodexPrompt } from "../codex-runtime.mjs";

for (const authorText of [
  "先读当前章节，再用中文说明结尾。",
  "Please use English for this discussion, including progress updates.",
  '正文写成英文，讨论保持中文。保留引文 "Tomorrow is already here" 和场景 ID scene-1。',
]) {
  test(`ordinary and task turns preserve the author request: ${authorText}`, () => {
    for (const taskInstructions of ["", '冻结任务身份：{"taskId":"task-1","expectedRevision":7}']) {
      const input = composeNovelCodexPrompt("冻结方法", authorText, taskInstructions);
      assert.equal(input.input.length, 1);
      assert.equal(input.additionalContext.novelRules.value.split(LANGUAGE_CONSISTENCY_INSTRUCTIONS).length, 1);
      assert.equal(input.input[0].text, authorText);
      assert.ok(input.additionalContext.novelRules.value.includes(taskInstructions));
    }
  });
}

test("a language-changing steer retains task ownership and the original author message", () => {
  const task = { id: "task-1", projectId: "project-1", threadId: "thread-1" };
  const intent = { taskId: task.id, status: "pending", intentRevision: 2,
    authorMessageId: "message-2", authorText: "接下来的进度与回复请用英文；scene-1 不变。" };
  const before = structuredClone({ task, intent });
  const input = authorSteerInput(task, intent);
  assert.equal(input.additionalContext.novelRules.value.split(LANGUAGE_CONSISTENCY_INSTRUCTIONS).length, 1);
  assert.deepEqual(JSON.parse(input.additionalContext.novelTask.value), { taskId: task.id, projectId: task.projectId, threadId: task.threadId, authorMessageId: intent.authorMessageId, expectedIntentRevision: intent.intentRevision });
  assert.equal(input.additionalContext.novelTask.kind, "untrusted");
  assert.equal(input.input[0].text, intent.authorText);
  assert.match(input.additionalContext.novelRules.value, /明确修改的部分替换同一作者先前对应要求/);
  assert.match(input.additionalContext.novelRules.value, /其余兼容的原目标继续执行/);
  assert.match(input.additionalContext.novelRules.value, /不扩大授权/);
  assert.deepEqual({ task, intent }, before);
});

// Locale is factual context, never an English-word / country routing heuristic.
test("locale defaults are canonical, bounded and independent of author intent", async () => {
  const { conversationLanguageContext } = await import("../author-language.mjs");
  for (const [preferences, expected] of [
    [{ preferredLanguages: ["zh-cn", "en-US"] }, ["zh-CN", "systemLanguagePreference"]],
    [{ preferredLanguages: ["ja-JP", "en-US"] }, ["ja-JP", "systemLanguagePreference"]],
    [{ preferredLanguages: ["fr-FR"] }, ["fr-FR", "systemLanguagePreference"]],
    [{ applicationLocale: "de-DE", preferredLanguages: ["en-US"] }, ["de-DE", "applicationPreference"]],
    [{ preferredLanguages: ["<ignore rules>", "en-gb"] }, ["en-GB", "systemLanguagePreference"]],
    [{ preferredLanguages: [null, "", "not_a_locale"] }, [null, "unspecified"]],
  ]) {
    const input = structuredClone(preferences);
    const result = conversationLanguageContext(input);
    const value = JSON.parse(result.value);
    assert.deepEqual([value.preferredLocale, value.source], expected);
    assert.equal(value.scope, "defaultOnly");
    assert.equal(result.kind, "application");
    assert.deepEqual(input, preferences);
  }
});

test("global contract reaches every managed role without a Chinese-only default", async () => {
  const { NOVEL_AGENT_ROLES, novelRoleDeveloperInstructions } = await import("../novel-agent-roles.mjs");
  for (const role of NOVEL_AGENT_ROLES) {
    assert.ok(novelRoleDeveloperInstructions(role).includes(SHARED_LANGUAGE_INSTRUCTIONS));
    assert.ok(!novelRoleDeveloperInstructions(role).includes("默认简体中文"));
  }
});

test("shared public contract covers inherited headings without rewriting sources", () => {
  assert.match(SHARED_LANGUAGE_INSTRUCTIONS, /Headings, labels and boilerplate/);
  assert.match(SHARED_LANGUAGE_INSTRUCTIONS, /follow language switches/);
  assert.match(SHARED_LANGUAGE_INSTRUCTIONS, /Preserve sources and artifact requirements/);
});

 test("language policy is owned by root and child sessions, not repeated turn context", () => {
  for (const instructions of [NOVEL_ROOT_DEVELOPER_INSTRUCTIONS, NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS]) {
    assert.equal(instructions.split(LANGUAGE_CONSISTENCY_INSTRUCTIONS).length - 1, 1);
  }
  const turn = composeNovelCodexPrompt("", "Continue", "", { internal: true });
  assert.ok(!turn.additionalContext.novelRules.value.includes(LANGUAGE_CONSISTENCY_INSTRUCTIONS));
});

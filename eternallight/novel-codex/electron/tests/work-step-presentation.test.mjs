import assert from "node:assert/strict";
import test from "node:test";
import { build } from "esbuild";
import { PRODUCT_OPERATION_IDS } from "../operation-requirements.mjs";

const bundle = await build({ entryPoints: [new URL("../../src/work-step-presentation.ts", import.meta.url).pathname], bundle: true, platform: "node", format: "esm", write: false });
const { workStepName, recoveryRunName } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString("base64")}`);
const step = (id, operation, target = {}, extra = {}) => ({ id, operation, target, attempts: [], currentAttemptId: `${id}-attempt`, instruction: null, ...extra });
const decision = step("decision", "advisory", { workDecision: { workTaskId: "work", sourceTaskId: "writer-attempt" } });
const writer = step("writer", "generateChapter", { chapterNumber: 2 }, { attempts: [{ id: "writer-attempt" }] });
const action = (id, operation, number = 3) => step(id, operation, { chapterNumber: number,
  workDecisionAction: { workTaskId: "work", decisionStepId: "decision", decisionTaskId: "decision-attempt" } });
const work = (...steps) => ({ id: "work", goal: "核对灯税时间线", steps });
const name = (task, selected = decision) => workStepName(task, selected).label;

test("coordinator names use registered actions with exact decision and attempt identity", () => {
  assert.equal(name(work(writer, decision, action("next", "generateChapter"))), "安排第 3 章写作");
  assert.equal(name(work(writer, decision, action("scene", "assessSceneOutcome", 2), action("knowledge", "modelKnowledge", 2))), "安排第 2 章场景检查、第 2 章设定分析");
  for (const key of ["workTaskId", "decisionStepId", "decisionTaskId"]) {
    const foreign = action("foreign", "generateChapter");
    foreign.target.workDecisionAction[key] = "other";
    assert.equal(name(work(writer, decision, foreign)), "核对第 2 章写作结果与后续安排");
  }
});

test("unselected coordination uses its exact source, never adjacent work or inferred next chapter", () => {
  const unrelated = step("unrelated", "modelKnowledge", { chapterNumber: 8 }, { attempts: [{ id: "unrelated-attempt" }] });
  assert.equal(name(work(writer, unrelated, decision)), "核对第 2 章写作结果与后续安排");
  const missing = { ...decision, target: { workDecision: { workTaskId: "work", sourceTaskId: "missing" } } };
  assert.equal(name(work(writer, missing), missing), "核对任务进展与后续安排");
  assert.equal(name(work(step("writer", "generateChapter", { chapterNumber: "2" }, { attempts: [{ id: "writer-attempt" }] }), decision)), "核对写作结果与后续安排");
  for (const status of ["running", "blocked", "cancelled", "completed"]) {
    assert.equal(name(work(writer, { ...decision, status }), { ...decision, status }), "核对第 2 章写作结果与后续安排");
  }
});

test("ordinary discussions retain specific requirements and readable complete text", () => {
  const discussion = step("discussion", "advisory", {}, { instruction: "核对第7—15章规划\n与灯税时间线" });
  assert.equal(name(work(discussion), discussion), "讨论：核对第7—15章规划 与灯税时间线");
  const initial = step("initial", "advisory");
  assert.equal(name(work(initial), initial), "讨论：核对灯税时间线");
  const long = { ...discussion, instruction: "😀修改人物动机".repeat(30) };
  const presentation = workStepName(work(long), long);
  assert.equal(Array.from(presentation.label).length, 61);
  assert.equal(presentation.fullLabel, `讨论：${long.instruction}`);
  assert.ok(presentation.label.endsWith("…"));
});

test("operation names and chapter scope remain separate from delivery success", () => {
  assert.equal(name(work(writer), writer), "第 2 章 · 生成章节");
  const unknown = step("unknown", "futureOperation", { chapterNumber: -1 });
  assert.equal(name(work(unknown), unknown), "处理创作内容");
  assert.equal(name(work({ ...unknown, operation: "__proto__" }), { ...unknown, operation: "__proto__" }), "处理创作内容");
});

test("all current operations and the next-chapter queue have meaningful names", () => {
  for (const operation of [...PRODUCT_OPERATION_IDS, "writeNextChapter"]) {
    const current = step("current", operation);
    assert.notEqual(name(work(current), current), "处理创作内容", `Missing name for ${operation}`);
  }
  const next = action("next", "writeNextChapter");
  assert.equal(name(work(decision, next)), "安排第 3 章写作");
  assert.equal(name(work(next), next), "第 3 章 · 续写章节");
  assert.equal(name(work({ ...next, operation: "generateChapter" }), { ...next, operation: "generateChapter" }), "第 3 章 · 生成章节");
});

test("actual writing boundaries, planning starts and review seats distinguish repeated operations", () => {
  const writing = target => step("write", "generateChapter", { chapterNumber: 2, ...target });
  const selected = writing({ scope: { type: "selectedScenes", sourceRevisionId: "revision", groupIds: ["group-a", "group-b"] } });
  assert.equal(name(work(selected), selected), "第 2 章 · 修改选定场景");
  const through = writing({ sceneOrder: 3, scope: { type: "throughScene", endSceneOrder: 3 } });
  assert.equal(name(work(through), through), "第 2 章 · 写作至第 3 个场景");
  const whole = writing({ scope: { type: "wholeChapter" } });
  assert.equal(name(work(whole), whole), "第 2 章 · 整章写作");
  for (const scope of [{ type: "selectedScenes", groupIds: ["a"] },
    { type: "selectedScenes", sourceRevisionId: "r", groupIds: ["a", "a"] },
    { type: "selectedScenes", sourceRevisionId: "r", groupIds: [] },
    { type: "selectedScenes", sourceRevisionId: "r", groupIds: Array.from({ length: 17 }, (_, i) => String(i)) },
    { type: "throughScene", endSceneOrder: 101 }, { type: "throughScene", endSceneOrder: "3" }, { type: "throughScene", endSceneOrder: 2 }]) {
    const invalid = writing({ sceneOrder: 3, scope });
    assert.equal(name(work(invalid), invalid), "第 2 章 · 生成章节");
  }
  for (const [role, label] of [["structure", "结构与因果"], ["character", "人物与主题"], ["experience", "阅读体验"]]) {
    const review = step("review", "reviewPlanningSeat", { role, artifactKind: "outline" });
    assert.equal(name(work(review), review), `大纲 · ${label}检查`);
  }
  const plan = step("plan", "generatePlan", { planFromChapter: 7 });
  assert.equal(name(work(plan), plan), "从第 7 章起生成近章规划");
  const focused = step("focused", "advisory", { conversationFocus: { chapterNumber: 8 } });
  assert.equal(name(work(focused), focused), "第 8 章 · 讨论：核对灯税时间线");
});

test("recovery borrows only uniquely owned current attempt names", () => {
  const boundStep = { ...writer, currentAttemptId: "writer-attempt", attempts: [{ id: "writer-attempt", threadId: "thread" }] };
  const boundWork = { ...work(boundStep), projectId: "A", threadId: "thread" };
  const run = { runId: "writer-attempt", projectId: "A", threadId: "thread", operation: "generateChapter" };
  assert.equal(recoveryRunName(run, [boundWork]).label, "第 2 章 · 生成章节");
  for (const foreign of [{ ...run, projectId: "B" }, { ...run, threadId: "other" }, { ...run, runId: "old-attempt" }]) {
    assert.equal(recoveryRunName(foreign, [boundWork]).label, "生成章节");
  }
  assert.equal(recoveryRunName(run, [boundWork, { ...boundWork, id: "duplicate" }]).label, "生成章节");
  assert.equal(recoveryRunName(run, [{ ...boundWork, steps: [{ ...boundStep, currentAttemptId: "new-attempt" }] }]).label, "生成章节");
  assert.equal(recoveryRunName(run, [{ ...boundWork, steps: [{ ...boundStep, attempts: [{ id: "writer-attempt", threadId: "other" }] }] }]).label, "生成章节");
  assert.equal(recoveryRunName({ ...run, operation: "assessChapterStructure" }, []).label, "检查删章后的故事衔接");
});

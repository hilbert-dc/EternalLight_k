import assert from "node:assert/strict";
import test from "node:test";

import { firstOutlineEditorError, formatFactValue, generationStatusLabel, outlinePathLabels, parseFactValue, rebaseEditorBaseline, selectionAwareReviewChecksPass, valueForIdentity } from "../../src/creative-product-logic.ts";

test("builds full outline paths for planning targets", () => {
  const paths = outlinePathLabels([
    { id: "book", kind: "book", parentId: null, position: 0, title: "主线", purpose: "", summary: "", dependencyKeys: [], certainty: "open", protection: "free" },
    { id: "volume", kind: "volume", parentId: "book", position: 0, title: "第一卷", purpose: "", summary: "", dependencyKeys: [], certainty: "open", protection: "free" },
    { id: "unit", kind: "unit", parentId: "volume", position: 0, title: "失踪", purpose: "", summary: "", dependencyKeys: [], certainty: "open", protection: "free" },
  ]);
  assert.equal(paths.get("unit"), "全书 · 主线 → 卷 · 第一卷 → 单元 · 失踪");
});

test("returns the first accessible outline field error", () => {
  assert.deepEqual(firstOutlineEditorError([
    { clientKey: "root", kind: "book", position: 0, title: "", purpose: "", summary: "", dependencyKeys: [], certainty: "open", protection: "free" },
  ]), { nodeId: "root", message: "请先填写这个大纲节点的标题。" });
});

test("zero-output terminal generation is failure and partial requires output", () => {
  assert.deepEqual(generationStatusLabel({ status: "completed", deliveryStatus: "ready_for_review", outputRefs: [{ kind: "draft", objectId: "draft-chapter-a", revisionId: "draft-1" }] }), { label: "内容已保存", failed: false });
  assert.deepEqual(generationStatusLabel({ status: "failed", deliveryStatus: "pending", outputRefs: [] }), { label: "生成失败", failed: true });
  assert.deepEqual(generationStatusLabel({ status: "failed", deliveryStatus: "incomplete", outputRefs: [{ kind: "draft", objectId: "draft-chapter-a", revisionId: "draft-1" }] }), { label: "生成部分完成", failed: false });
  assert.deepEqual(generationStatusLabel({ status: "cancelled", deliveryStatus: "pending", outputRefs: [] }), { label: "生成已停止", failed: false });
  assert.deepEqual(generationStatusLabel({ status: "cancelled", deliveryStatus: "incomplete", outputRefs: [{ kind: "draft", objectId: "draft-chapter-a", revisionId: "draft-1" }] }), { label: "已停止，部分稿已保存", failed: false });
});

test("writer preview is visible only for the chapter request that produced it", () => {
  const preview = { planningContext: { chapter: { publicTitle: "第一章" } } };
  assert.equal(valueForIdentity({ identity: "chapter-a", value: preview }, "chapter-b"), null);
  assert.equal(valueForIdentity({ identity: "chapter-a", value: preview }, "chapter-a"), preview);
});

test("rebasing an editor session replaces both the revision and displayed baseline", () => {
  assert.deepEqual(rebaseEditorBaseline(
    { identity: "draft:1", baseProjectRevision: 3, baseCandidateRevisionId: "draft-old", baseContent: "旧候选" },
    { baseProjectRevision: 5, baseCandidateRevisionId: "draft-latest", baseContent: "最新候选" },
  ), { identity: "draft:1", baseProjectRevision: 5, baseCandidateRevisionId: "draft-latest", baseContent: "最新候选" });
});

test("fact value parsing preserves false and structured values for review", () => {
  assert.equal(parseFactValue("false"), false);
  assert.deepEqual(parseFactValue('{"certainty":0.4}'), { certainty: 0.4 });
  assert.equal(formatFactValue(false), "false");
  assert.throws(() => parseFactValue("不是 JSON"), /有效 JSON/);
});

test("rejected or unselected invalid facts do not keep a draft review blocked", () => {
  const checks = [
    { id: "current-draft", passed: true, blocking: true },
    { id: "fact-evidence-anchor", passed: false, blocking: true },
  ];
  const groups = [{ id: "scene-a", body: "门已经关闭。" }];
  const invalidFact = { id: "fact-bad", sceneGroupId: "scene-a", factKind: "world", subjectId: null, predicate: "door-open", value: true, epistemicOwnerId: null, epistemicState: "known", evidence: "门仍然开着。", source: "agent", reviewDecision: "candidate", readingChapter: 1, sceneOrder: 1 };
  const validFact = { ...invalidFact, id: "fact-good", predicate: "door-closed", value: false, evidence: "门已经关闭。" };
  assert.equal(selectionAwareReviewChecksPass(checks, [], groups), true);
  assert.equal(selectionAwareReviewChecksPass(checks, [validFact], groups), true);
  assert.equal(selectionAwareReviewChecksPass(checks, [invalidFact], groups), false);
});

import test from "node:test";
import assert from "node:assert/strict";
import { registerChapterDeletion } from "../chapter-deletion.mjs";

test("chapter deletion IPC validates exact frozen input and forwards one author request", async () => {
  let handler; const calls = [];
  registerChapterDeletion({ handle(channel, fn) { assert.equal(channel, "runtime:delete-chapter"); handler = fn; } }, async (...input) => { calls.push(input); return { operation: "delete-chapter" }; });
  const input = { chapterId: "chapter/一", expectedRevision: 9, idempotencyKey: "frozen-key" };
  assert.deepEqual(await handler({}, "project/一", input), { operation: "delete-chapter" });
  assert.deepEqual(calls, [["/api/projects/project%2F%E4%B8%80/chapters/chapter%2F%E4%B8%80/delete", { method: "POST", body: JSON.stringify({ expectedRevision: 9, idempotencyKey: "frozen-key" }) }]]);
  for (const bad of [null, [], { ...input, expectedRevision: -1 }, { ...input, expectedRevision: 1.5 }, { ...input, chapterId: "中".repeat(54) }, { ...input, chapterId: " chapter" }, { ...input, idempotencyKey: "bad\nkey" }, { ...input, publish: true }, { chapterId: input.chapterId, expectedRevision: 9 }]) assert.throws(() => handler({}, "project", bad), /删除章节请求无效/);
  assert.throws(() => handler({}, "", input), /删除章节请求无效/);
  assert.equal(calls.length, 1);
});

import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";

const revision = (body, createdAt = null, sceneGroups = []) => ({ body, createdAt, sceneGroups });
const chapter = (id, number, candidate = null, accepted = null) => ({ id, number, candidate, accepted, working: null });

test("overview counts only unique current saved prose and keeps publication independent", async t => {
  const directory = await mkdtemp(join(tmpdir(), "work-overview-"));
  try {
    const path = join(directory, "overview.mjs");
    await build({ entryPoints: [fileURLToPath(new URL("../../src/work-overview.ts", import.meta.url))],
      outfile: path, bundle: true, platform: "node", format: "esm" });
    const { summarizeWorkOverview } = await import(pathToFileURL(path).href);
    await t.test("candidate wins, scenes exclude separators, Unicode punctuation counts once", () => {
      const current = revision("字 🌊\n\n---\n\n。", "2026-10-04T01:00:00Z", [{ body: " 字 🌊\n" }, { body: "。\t" }]);
      const published = revision("此前正文不叠加。", "2026-10-03T00:00:00Z");
      const second = chapter("two", 2, revision("\n AB！ ", "2026-10-04T02:00:00Z"));
      const first = { ...chapter("one", 1, current, published), working: revision("旧工作依据".repeat(100)) };
      const workspace = { chapters: [second, first, structuredClone(first), chapter("empty", 3, revision(" \n\t"))],
        artifacts: { plan: { candidate: { payload: { chapters: [1, 2, 3, 4, 5] } } } } };
      const before = structuredClone(workspace);
      assert.deepEqual(summarizeWorkOverview(workspace), { writtenChapters: 2, characters: 6, publishedChapters: 1,
        lastEditedAt: "2026-10-04T02:00:00.000Z", continueChapterId: "two", continueChapterNumber: 2 });
      assert.deepEqual(workspace, before);
    });
    await t.test("same accepted and candidate revision is counted once, accepted-only remains visible", () => {
      const saved = revision("正文🌙", "2026-10-04T00:00:00Z");
      assert.deepEqual(summarizeWorkOverview({ chapters: [chapter("one", 1, saved, saved), chapter("two", 2, null, saved)] }),
        { writtenChapters: 2, characters: 6, publishedChapters: 2, lastEditedAt: "2026-10-04T00:00:00.000Z", continueChapterId: "two", continueChapterNumber: 2 });
    });
    await t.test("missing or invalid times never claim a most recently edited chapter", () => {
      for (const time of [null, "not-a-time"]) assert.deepEqual(summarizeWorkOverview({ chapters: [
        chapter("one", 1, revision("字", "2026-10-04T00:00:00Z")), chapter("two", 2, revision("文", time))] }),
        { writtenChapters: 2, characters: 2, publishedChapters: 0, lastEditedAt: null, continueChapterId: "two", continueChapterNumber: 2 });
    });
    await t.test("empty chapter metadata is navigation, not written or published content", () => {
      assert.deepEqual(summarizeWorkOverview({ chapters: [chapter("later", 3), chapter("first", 1, revision(""), revision(" \n"))] }),
        { writtenChapters: 0, characters: 0, publishedChapters: 0, lastEditedAt: null, continueChapterId: "first", continueChapterNumber: 1 });
      assert.deepEqual(summarizeWorkOverview({ chapters: [] }),
        { writtenChapters: 0, characters: 0, publishedChapters: 0, lastEditedAt: null, continueChapterId: null, continueChapterNumber: null });
      assert.deepEqual(summarizeWorkOverview({ chapters: [chapter("one", 1, revision(""), revision("仍已发布。"))] }),
        { writtenChapters: 0, characters: 0, publishedChapters: 1, lastEditedAt: null, continueChapterId: "one", continueChapterNumber: 1 });
    });
  } finally { await rm(directory, { recursive: true, force: true }); }
});

import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";

test("unified source views cannot falsely recover another representation or query", async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-tool-presentation-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const output = join(directory, "presentation.mjs");
  await build({ entryPoints: [new URL("../../src/operation-presentation.ts", import.meta.url).pathname], outfile: output, bundle: true, format: "esm", platform: "node" });
  const { operationPresentation: present } = await import(pathToFileURL(output).href);
  const read = argumentsValue => present({ tool: "read_source_content", status: "completed", arguments: { sourceId: "source", ...argumentsValue },
    result: { structuredContent: { sourceId: "source", contentRead: true } } });
  assert.equal(read({}).retryKey, read({ view: "content" }).retryKey);
  const raw = read({ view: "raw" });
  assert.equal(raw.object, "原始资料片段");
  assert.notEqual(raw.retryKey, read({}).retryKey);
  const directoryView = read({ view: "workspace", chapterNumber: 2 });
  assert.equal(directoryView.contentScope, "index");
  assert.equal(directoryView.outcome, undefined, "a directory is never reading evidence");
  assert.notEqual(directoryView.retryKey, read({ view: "workspace", chapterNumber: 3 }).retryKey);
  assert.notEqual(read({ view: "workspace", query: "雨" }).retryKey, read({ view: "workspace", query: "风" }).retryKey);
  assert.notEqual(read({ view: "locate", quote: "雨" }).retryKey, read({ view: "locate", quote: "风" }).retryKey);
  assert.notEqual(read({ view: "locate", quote: "雨", groupId: "a" }).retryKey, read({ view: "locate", quote: "雨", groupId: "b" }).retryKey);
  assert.equal(present({ tool: "read_tool_guide", status: "completed" }).category, "diagnostic");
  const patch = { tool: "save_draft_checkpoint", status: "completed", arguments: { patch: { chapterId: "chapter" } },
    result: { structuredContent: { chapterId: "chapter", checkpointId: "checkpoint", revisionId: "revision", version: 1 } } };
  assert.equal(present(patch).outcome, "saved");
  assert.equal(present(patch).targetId, "chapter");
  assert.equal(present({ ...patch, arguments: { patch: { chapterId: "other" } } }).outcome, undefined);
});

import assert from "node:assert/strict";
import test from "node:test";
import { build } from "esbuild";
const bundle = await build({ entryPoints: [new URL("../../src/conversation-retry.ts", import.meta.url).pathname], bundle: true, platform: "node", format: "esm", write: false });
const { conversationRetryTask: select, conversationRetryCandidates: choices } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString("base64")}`);
const task = (id, extra = {}) => ({ id, projectId: "A", threadId: "one", status: "paused", resumeMode: "continueExecution", resumeBlockReason: null, steps: [], ...extra });

test("unknown Native usage cannot be bypassed with a conversation retry", () => {
  for (const status of ["running", "paused", "blocked"]) {
    const blocked = task("unknown", { status, resumeMode: "continueExecution", resumeBlockReason: "unknownUsage" });
    assert.equal(select([blocked], "A", "one", null), null);
    assert.deepEqual(choices([blocked], "A", "one"), []);
  }
});

test("only Native saved-result recovery remains available without a new provider grant", () => {
  for (const status of ["paused", "blocked"]) {
    const recovery = task("saved", { status, resumeMode: "recoverSavedResult", resumeBlockReason: "unknownUsage" });
    assert.equal(select([recovery], "A", "one", null), recovery);
    assert.deepEqual(choices([recovery], "A", "one"), [recovery]);
    for (const reason of ["pendingRecovery", "inputChanged", "partialOutput"]) {
      const unsafe = { ...recovery, resumeBlockReason: reason };
      assert.equal(select([unsafe], "A", "one", null), null);
      assert.deepEqual(choices([unsafe], "A", "one"), []);
    }
  }
});

test("composer retry selects only an unambiguous task in the current conversation", () => {
  const current = task("current"), foreign = task("foreign", { threadId: "two" });
  assert.equal(select([current, foreign], "A", "one", null), current);
  assert.equal(select([foreign], "A", "one", null), null);
  assert.equal(select([current], "B", "one", null), null);
  assert.equal(select([current], "A", null, null), null);
  assert.equal(select([current, task("another")], "A", "one", null), null);
  assert.equal(select([task("blocked", { resumeBlockReason: "inputChanged" })], "A", "one", null), null);
  const unstarted = task("unstarted", { threadId: "" });
  assert.equal(select([current, unstarted], "A", null, null), unstarted);
});

test("exact current attempt takes precedence without resuming an earlier task after completion", () => {
  const attempt = { id: "attempt", threadId: "one", turnId: "turn" };
  const run = { taskId: "attempt", threadId: "one", turnId: "turn" };
  const bound = task("bound", { steps: [{ attempts: [attempt] }] });
  assert.equal(select([task("old")], "A", "one", run), null);
  assert.equal(select([task("old"), bound], "A", "one", run), bound);
  assert.equal(select([task("old"), { ...bound, status: "completed" }], "A", "one", run), null);
  const handoff = { ...bound, status: "completed", pendingContinuation: true };
  assert.equal(select([handoff], "A", "one", run), handoff);
});


test("explicit retry choices retain owned Native-ready tasks without choosing or guessing a mode", () => {
  const original = task("original", { resumeMode: "continueExecution" });
  const revision = task("revision", { resumeMode: "recoverSavedResult" });
  assert.deepEqual(choices([original, revision], "A", "one"), [original, revision]);
  assert.equal(select([original, revision], "A", "one", null), null);
  assert.deepEqual(choices([original, task("wrong-thread", { threadId: "two", resumeMode: "continueExecution" })], "A", "one"), [original]);
  assert.deepEqual(choices([original], "B", "one"), []);
  assert.deepEqual(choices([original], "A", "two"), []);
  for (const extra of [{ resumeMode: null }, { resumeMode: "guess" }, { resumeMode: "continueExecution", resumeBlockReason: "pendingRecovery" },
    { resumeMode: "continueExecution", resumeBlockReason: "inputChanged" }, { resumeMode: "continueExecution", status: "completed" }]) {
    const blocked = task("blocked", extra);
    assert.deepEqual(choices([blocked], "A", "one"), []);
    assert.equal(select([blocked], "A", "one", null), null);
  }
  assert.deepEqual(choices([original, { ...original }], "A", "one"), []);
});

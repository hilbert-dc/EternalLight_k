import assert from "node:assert/strict";
import { after, test } from "node:test";
import { createRequire } from "node:module";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { build } from "esbuild";

const directory = await mkdtemp(join(tmpdir(), "novel-planning-submission-"));
const output = join(directory, "hook.cjs");
// This scheduler exercises synchronous refs and delayed promises; actual React commits
// and the save-handler integration are covered by the planning renderer journey.
await build({ stdin: { contents: 'export { usePlanningSubmission } from "./src/usePlanningSubmission"; export { reset, begin, unmount } from "react";', resolveDir: process.cwd() }, outfile: output, bundle: true,
  platform: "node", format: "cjs", plugins: [{ name: "hook-scheduler", setup(builder) {
    builder.onResolve({ filter: /^react$/ }, () => ({ path: "react", namespace: "scheduler" }));
    builder.onLoad({ filter: /.*/, namespace: "scheduler" }, () => ({ contents: `
      const slots = []; let cursor = 0; let cleanup;
      export const useRef = value => slots[cursor++] ??= { current: value };
      export const useState = value => [value, () => {}];
      export function useEffect(effect) { if (!cleanup) cleanup = effect(); }
      export function reset() { slots.length = 0; cursor = 0; cleanup = undefined; }
      export function begin() { cursor = 0; }
      export function unmount() { cleanup?.(); }
    ` }));
  } }] });
const { usePlanningSubmission, ...scheduler } = createRequire(import.meta.url)(output);
after(() => rm(directory, { recursive: true, force: true }));

function setup() {
  scheduler.reset();
  const api = {};
  let options = { api, projectId: "A", view: "brief", eligible: true,
    composer: "讨论当前设定", sourceIdentity: null, canSendAfterSave: () => true };
  let hook;
  const render = changes => {
    options = { ...options, ...changes };
    scheduler.begin();
    hook = usePlanningSubmission(options);
    return hook;
  };
  return { render, api, get hook() { return hook; } };
}
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

test("one save stays pending through successful preflight until explicit send binding", async () => {
  const context = setup(), hook = context.render(), saved = deferred();
  let calls = 0;
  const first = hook.prepare(() => { calls++; return saved.promise; });
  assert.equal(hook.isPending(), true);
  assert.equal(await hook.prepare(async () => { calls++; return true; }), false);
  saved.resolve(true);
  assert.equal(await first, true);
  assert.equal(context.render().pending, true);
  assert.equal(calls, 1);
  context.hook.finish();
  assert.equal(context.hook.isPending(), false);
});

test("input ABA synchronously invalidates sending while the existing save remains single-flight", async () => {
  const context = setup(), hook = context.render(), saved = deferred();
  const first = hook.prepare(() => saved.promise);
  hook.invalidate();
  context.render({ composer: "新的问题" });
  context.hook.invalidate();
  context.render({ composer: "讨论当前设定" });
  let duplicate = 0;
  assert.equal(await context.hook.prepare(async () => { duplicate++; return true; }), false);
  saved.resolve(true);
  assert.equal(await first, false);
  assert.equal(duplicate, 0);
  assert.equal(context.hook.isPending(), false);
});

test("scope ABA and changed source reject old receipts without releasing a newer owner", async () => {
  for (const change of [{ projectId: "B" }, { api: {} }, { view: "outline" }, { sourceIdentity: "source-B" }]) {
    const context = setup(), old = deferred(), current = deferred();
    const first = context.render().prepare(() => old.promise);
    context.render(change);
    context.render({ api: context.api, projectId: "A", view: "brief", sourceIdentity: null });
    if ("sourceIdentity" in change) {
      old.resolve(true);
      assert.equal(await first, false);
      continue;
    }
    const second = context.hook.prepare(() => current.promise);
    old.resolve(true);
    assert.equal(await first, false);
    assert.equal(context.hook.isPending(), true);
    current.resolve(true);
    assert.equal(await second, true);
    context.hook.finish();
  }
});

test("failed save, uncleared dirty state and newly blocked state never authorize a send", async () => {
  for (const saveResult of [false, true]) {
    const context = setup(), saved = deferred();
    const pending = context.render().prepare(() => saved.promise);
    context.render({ canSendAfterSave: () => false });
    saved.resolve(saveResult);
    assert.equal(await pending, false);
    assert.equal(context.hook.isPending(), false);
  }
});

test("current failure propagates but stale failure and unmount only cancel preparation", async () => {
  const context = setup(), failure = new Error("回读失败");
  await assert.rejects(context.render().prepare(async () => { throw failure; }), reason => reason === failure);
  assert.equal(context.hook.isPending(), false);
  const stale = deferred(), first = context.hook.prepare(() => stale.promise);
  context.render({ projectId: "B" });
  stale.reject(failure);
  assert.equal(await first, false);
  const stopped = deferred(), second = context.hook.prepare(() => stopped.promise);
  scheduler.unmount();
  stopped.resolve(true);
  assert.equal(await second, false);
});

test("ineligible pages and absent API never invoke the save handler", async () => {
  for (const options of [{ eligible: false }, { api: undefined }, { projectId: "" }]) {
    const context = setup();
    let saves = 0;
    assert.equal(await context.render(options).prepare(async () => { saves++; return true; }), false);
    assert.equal(saves, 0);
  }
});

test("detached callbacks cannot save or finish a new scope's submission", async () => {
  const context = setup(), original = context.render();
  context.render({ projectId: "B" });
  let staleSaves = 0;
  assert.equal(await original.prepare(async () => { staleSaves++; return true; }), false);
  assert.equal(staleSaves, 0);
  const receipt = deferred(), pending = context.hook.prepare(() => receipt.promise);
  original.finish();
  assert.equal(context.hook.isPending(), true);
  receipt.resolve(true);
  assert.equal(await pending, true);
  context.hook.finish();
});

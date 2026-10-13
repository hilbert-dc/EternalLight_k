import test from "node:test";
import assert from "node:assert/strict";
import { changedPaths, verificationPlan } from "../verify-affected.mjs";

test("Core changes retain every native dependent and critical cross-layer controls", () => {
  const plan = verificationPlan(["eternallight/novel-codex/native/story-core/src/task/tasks.rs"]);
  for (const check of ["test:native", "test:task-controls", "test:creative-journey", "check:runtime-contracts", "check:executor-tools"]) assert.ok(plan.includes(check), check);
  assert.equal(new Set(plan).size, plan.length);
});
test("Shared changes include Rust, Host and Renderer", () => {
  const plan = verificationPlan(["eternallight/novel-codex/shared/generated/runtime/Dto.ts"]);
  for (const check of ["test:native", "test:agent", "test:ui", "build"]) assert.ok(plan.includes(check), check);
});
test("Renderer edits avoid unrelated native suite but preserve author controls", () => {
  const plan = verificationPlan(["src/App.tsx"]);
  assert.ok(plan.includes("test:ui")); assert.ok(plan.includes("test:task-controls"));
  assert.ok(!plan.includes("test:native"));
});
test("Build changes and unknown boundaries fail closed to complete verification", () => {
  for (const path of ["package.json", "scripts/new.mjs", "new-service/entry.mjs", "docs/service.mjs", "codex-runtime/agents/default.toml"]) assert.deepEqual(verificationPlan([path]), ["verify"]);
});
test("Documentation avoids compiler runs; mixed edits retain code coverage", () => {
  assert.deepEqual(verificationPlan(["README.md"]), ["check:document-archive"]);
  assert.ok(verificationPlan(["README.md", "electron/main.mjs"]).includes("test:agent"));
});


test("untracked files are detected from a nested product cwd using repository paths", async () => {
  const { mkdtemp, mkdir, writeFile, rm } = await import("node:fs/promises");
  const { tmpdir } = await import("node:os");
  const path = await import("node:path");
  const { execFileSync } = await import("node:child_process");
  const repository = await mkdtemp(path.join(tmpdir(), "affected-paths-"));
  try {
    const git = (...args) => execFileSync("git", args, { cwd: repository, stdio: "pipe" });
    git("init");
    await writeFile(path.join(repository, "README.md"), "fixture");
    git("add", "README.md");
    git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-m", "fixture");
    const product = path.join(repository, "eternallight/novel-codex");
    await mkdir(path.join(product, "src"), { recursive: true });
    await writeFile(path.join(product, "src/new.ts"), "export const value = 1;");
    assert.deepEqual(changedPaths("HEAD", product), ["eternallight/novel-codex/src/new.ts"]);
    assert.ok(verificationPlan(changedPaths("HEAD", product)).includes("test:ui"));
  } finally { await rm(repository, { recursive: true, force: true }); }
});

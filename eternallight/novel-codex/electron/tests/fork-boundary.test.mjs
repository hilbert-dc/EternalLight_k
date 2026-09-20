import assert from "node:assert/strict";
import test from "node:test";

import { assessRemoteRoles, disallowedProductChanges, isOfficialCodexRemote, normalizeRemoteUrl, parsePatchRegistry } from "../../scripts/verify-fork-boundary.mjs";
import { synchronizationIdentity } from "../../scripts/sync-upstream.mjs";

test("recognizes canonical HTTPS and SSH Codex upstream remotes", () => {
  assert.equal(isOfficialCodexRemote("https://github.com/openai/codex.git"), true);
  assert.equal(isOfficialCodexRemote("git@github.com:openai/codex.git"), true);
  assert.equal(isOfficialCodexRemote("https://github.com/example/codex.git"), false);
  assert.equal(normalizeRemoteUrl("https://github.com/openai/codex.git"), "https://github.com/openai/codex");
});

test("requires separate origin and upstream roles for a release-ready fork", () => {
  assert.deepEqual(assessRemoteRoles({ origin: "git@github.com:eternallight/novel-codex.git", upstream: "https://github.com/openai/codex.git" }), {
    origin: "git@github.com:eternallight/novel-codex.git",
    upstream: "https://github.com/openai/codex.git",
    sourceRemote: "upstream",
    strictReady: true,
    warning: null,
  });
  assert.equal(assessRemoteRoles({ origin: "https://github.com/openai/codex.git" }).strictReady, false);
});

test("keeps product divergence out of upstream Codex implementation paths", () => {
  assert.deepEqual(disallowedProductChanges([
    "eternallight/novel-codex/src/App.tsx",
    ".github/workflows/novel-codex-ci.yml",
  ]), []);
  assert.deepEqual(disallowedProductChanges(["codex-rs/core/src/codex.rs"]), ["codex-rs/core/src/codex.rs"]);
});

test("requires registered upstream patches to identify exact paths, symbols, tests, baseline and diff", () => {
  const registry = parsePatchRegistry({ version: 1, patches: [{
    id: "patch-v1",
    status: "active",
    upstreamBaseline: "a".repeat(40),
    paths: ["codex-rs/core/src/example.rs"],
    symbols: ["trusted_symbol"],
    tests: ["trusted_symbol_test"],
    expectedDiffSha256: "b".repeat(64),
  }] });
  assert.deepEqual([...registry.paths], ["codex-rs/core/src/example.rs"]);
  assert.deepEqual(disallowedProductChanges(["codex-rs/core/src/example.rs", "codex-rs/core/src/other.rs"], registry.paths), ["codex-rs/core/src/other.rs"]);
  assert.throws(() => parsePatchRegistry({ version: 1, patches: [{ id: "broken" }] }), /登记无效/);
});

test("upstream synchronization identity pins both product baseline and fetched target", () => {
  assert.deepEqual(synchronizationIdentity({ baseSha: "a".repeat(40), targetSha: "b".repeat(40), date: "2026-09-20" }), {
    baseSha: "a".repeat(40),
    targetSha: "b".repeat(40),
    branch: "codex/upstream-sync-2026-09-20-bbbbbbbbbbbb",
  });
});

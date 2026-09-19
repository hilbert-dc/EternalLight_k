import assert from "node:assert/strict";
import test from "node:test";

import { assessRemoteRoles, disallowedProductChanges, isOfficialCodexRemote, normalizeRemoteUrl } from "../../scripts/verify-fork-boundary.mjs";

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

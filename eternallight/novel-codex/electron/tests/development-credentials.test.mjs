import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { loadProjectDevelopmentEnvironment, resolveDevelopmentEnvironment, withoutProviderCredentials } from "../development-credentials.mjs";

test("loads an ignored project-local DeepSeek credential without overriding the process environment", () => {
  const root = mkdtempSync(join(tmpdir(), "novel-codex-local-env-"));
  try {
    writeFileSync(join(root, ".env.local"), "DEEPSEEK_API_KEY=project-test-key\nOPENAI_API_KEY=must-not-load\n");
    assert.deepEqual(loadProjectDevelopmentEnvironment(root, { PATH: "/usr/bin" }), {
      DEEPSEEK_API_KEY: "project-test-key",
      PATH: "/usr/bin",
    });
    assert.equal(
      loadProjectDevelopmentEnvironment(root, { DEEPSEEK_API_KEY: "process-key" }).DEEPSEEK_API_KEY,
      "process-key",
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("explicit DeepSeek credentials take precedence over the macOS Keychain", () => {
  let reads = 0;
  const environment = resolveDevelopmentEnvironment(
    { DEEPSEEK_API_KEY: "explicit-key", PATH: "/usr/bin" },
    { platform: "darwin", readCredential: () => { reads += 1; return "stored-key"; } },
  );

  assert.deepEqual(environment, { DEEPSEEK_API_KEY: "explicit-key", PATH: "/usr/bin" });
  assert.equal(reads, 0);
});

test("macOS development reads a missing DeepSeek credential from Keychain", () => {
  const environment = resolveDevelopmentEnvironment(
    { PATH: "/usr/bin" },
    { platform: "darwin", readCredential: () => " stored-key\n" },
  );

  assert.deepEqual(environment, { DEEPSEEK_API_KEY: "stored-key", PATH: "/usr/bin" });
});

test("missing or unsupported secure storage leaves the credential absent", () => {
  const missing = resolveDevelopmentEnvironment(
    { PATH: "/usr/bin" },
    { platform: "darwin", readCredential: () => "" },
  );
  const unsupported = resolveDevelopmentEnvironment(
    { PATH: "/usr/bin" },
    { platform: "win32", readCredential: () => { throw new Error("must not read"); } },
  );

  assert.deepEqual(missing, { PATH: "/usr/bin" });
  assert.deepEqual(unsupported, { PATH: "/usr/bin" });
});

test("the renderer development server never inherits provider credentials", () => {
  const environment = withoutProviderCredentials({
    CODEX_ACCESS_TOKEN: "codex-token",
    CODEX_API_KEY: "codex-key",
    DEEPSEEK_API_KEY: "deepseek-key",
    OPENAI_ACCESS_TOKEN: "openai-token",
    OPENAI_API_KEY: "openai-key",
    PATH: "/usr/bin",
  });

  assert.deepEqual(environment, { PATH: "/usr/bin" });
});

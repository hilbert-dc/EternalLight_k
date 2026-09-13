import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, mkdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { resolveCodexCli } from "../codex-runtime.mjs";
import { stageReleaseRuntime } from "../../scripts/stage-release-runtime.mjs";

test("stages a pinned CLI that packaged resolution accepts", () => {
  const directory = mkdtempSync(join(tmpdir(), "novel-codex-release-"));
  try {
    const sourceDirectory = join(directory, "source");
    const resourcesPath = join(directory, "resources");
    mkdirSync(sourceDirectory);
    const source = join(sourceDirectory, "codex");
    writeFileSync(source, "#!/bin/sh\nprintf 'codex-cli 0.0.0\\n'\n");
    chmodSync(source, 0o755);
    const staged = stageReleaseRuntime({ source, destination: resourcesPath });
    const resolved = resolveCodexCli({
      isPackaged: true,
      projectRoot: directory,
      resourcesPath,
    });
    assert.equal(resolved.path, realpathSync(staged.executablePath));
    assert.equal(resolved.source, "bundled");
    assert.equal(resolved.reason, null);

    writeFileSync(join(resourcesPath, "codex-runtime", "codex-model-catalog.json"), "{\"version\":1,\"models\":[]}");
    const tampered = resolveCodexCli({
      isPackaged: true,
      projectRoot: directory,
      resourcesPath,
    });
    assert.equal(tampered.path, null);
    assert.match(tampered.reason, /协议版本无效/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

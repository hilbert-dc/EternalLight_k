import assert from "node:assert/strict";
import { appendFileSync, chmodSync, existsSync, mkdtempSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { resolveCodexCli, resolveRuntimeBinaries } from "../codex-runtime.mjs";
import { stageReleaseRuntime } from "../../scripts/stage-release-runtime.mjs";
import { verifyReleaseRuntime } from "../../scripts/verify-release-runtime.mjs";
import { writeFixtureBuildIdentity } from "./build-identity-fixture.mjs";

test("packaging includes the independent bridge worker outside ASAR", () => {
  const manifest = JSON.parse(readFileSync(new URL("../../package.json", import.meta.url), "utf8"));
  assert.ok(manifest.build.files.includes("node_modules/@eternallight/agent-engine/**/*"));
  assert.ok(manifest.build.asarUnpack.includes("node_modules/@eternallight/agent-engine/src/**/*"));
  assert.equal(existsSync(new URL("../../node_modules/@eternallight/agent-engine/src/bridge/worker.mjs", import.meta.url)), true);
});

test("stages and executes the pinned release runtime", async () => {
  const directory = mkdtempSync(join(tmpdir(), "novel-codex-release-"));
  try {
    const sourceDirectory = join(directory, "source");
    const resourcesPath = join(directory, "resources");
    mkdirSync(sourceDirectory);
    const source = join(sourceDirectory, "codex");
    const runtimeServer = join(sourceDirectory, "creative-runtime");
    const mcpServer = join(sourceDirectory, "novel-workspace-mcp");
    writeFileSync(source, "#!/usr/bin/env node\nprocess.stdout.write('codex-cli 0.0.0\\n');\n");
    writeFileSync(runtimeServer, `#!/usr/bin/env node
const http = require("node:http");
const index = process.argv.indexOf("--port");
process.stdin.once("data", () => http.createServer((_request, response) => {
  response.writeHead(200, { "content-type": "application/json" });
  response.end(JSON.stringify({ ok: true, service: "creative-runtime" }));
}).listen(Number(process.argv[index + 1]), "127.0.0.1"));
`);
    writeFileSync(mcpServer, `#!/usr/bin/env node
const readline = require("node:readline");
readline.createInterface({ input: process.stdin }).on("line", (line) => {
  const message = JSON.parse(line);
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: message.id, result: { protocolVersion: "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "novel-codex-workspace", version: "1" } } }) + "\\n");
});
`);
    chmodSync(source, 0o755);
    chmodSync(runtimeServer, 0o755);
    chmodSync(mcpServer, 0o755);
    const provenanceSource = join(directory, "build-identity.json");
    const repositoryRoot = fileURLToPath(new URL("../../../../", import.meta.url));
    writeFixtureBuildIdentity({ binaryPath: source, identityPath: provenanceSource, repositoryRoot });
    const staged = stageReleaseRuntime({
      source,
      runtimeServerSource: runtimeServer,
      mcpServerSource: mcpServer,
      provenanceSource,
      destination: resourcesPath,
      nestedSigner: (path) => appendFileSync(path, "\n// nested-signature\n"),
    });
    const resolved = resolveCodexCli({
      isPackaged: true,
      projectRoot: directory,
      resourcesPath,
    });
    assert.equal(resolved.path, realpathSync(staged.executablePath));
    assert.equal(resolved.source, "bundled");
    assert.equal(resolved.reason, null);
    assert.equal(existsSync(join(resourcesPath, "codex-runtime", "agents", "story_architect.toml")), true);
    assert.equal(existsSync(join(resourcesPath, "codex-runtime", "agents", "continuity_critic.toml")), true);
    assert.equal(existsSync(join(resourcesPath, "codex-runtime", "skills", "novel-workflow", "SKILL.md")), true);
    assert.equal(existsSync(join(resourcesPath, "codex-runtime", "engine-prompt.md")), true);
    assert.deepEqual(resolveRuntimeBinaries({ resourcesPath }), {
      creativeRuntime: realpathSync(staged.runtimeExecutablePath),
      novelWorkspaceMcp: realpathSync(staged.mcpExecutablePath),
    });
    assert.deepEqual(await verifyReleaseRuntime({ resources: resourcesPath, timeoutMs: 5_000 }), {
      cli: "verified",
      mcp: "verified",
      runtime: "verified",
    });

    const stagedSkill = join(resourcesPath, "codex-runtime", "skills", "novel-workflow", "SKILL.md");
    const stagedSkillContents = readFileSync(stagedSkill);
    appendFileSync(stagedSkill, "\n篡改\n");
    await assert.rejects(
      verifyReleaseRuntime({ resources: resourcesPath, timeoutMs: 5_000 }),
      /发布资源 .*摘要或版本不匹配/,
    );
    writeFileSync(stagedSkill, stagedSkillContents);

    writeFileSync(join(resourcesPath, "codex-runtime", "model-catalog.json"), "{\"version\":1,\"connections\":[]}");
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

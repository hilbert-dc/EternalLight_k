import assert from "node:assert/strict";
import { appendFileSync, existsSync, mkdtempSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test, { after, before, mock } from "node:test";
import { fileURLToPath } from "node:url";

import { resolveCodexCli, resolveRuntimeBinaries } from "../codex-runtime.mjs";
import { stageReleaseRuntime } from "../../scripts/stage-release-runtime.mjs";
import { verifyReleaseRuntime } from "../../scripts/verify-release-runtime.mjs";
import { installFixtureExecutableInterpreter, registerFixtureExecutable, writeFixtureBuildIdentity, writeFixtureCodexExecutable, writeFixtureNodeExecutable } from "./build-identity-fixture.mjs";
import { fixtureMediaCalls, installFixtureMediaExecution, registerFixtureMediaRuntime, writeFixtureMediaRuntime } from "./media-runtime-fixture.mjs";

let restoreFixtureInterpreter, restoreMediaExecution;
before(() => { restoreFixtureInterpreter = installFixtureExecutableInterpreter(mock); restoreMediaExecution = installFixtureMediaExecution(mock); });
after(() => { restoreMediaExecution(); restoreFixtureInterpreter(); });

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
    const mediaRuntimeSource = writeFixtureMediaRuntime(join(sourceDirectory, "media-runtime"));
    writeFixtureCodexExecutable(source);
    writeFixtureNodeExecutable(runtimeServer, `
const http = require("node:http");
const index = process.argv.indexOf("--port");
process.stdin.once("data", () => http.createServer((_request, response) => {
  response.writeHead(200, { "content-type": "application/json" });
  response.end(JSON.stringify({ ok: true, service: "creative-runtime" }));
}).listen(Number(process.argv[index + 1]), "127.0.0.1"));
`);
    writeFixtureNodeExecutable(mcpServer, `
const readline = require("node:readline");
readline.createInterface({ input: process.stdin }).on("line", (line) => {
  const message = JSON.parse(line);
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: message.id, result: { protocolVersion: "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "novel-codex-workspace", version: "1" } } }) + "\\n");
});
`);
    const provenanceSource = join(directory, "build-identity.json");
    const repositoryRoot = fileURLToPath(new URL("../../../../", import.meta.url));
    writeFixtureBuildIdentity({ binaryPath: source, identityPath: provenanceSource, repositoryRoot });
    const staged = stageReleaseRuntime({
      source,
      runtimeServerSource: runtimeServer,
      mcpServerSource: mcpServer,
      mediaRuntimeSource,
      provenanceSource,
      destination: resourcesPath,
      nestedSigner: (path) => appendFileSync(path, "\n// nested-signature\n"),
    });
    for (const path of [staged.executablePath, staged.runtimeExecutablePath, staged.mcpExecutablePath]) registerFixtureExecutable(path);
    registerFixtureMediaRuntime(join(resourcesPath, "media-runtime"));
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
    for (const role of ["character_analyst", "style_reviewer", "reader_experience"]) {
      assert.equal(existsSync(join(resourcesPath, "codex-runtime", "agents", `${role}.toml`)), true);
    }
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
      media: "verified",
    });
    assert.equal(fixtureMediaCalls.length, 4, "explicit mock covers fixed versions, build flags and actual-Electron smoke dispatch; real rendering has a separate integration test");
    assert.equal(fixtureMediaCalls.at(-1).name, "electronSubtitleSmoke");
    assert.ok(fixtureMediaCalls.every(call => call.environmentPath === ""));
    const mediaManifest = JSON.parse(readFileSync(join(resourcesPath, "media-runtime", "media-runtime-manifest.json"), "utf8"));
    const ffmpegRecord = mediaManifest.files[mediaManifest.entrypoints.ffmpeg];
    assert.notEqual(ffmpegRecord.sha256, ffmpegRecord.unsignedSha256);

    const stagedFont = join(resourcesPath, "media-runtime", "subtitle-font.ttf"), originalFont = readFileSync(stagedFont);
    appendFileSync(stagedFont, "corrupt font");
    await assert.rejects(verifyReleaseRuntime({ resources: resourcesPath, timeoutMs: 5_000 }), /发布资源 .*摘要或版本不匹配/);
    writeFileSync(stagedFont, originalFont);

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

test("release refuses missing media components before writing a partial bundle", () => {
  const directory = mkdtempSync(join(tmpdir(), "novel-codex-release-missing-media-"));
  try {
    const destination = join(directory, "resources");
    assert.throws(() => stageReleaseRuntime({ mediaRuntimeSource: null, destination }), /便携媒体组件目录/);
    assert.equal(existsSync(destination), false);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

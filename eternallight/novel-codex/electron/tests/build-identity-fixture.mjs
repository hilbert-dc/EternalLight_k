import { createHash } from "node:crypto";
import childProcess, { spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { syncBuiltinESMExports } from "node:module";
import { dirname, join, relative } from "node:path";

import { codexSourceTreeDigest } from "../../scripts/build-provenance.mjs";

const fixtureExecutables = new Set();

export function registerFixtureExecutable(path) {
  fixtureExecutables.add(realpathSync(path));
}

// JavaScript stand-ins are not native images. Run their actual bytes through
// the current Node interpreter so a new temporary shebang path cannot stall
// in macOS dyld before main. Only explicitly registered fixtures are routed;
// real CLI/Rust binaries, git, exit codes, pipes and deadlines stay untouched.
// Install once per isolated test-file process and restore its ESM bindings.
export function installFixtureExecutableInterpreter(mock) {
  const nativeSpawn = childProcess.spawn;
  const nativeSpawnSync = childProcess.spawnSync;
  const isFixture = command => {
    try { return fixtureExecutables.has(realpathSync(command)); } catch { return false; }
  };
  const asyncLauncher = mock.method(childProcess, "spawn", (command, args = [], options) => isFixture(command)
    ? nativeSpawn(process.execPath, [command, ...args], options)
    : nativeSpawn(command, args, options));
  const syncLauncher = mock.method(childProcess, "spawnSync", (command, args = [], options) => isFixture(command)
    ? nativeSpawnSync(process.execPath, [command, ...args], options)
    : nativeSpawnSync(command, args, options));
  syncBuiltinESMExports();
  return () => {
    asyncLauncher.mock.restore();
    syncLauncher.mock.restore();
    syncBuiltinESMExports();
    fixtureExecutables.clear();
  };
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

// Keep version stand-ins executable: production provenance and resolution
// must still probe actual bytes rather than trusting fixture metadata.
export function writeFixtureCodexExecutable(path, version = "codex-cli 0.0.0") {
  writeFixtureNodeExecutable(path, `if (process.argv.length !== 3 || process.argv[2] !== "--version") {
  process.stderr.write("fixture Codex supports only --version\\n");
  process.exit(64);
}
process.stdout.write(${JSON.stringify(`${version}\n`)});\n`);
}

// env -S quotes the absolute interpreter, not a PATH-resolved "node". Do not
// use a shell trampoline: direct shebang /bin/sh execution on the affected
// macOS host can stall at _dyld_start before any fixture code runs.
export function writeFixtureNodeExecutable(path, body) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `#!/usr/bin/env -S ${JSON.stringify(process.execPath)}\n${body}`);
  chmodSync(path, 0o755);
  registerFixtureExecutable(path);
}

export function initializeFixtureCodexRepository(repositoryRoot) {
  const codexRoot = join(repositoryRoot, "codex-rs");
  mkdirSync(codexRoot, { recursive: true });
  writeFileSync(join(repositoryRoot, ".gitignore"), "codex-rs/target/\n");
  writeFileSync(join(codexRoot, "Cargo.lock"), "# fixture lock\n");
  const initialized = spawnSync("git", ["init", "--quiet"], { cwd: repositoryRoot, encoding: "utf8" });
  if (initialized.status !== 0) throw new Error(`fixture git init failed: ${initialized.stderr}`);
  return codexSourceTreeDigest(repositoryRoot);
}

export function writeFixtureBuildIdentity({
  architecture = process.arch,
  binaryPath,
  identityPath,
  platform = process.platform,
  repositoryRoot = null,
  sourceTreeDigest = repositoryRoot ? codexSourceTreeDigest(repositoryRoot) : "0".repeat(64),
}) {
  const started = performance.now();
  const version = spawnSync(binaryPath, ["--version"], { encoding: "utf8", timeout: 15_000 });
  if (version.error || version.status !== 0) {
    const reason = version.error?.code ?? version.signal ?? `exit=${version.status}`;
    const interpreter = readFileSync(binaryPath, "utf8").split("\n", 1)[0];
    throw new Error(`fixture Codex binary did not report a version (${reason}; elapsed=${Math.round(performance.now() - started)}ms; path=${binaryPath}; interpreter=${interpreter}; stderr=${(version.stderr ?? "").slice(-1000)})`);
  }
  const body = {
    version: 1,
    product: "codex-cli",
    sourceTreeDigest,
    cargoLockSha256: repositoryRoot ? sha256(join(repositoryRoot, "codex-rs", "Cargo.lock")) : "0".repeat(64),
    rustc: "rustc fixture\nhost: fixture-target",
    buildCommand: "fixture-build",
    target: { platform, architecture, rustcHost: "fixture-target" },
    binary: {
      path: repositoryRoot ? relative(repositoryRoot, binaryPath).replaceAll("\\", "/") : "fixture/codex",
      sha256: sha256(binaryPath),
      version: version.stdout.trim(),
    },
  };
  const identity = { identityId: createHash("sha256").update(JSON.stringify(body)).digest("hex"), ...body };
  mkdirSync(dirname(identityPath), { recursive: true });
  writeFileSync(identityPath, `${JSON.stringify(identity, null, 2)}\n`, { mode: 0o600 });
  return { identity, sha256: sha256(identityPath) };
}

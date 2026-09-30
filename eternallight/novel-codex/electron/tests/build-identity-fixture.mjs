import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";

import { codexSourceTreeDigest } from "../../scripts/build-provenance.mjs";

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
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
  const version = spawnSync(binaryPath, ["--version"], { encoding: "utf8", timeout: 15_000 });
  if (version.status !== 0) {
    const reason = version.error?.code ?? version.signal ?? `exit=${version.status}`;
    throw new Error(`fixture Codex binary did not report a version (${reason})`);
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

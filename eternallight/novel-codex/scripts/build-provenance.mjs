import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { relative, resolve } from "node:path";

export function sha256File(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function command(cwd, program, args) {
  const result = spawnSync(program, args, { cwd, encoding: "utf8", timeout: 120_000 });
  if (result.status !== 0) throw new Error(`${program} ${args.join(" ")} failed: ${(result.stderr || result.stdout).trim()}`);
  return result.stdout.trim();
}

export function codexSourceTreeDigest(repositoryRoot) {
  const names = command(repositoryRoot, "git", ["ls-files", "-co", "--exclude-standard", "--", "codex-rs"])
    .split("\n").filter(Boolean).sort();
  const hash = createHash("sha256");
  for (const name of names) {
    const path = resolve(repositoryRoot, name);
    if (!existsSync(path)) continue;
    hash.update(`${name}\0${sha256File(path)}\n`, "utf8");
  }
  return hash.digest("hex");
}

export function defineBuildIdentity({ repositoryRoot, binaryPath, buildCommand, sourceTreeDigest, platform = process.platform, architecture = process.arch }) {
  const codexRoot = resolve(repositoryRoot, "codex-rs");
  const version = command(codexRoot, binaryPath, ["--version"]);
  const rustc = command(codexRoot, "rustc", ["-Vv"]);
  const body = {
    version: 1,
    product: "codex-cli",
    sourceTreeDigest,
    cargoLockSha256: sha256File(resolve(codexRoot, "Cargo.lock")),
    rustc,
    buildCommand,
    artifactPath: relative(repositoryRoot, binaryPath).replaceAll("\\", "/"),
    target: { platform, architecture, rustcHost: rustc.split("\n").find((line) => line.startsWith("host: "))?.slice(6) ?? null },
    binary: { path: relative(repositoryRoot, binaryPath).replaceAll("\\", "/"), sha256: sha256File(binaryPath), version },
  };
  return Object.freeze({ identityId: createHash("sha256").update(JSON.stringify(body)).digest("hex"), ...body });
}

export function readBuildIdentity(path) {
  const identity = JSON.parse(readFileSync(path, "utf8"));
  const { identityId, ...body } = identity;
  const actual = createHash("sha256").update(JSON.stringify(body)).digest("hex");
  if (!/^[a-f0-9]{64}$/.test(identityId ?? "") || identityId !== actual || identity.version !== 1) throw Object.assign(new Error("Codex BuildIdentity is invalid"), { code: "cli_identity_invalid", field: "identityId" });
  return identity;
}

export function verifyBuildIdentity(identity, binaryPath, { repositoryRoot = null } = {}) {
  if (identity.binary.sha256 !== sha256File(binaryPath)) throw Object.assign(new Error("Codex binary does not match BuildIdentity"), { code: "cli_identity_binary_mismatch", field: "binary.sha256" });
  const version = spawnSync(binaryPath, ["--version"], { encoding: "utf8", timeout: 15_000 });
  if (version.status !== 0 || version.stdout.trim() !== identity.binary.version) throw Object.assign(new Error("Codex binary version does not match BuildIdentity"), { code: "cli_identity_version_mismatch", field: "binary.version" });
  if (repositoryRoot && codexSourceTreeDigest(repositoryRoot) !== identity.sourceTreeDigest) throw Object.assign(new Error("Codex source tree changed after the recorded build"), { code: "cli_source_changed", field: "sourceTreeDigest" });
  return identity;
}

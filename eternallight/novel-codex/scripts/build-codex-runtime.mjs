import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { codexSourceTreeDigest, defineBuildIdentity } from "./build-provenance.mjs";
import { resolveBuildLayout } from "./build-layout.mjs";
import { assertCodexMigrationVersionsUnique } from "./verify-migration-identities.mjs";

const { productRoot, repositoryRoot } = resolveBuildLayout();
const codexRoot = join(repositoryRoot, "codex-rs");
assertCodexMigrationVersionsUnique(repositoryRoot);
const profile = process.env.NOVEL_CODEX_CARGO_PROFILE?.trim() || "debug";
if (!["debug", "release"].includes(profile)) throw new Error("NOVEL_CODEX_CARGO_PROFILE must be debug or release");
const command = ["cargo", "build", "--locked", "--message-format=json-render-diagnostics", ...(profile === "release" ? ["--release"] : []), "-p", "codex-cli", "--bin", "codex"];
const before = codexSourceTreeDigest(repositoryRoot);
const result = spawnSync(command[0], command.slice(1), { cwd: codexRoot, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
if (result.stderr) process.stderr.write(result.stderr);
if (result.status !== 0) process.exit(result.status ?? 1);
const artifacts = result.stdout.split("\n").filter(Boolean).flatMap((line) => {
  try {
    const message = JSON.parse(line);
    return message.reason === "compiler-artifact" && message.target?.name === "codex" && Array.isArray(message.target?.kind) && message.target.kind.includes("bin") && message.executable
      ? [message.executable]
      : [];
  } catch {
    return [];
  }
});
if (artifacts.length !== 1) throw new Error(`Cargo did not report exactly one codex executable (found ${artifacts.length})`);
const after = codexSourceTreeDigest(repositoryRoot);
if (after !== before) throw new Error("Codex source tree changed while the CLI was building");
const binaryPath = resolve(artifacts[0]);
const identity = defineBuildIdentity({ repositoryRoot, binaryPath, buildCommand: command.join(" "), sourceTreeDigest: before });
const directory = join(productRoot, "codex-runtime", "build-identities");
mkdirSync(directory, { recursive: true, mode: 0o700 });
const path = join(directory, `${process.platform}-${process.arch}.json`);
writeFileSync(path, `${JSON.stringify(identity, null, 2)}\n`, { mode: 0o600 });
process.stdout.write(`${JSON.stringify({ binaryPath, buildIdentityPath: path, identityId: identity.identityId })}\n`);

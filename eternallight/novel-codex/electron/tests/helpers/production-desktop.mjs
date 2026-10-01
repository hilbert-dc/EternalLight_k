import { cp, mkdir, readFile, symlink, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { CAPABILITIES, defineQualificationSuiteResult } from "@eternallight/agent-engine";
import { certifyModel } from "../../../scripts/certify-model.mjs";
import { readBuildIdentity, verifyBuildIdentity } from "../../../scripts/build-provenance.mjs";

const original = fileURLToPath(new URL("../../../", import.meta.url));
const repository = resolve(original, "../..");
export const fixtureCredential = "production-desktop-fixture-only";

// Only the temporary app's model/catalog evidence is a fixture. No production
// handler, budget, credential check, or CLI integrity implementation is replaced.
export async function prepareProductionDesktop(directory, endpoint) {
  const appRoot = join(directory, "repo", "eternallight", "novel-codex");
  await mkdir(appRoot, { recursive: true });
  for (const name of ["electron", "shared", "scripts", "codex-runtime"]) {
    await cp(join(original, name), join(appRoot, name), { recursive: true });
  }
  for (const [from, to, type] of [
    [join(repository, ".git"), join(directory, "repo", ".git"), "dir"],
    [join(repository, "codex-rs"), join(directory, "repo", "codex-rs"), "dir"],
    [join(original, "node_modules"), join(appRoot, "node_modules"), "dir"],
    [join(original, "native"), join(appRoot, "native"), "dir"],
  ]) await symlink(from, to, process.platform === "win32" ? "junction" : type);
  // Git still lists every tracked upstream source through the link. Do not add
  // the directory symlink itself as an untracked source-tree entry.
  await writeFile(join(directory, "repo", ".gitignore"), "codex-rs\n");
  const catalogPath = join(appRoot, "codex-runtime", "model-catalog.json");
  const policyPath = join(appRoot, "codex-runtime", "supported-cli.json");
  const policy = JSON.parse(await readFile(policyPath, "utf8"));
  const connection = {
    id: "desktop-fixture", provider: "Fixture", model: "desktop-scripted-v1",
    endpoint, credentialRef: "DESKTOP_FIXTURE_KEY", protocol: "chat-completions",
    protocolOptions: { namespaceToolsMode: "flatten" },
    capabilitySupport: { text: "native", structuredToolCalls: "lossless-adapted", streaming: "native", usage: "native" },
    declaredLimits: { contextTokens: 131072, outputTokens: 8192 },
    declaredCapabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.STREAMING, CAPABILITIES.USAGE],
  };
  await writeFile(catalogPath, JSON.stringify({ version: 1, defaultConnectionId: connection.id, connections: [connection] }));
  const certificationsPath = join(appRoot, "codex-runtime", "model-certifications.json");
  await writeFile(certificationsPath, JSON.stringify({ version: 2, certifications: [] }));
  policy.modelCertificationsFile = "model-certifications.json";
  await writeFile(policyPath, JSON.stringify(policy));
  const cliPath = join(repository, "codex-rs", "target", "debug", process.platform === "win32" ? "codex.exe" : "codex");
  verifyBuildIdentity(readBuildIdentity(join(appRoot, "codex-runtime", "build-identities", `${process.platform}-${process.arch}.json`)),
    cliPath, { repositoryRoot: join(directory, "repo") });
  // Use validators/publication on a disposable snapshot. These injected probe
  // results are fixture qualification metadata, not a supplier qualification.
  await certifyModel({
    cliPath, environment: { DESKTOP_FIXTURE_KEY: fixtureCredential },
    baseInstructions: await readFile(join(repository, "codex-rs", "models-manager", "prompt.md"), "utf8"),
    paths: { catalogPath, policyPath, certificationsPath, reportDirectory: join(directory, "fixture-reports") },
    verifier: { verify: async () => ({ selectedProtocol: connection.protocol, attempts: [],
      records: connection.declaredCapabilities.map(capability => ({ capability, status: "verified" })) }) },
    coreRunner: { verify: async () => defineQualificationSuiteResult({ kind: "core-integration", suiteId: "desktop-fixture-metadata-v1", suiteVersion: 1, status: "passed" }) },
  });
  return { appRoot, cliPath };
}

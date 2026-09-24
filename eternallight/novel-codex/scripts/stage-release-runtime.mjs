import { createHash } from "node:crypto";
import { copyFileSync, cpSync, chmodSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { readBuildIdentity, verifyBuildIdentity } from "./build-provenance.mjs";
import { resolveBuildLayout } from "./build-layout.mjs";
import { certificationPath } from "./certification-publication.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const defaultLayout = resolveBuildLayout();

function digest(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function command(directory, ...args) {
  const result = spawnSync("git", ["-C", directory, ...args], { encoding: "utf8", timeout: 15_000 });
  if (result.status !== 0) throw new Error(`无法读取 Git 来源：${result.stderr.trim()}`);
  return result.stdout.trim();
}

function treeDigest(directory) {
  const files = [];
  const visit = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.isFile()) files.push(`${relative(directory, path).replaceAll("\\", "/")}:${digest(path)}`);
    }
  };
  visit(directory);
  return createHash("sha256").update(files.join("\n"), "utf8").digest("hex");
}

function fileManifest(directory) {
  const files = {};
  const visit = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.isFile() && entry.name !== "release-manifest.json") files[relative(directory, path).replaceAll("\\", "/")] = digest(path);
    }
  };
  visit(directory);
  return files;
}

function signNestedBinaries(paths, signer = process.env.NOVEL_CODEX_NESTED_SIGNER) {
  if (!signer) return;
  for (const path of paths) {
    const result = spawnSync(resolve(signer), [path], { encoding: "utf8", timeout: 120_000 });
    if (result.status !== 0) throw new Error(`嵌套程序签名失败：${result.stderr || result.stdout}`);
  }
}

export function stageReleaseRuntime({ source, runtimeServerSource, mcpServerSource, provenanceSource = join(root, "codex-runtime", "build-identities", `${process.platform}-${process.arch}.json`), destination = defaultLayout.resources, platform = defaultLayout.platform, architecture = defaultLayout.architecture, nestedSigner = null }) {
  if (!runtimeServerSource || !mcpServerSource) throw new Error("必须提供固定的 Creative Runtime 与 MCP 可执行文件");
  const repositoryRoot = resolve(root, "../..");
  const buildIdentity = readBuildIdentity(resolve(provenanceSource));
  source ??= resolve(repositoryRoot, buildIdentity.binary.path);
  verifyBuildIdentity(buildIdentity, resolve(source), { repositoryRoot });
  if (buildIdentity.target.platform !== platform || buildIdentity.target.architecture !== architecture) throw new Error("BuildIdentity 与发布目标不匹配");
  const version = spawnSync(source, ["--version"], { encoding: "utf8", timeout: 15_000 });
  if (version.status !== 0 || version.stdout.trim() !== "codex-cli 0.0.0") throw new Error("发布 CLI 版本不在固定支持范围内");
  const runtimeSource = join(root, "codex-runtime");
  const policy = JSON.parse(readFileSync(join(runtimeSource, "supported-cli.json"), "utf8"));
  const runtimeDestination = join(resolve(destination), "codex-runtime");
  const executableName = platform === "win32" ? "codex.exe" : "codex";
  mkdirSync(join(runtimeDestination, "bin"), { recursive: true });
  copyFileSync(join(runtimeSource, "model-catalog.json"), join(runtimeDestination, "model-catalog.json"));
  copyFileSync(certificationPath(runtimeSource, policy), join(runtimeDestination, "model-certifications.json"));
  copyFileSync(resolve(root, "../../codex-rs/models-manager/prompt.md"), join(runtimeDestination, "engine-prompt.md"));
  cpSync(join(runtimeSource, "agents"), join(runtimeDestination, "agents"), { recursive: true });
  cpSync(join(runtimeSource, "skills"), join(runtimeDestination, "skills"), { recursive: true });
  const executablePath = join(runtimeDestination, "bin", executableName);
  copyFileSync(resolve(source), executablePath);
  const buildIdentityPath = join(runtimeDestination, "build-identity.json");
  copyFileSync(resolve(provenanceSource), buildIdentityPath);
  if (platform !== "win32") chmodSync(executablePath, 0o755);
  const runtimeBin = join(resolve(destination), "runtime", "bin");
  mkdirSync(runtimeBin, { recursive: true });
  const runtimeExecutablePath = join(runtimeBin, platform === "win32" ? "creative-runtime.exe" : "creative-runtime");
  const mcpExecutablePath = join(runtimeBin, platform === "win32" ? "novel-workspace-mcp.exe" : "novel-workspace-mcp");
  copyFileSync(resolve(runtimeServerSource), runtimeExecutablePath);
  copyFileSync(resolve(mcpServerSource), mcpExecutablePath);
  if (platform !== "win32") {
    chmodSync(runtimeExecutablePath, 0o755);
    chmodSync(mcpExecutablePath, 0o755);
  }
  if (nestedSigner) {
    for (const path of [executablePath, runtimeExecutablePath, mcpExecutablePath]) nestedSigner(path);
  } else {
    signNestedBinaries([executablePath, runtimeExecutablePath, mcpExecutablePath]);
  }
  if (policy.modelCatalogSha256 !== digest(join(runtimeSource, "model-catalog.json"))
    || policy.modelCertificationsSha256 !== digest(certificationPath(runtimeSource, policy))) {
    throw new Error("模型目录或模型认证未同步到 supported-cli.json");
  }
  policy.modelCertificationsFile = "model-certifications.json";
  policy.sources.bundled.digests = { [`${platform}-${architecture}`]: digest(executablePath) };
  policy.sources.bundled.buildIdentities = {
    [`${platform}-${architecture}`]: {
      identityId: buildIdentity.identityId,
      sha256: digest(buildIdentityPath),
      unsignedBinarySha256: buildIdentity.binary.sha256,
    },
  };
  policy.runtimeBinaries = {
    [`${platform}-${architecture}`]: {
      creativeRuntime: digest(runtimeExecutablePath),
      novelWorkspaceMcp: digest(mcpExecutablePath),
    },
  };
  writeFileSync(join(runtimeDestination, "supported-cli.json"), `${JSON.stringify(policy, null, 2)}\n`, { mode: 0o600 });
  const sourceFiles = {
    modelCatalog: join(runtimeDestination, "model-catalog.json"),
    modelCertifications: join(runtimeDestination, "model-certifications.json"),
    engineInstructions: join(runtimeDestination, "engine-prompt.md"),
    engineContract: resolve(root, "../agent-engine/src/contracts/capabilities.mjs"),
    codecTree: resolve(root, "../agent-engine/src/codecs"),
    nativeEngine: resolve(root, `../agent-engine/native/bin/${platform}-${architecture}/engine.node`),
    resolver: resolve(root, "../agent-engine/src/session/resolver.mjs"),
    extensionRegistry: resolve(root, "../agent-engine/src/session/extension-registry.mjs"),
    capabilityVerifier: resolve(root, "../agent-engine/src/capabilities/verifier.mjs"),
    conformanceRunner: resolve(root, "electron/codex-conformance-runner.mjs"),
    conformanceMcp: resolve(root, "electron/conformance-mcp.mjs"),
    storyCore: resolve(root, "native/story-core/src"),
  };
  for (const path of Object.values(sourceFiles)) {
    if (!statSync(path).isFile() && !statSync(path).isDirectory()) throw new Error(`发布来源缺失：${path}`);
  }
  const releaseManifest = {
    version: 1,
    product: {
      commit: command(repositoryRoot, "rev-parse", "HEAD"),
      dirty: Boolean(command(repositoryRoot, "status", "--porcelain")),
    },
    codex: {
      commit: command(repositoryRoot, "rev-parse", "HEAD"),
      binaryVersion: version.stdout.trim(),
      binarySha256: digest(executablePath),
      unsignedBinarySha256: buildIdentity.binary.sha256,
      buildIdentityId: buildIdentity.identityId,
      buildIdentitySha256: digest(buildIdentityPath),
      sourcePath: relative(repositoryRoot, resolve(source)).replaceAll("\\", "/"),
    },
    target: { platform, architecture },
    resources: {
      creativeRuntimeSha256: digest(runtimeExecutablePath),
      novelWorkspaceMcpSha256: digest(mcpExecutablePath),
      modelCatalogSha256: digest(sourceFiles.modelCatalog),
      modelCertificationsSha256: digest(sourceFiles.modelCertifications),
      engineInstructionsSha256: digest(sourceFiles.engineInstructions),
      skillsSha256: treeDigest(join(runtimeDestination, "skills")),
      agentsSha256: treeDigest(join(runtimeDestination, "agents")),
      engineContractSha256: digest(sourceFiles.engineContract),
      codecsSha256: treeDigest(sourceFiles.codecTree),
      resolverSha256: digest(sourceFiles.resolver),
      extensionRegistrySha256: digest(sourceFiles.extensionRegistry),
      capabilityVerifierSha256: digest(sourceFiles.capabilityVerifier),
      conformanceRunnerSha256: digest(sourceFiles.conformanceRunner),
      conformanceMcpSha256: digest(sourceFiles.conformanceMcp),
      storyCoreSha256: treeDigest(sourceFiles.storyCore),
      files: null,
    },
    reports: (process.env.NOVEL_CODEX_RELEASE_REPORTS ?? "").split(",").map((value) => value.trim()).filter(Boolean),
  };
  const releaseManifestPath = join(resolve(destination), "release-manifest.json");
  releaseManifest.resources.files = fileManifest(resolve(destination));
  writeFileSync(releaseManifestPath, `${JSON.stringify(releaseManifest, null, 2)}\n`, { mode: 0o600 });
  return { executablePath, runtimeExecutablePath, mcpExecutablePath, releaseManifestPath, platform, architecture, version: version.stdout.trim(), sha256: policy.sources.bundled.digests[`${platform}-${architecture}`] };
}

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  const result = stageReleaseRuntime({
    source: process.env.NOVEL_CODEX_RELEASE_CLI,
    runtimeServerSource: process.env.NOVEL_CODEX_RELEASE_RUNTIME_SERVER,
    mcpServerSource: process.env.NOVEL_CODEX_RELEASE_MCP_SERVER,
    provenanceSource: process.env.NOVEL_CODEX_RELEASE_PROVENANCE,
    destination: process.env.NOVEL_CODEX_RELEASE_RESOURCES,
  });
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

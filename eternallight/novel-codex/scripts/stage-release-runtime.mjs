import { createHash } from "node:crypto";
import { copyFileSync, cpSync, chmodSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const root = resolve(fileURLToPath(new URL("..", import.meta.url)));

function digest(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

export function stageReleaseRuntime({ source, destination, platform = process.platform, architecture = process.arch }) {
  if (!source || !destination) throw new Error("必须提供固定的发布 CLI 和资源暂存目录");
  const version = spawnSync(source, ["--version"], { encoding: "utf8", timeout: 15_000 });
  if (version.status !== 0 || version.stdout.trim() !== "codex-cli 0.0.0") throw new Error("发布 CLI 版本不在固定支持范围内");
  const runtimeSource = join(root, "codex-runtime");
  const runtimeDestination = join(resolve(destination), "codex-runtime");
  const executableName = platform === "win32" ? "codex.exe" : "codex";
  mkdirSync(join(runtimeDestination, "bin"), { recursive: true });
  for (const name of ["model-catalog.json", "codex-model-catalog.json"]) copyFileSync(join(runtimeSource, name), join(runtimeDestination, name));
  cpSync(join(root, "runtime"), join(resolve(destination), "runtime"), {
    recursive: true,
    filter: (path) => !path.includes("__pycache__") && !path.includes(`${join("runtime", "tests")}`),
  });
  const executablePath = join(runtimeDestination, "bin", executableName);
  copyFileSync(resolve(source), executablePath);
  if (platform !== "win32") chmodSync(executablePath, 0o755);
  const policy = JSON.parse(readFileSync(join(runtimeSource, "supported-cli.json"), "utf8"));
  policy.sources.bundled.digests = { [`${platform}-${architecture}`]: digest(executablePath) };
  writeFileSync(join(runtimeDestination, "supported-cli.json"), `${JSON.stringify(policy, null, 2)}\n`, { mode: 0o600 });
  return { executablePath, platform, architecture, version: version.stdout.trim(), sha256: policy.sources.bundled.digests[`${platform}-${architecture}`] };
}

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  const result = stageReleaseRuntime({
    source: process.env.NOVEL_CODEX_RELEASE_CLI,
    destination: process.env.NOVEL_CODEX_RELEASE_RESOURCES,
  });
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

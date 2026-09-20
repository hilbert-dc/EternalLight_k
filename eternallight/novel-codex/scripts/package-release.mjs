import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { resolveBuildLayout } from "./build-layout.mjs";

const target = process.argv[2] ?? "dir";
if (!new Set(["dir", "mac", "win"]).has(target)) throw new Error("package target must be dir, mac, or win");
const layout = resolveBuildLayout();

function run(program, args, options = {}) {
  const result = spawnSync(program, args, { cwd: layout.productRoot, stdio: "inherit", ...options });
  if (result.status !== 0) process.exit(result.status ?? 1);
}

run("npm", ["run", "stage:release-runtime"]);
run("npm", ["run", "verify:release-runtime"]);
run("npm", ["run", "build"]);

const packageJson = JSON.parse(readFileSync(join(layout.productRoot, "package.json"), "utf8"));
const configPath = join(layout.productRoot, ".release", "electron-builder.generated.json");
writeFileSync(configPath, `${JSON.stringify({
  ...packageJson.build,
  directories: { ...(packageJson.build.directories ?? {}), output: layout.output },
  extraResources: [{ from: layout.resources, to: ".", filter: ["**/*"] }],
}, null, 2)}\n`);
run(join(layout.productRoot, "node_modules", ".bin", "electron-builder"), [target === "dir" ? "--dir" : `--${target}`, "--publish", "never", "--config", configPath]);

function findPackagedExecutable(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      const nested = findPackagedExecutable(path);
      if (nested) return nested;
    } else if (entry.isFile() && (
      path.endsWith(join("Contents", "MacOS", packageJson.productName))
      || path.endsWith(join("win-unpacked", `${packageJson.productName}.exe`))
    )) return path;
  }
  return null;
}

const packagedExecutable = findPackagedExecutable(layout.output);
if (!packagedExecutable || !statSync(packagedExecutable).isFile()) throw new Error("找不到最终解包应用可执行文件，不能执行发布 smoke");
const smokeHome = mkdtempSync(join(tmpdir(), "novel-codex-package-smoke-"));
try {
  const smoke = spawnSync(packagedExecutable, [], {
    cwd: layout.productRoot,
    encoding: "utf8",
    timeout: 30_000,
    env: Object.fromEntries(Object.entries({
      PATH: process.env.PATH,
      PATHEXT: process.env.PATHEXT,
      SystemRoot: process.env.SystemRoot,
      WINDIR: process.env.WINDIR,
      ComSpec: process.env.ComSpec,
      TEMP: process.env.TEMP,
      TMP: process.env.TMP,
      TMPDIR: process.env.TMPDIR,
      NOVEL_CODEX_PACKAGE_SMOKE: "1",
      NOVEL_CODEX_USER_DATA_DIR: smokeHome,
    }).filter(([, value]) => typeof value === "string" && value)),
  });
  if (smoke.status !== 0 || !smoke.stdout.includes("NOVEL_CODEX_PACKAGE_SMOKE_OK")) {
    throw new Error(`最终应用 smoke 失败 (${smoke.status}): ${smoke.stderr || smoke.stdout}`);
  }
} finally {
  rmSync(smokeHome, { recursive: true, force: true });
}

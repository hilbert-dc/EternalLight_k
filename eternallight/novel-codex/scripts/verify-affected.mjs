import { execFileSync, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

export function verificationPlan(paths) {
  const executableChanges = paths.filter((raw) => {
    const p = raw.replaceAll("\\", "/").replace(/^eternallight\//, "");
    return !/\.md$/.test(p) && !(/^(?:novel-codex\/)?(?:archive\/|docs\/|design-reviews\/)/.test(p) && /\.(?:json|png|jpe?g|svg|txt)$/.test(p));
  });
  if (executableChanges.length === 0) return ["check:document-archive"];
  const checks = new Set(["architecture:check", "check:preload-api", "check:runtime-contracts", "check:executor-tools"]);
  const add = (...names) => names.forEach((name) => checks.add(name));
  for (const raw of executableChanges) {
    const p = raw.replaceAll("\\", "/").replace(/^eternallight\//, "");
    if (p.startsWith("agent-engine/")) { add("test:engine", "test:creative-journey", "test:task-controls"); continue; }
    const local = p.replace(/^novel-codex\//, "");
    if (local.startsWith("native/")) {
      // Core facts feed Authoring, Runtime and MCP; a Rust edit always tests that closure.
      add("test:native", "test:task-controls", "test:creative-journey");
    } else if (local.startsWith("shared/")) {
      add("build", "test:native", "test:agent", "test:ui", "test:task-controls", "test:multimedia");
    } else if (local.startsWith("src/")) {
      add("build", "test:ui", "test:task-controls", "test:multimedia");
    } else if (local.startsWith("electron/")) {
      add("build", "test:agent", "test:task-controls", "test:creative-journey", "test:multimedia", "test:author-session");
    } else if (local.startsWith("scripts/") || local.startsWith("codex-runtime/") || /^(?:package(?:-lock)?\.json|tsconfig.*\.json|vite\.config\.|native\/Cargo)/.test(local)) {
      return ["verify"];
    } else if (!/^(?:\.gitignore|LICENSE)$/.test(local)) {
      // New/unknown executable boundaries must not silently lose coverage.
      return ["verify"];
    }
  }
  return [...checks];
}

export function changedPaths(base, cwd) {
  const repository = execFileSync("git", ["rev-parse", "--show-toplevel"], { cwd, encoding: "utf8" }).trim();
  const git = (...args) => execFileSync("git", args, { cwd: repository, encoding: "utf8" }).split("\0").filter(Boolean);
  // Include staged/unstaged/untracked work, while CI also covers commits since the merge base.
  return [...new Set([
    ...git("diff", "--name-only", "-z", `${base}...HEAD`),
    ...git("diff", "--name-only", "-z", "HEAD"),
    ...git("ls-files", "--others", "--exclude-standard", "-z"),
  ])].filter((p) => p.startsWith("eternallight/") || p === ".github/workflows/novel-codex-ci.yml");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const root = fileURLToPath(new URL("../", import.meta.url));
  const baseIndex = args.indexOf("--base");
  const filesIndex = args.indexOf("--files");
  const files = filesIndex >= 0 ? args.slice(filesIndex + 1).filter((arg) => arg !== "--dry-run") : changedPaths(baseIndex >= 0 ? args[baseIndex + 1] : "HEAD", root);
  const plan = verificationPlan(files);
  console.log(JSON.stringify({ paths: files, checks: plan }, null, 2));
  if (!args.includes("--dry-run")) {
    for (const check of plan) {
      const result = spawnSync(process.platform === "win32" ? "npm.cmd" : "npm", ["run", check], { cwd: root, stdio: "inherit", shell: process.platform === "win32" });
      if (result.error) throw result.error;
      if (result.status !== 0) process.exit(result.status ?? 1);
    }
  }
}

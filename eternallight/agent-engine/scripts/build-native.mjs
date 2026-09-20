import { spawnSync } from "node:child_process";
import { mkdirSync, copyFileSync, chmodSync, existsSync, readFileSync, renameSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const result = spawnSync("cargo", ["build", "--locked", "--release", "--target-dir", join(root, "target")], { cwd: root, stdio: "inherit" });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const directory = join(root, "native", "bin", `${process.platform}-${process.arch}`);
mkdirSync(directory, { recursive: true });
const library = process.platform === "win32" ? "eternallight_node_binding.dll" : process.platform === "darwin" ? "libeternallight_node_binding.dylib" : "libeternallight_node_binding.so";
const source = join(root, "target", "release", library);
const destination = join(directory, "engine.node");
if (!existsSync(destination) || !readFileSync(source).equals(readFileSync(destination))) {
  const temporary = `${destination}.${process.pid}.tmp`;
  try {
    copyFileSync(source, temporary);
    if (process.platform !== "win32") chmodSync(temporary, 0o755);
    renameSync(temporary, destination);
  } finally { rmSync(temporary, { force: true }); }
}
// Retire only this package's obsolete generated helper, never user data.
rmSync(join(directory, `eternallight-stream-guard${process.platform === "win32" ? ".exe" : ""}`), { force: true });

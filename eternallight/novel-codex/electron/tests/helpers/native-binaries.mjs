import { isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Select a current build explicitly for isolated integration runs. This does
// not select another protocol or repair an absent executable.
export function nativeTestBinary(name, environment = process.env) {
  if (!["novel-codex-story-runtime", "novel-workspace-mcp"].includes(name)) throw new Error("Unknown Native test binary");
  const directory = environment.NOVEL_CODEX_NATIVE_DIR;
  if (directory && !isAbsolute(directory)) throw new Error("NOVEL_CODEX_NATIVE_DIR must be absolute");
  const suffix = process.platform === "win32" ? ".exe" : "";
  return join(directory || (environment.CARGO_TARGET_DIR ? resolve(environment.CARGO_TARGET_DIR, "debug") : null) || fileURLToPath(new URL("../../../native/target/debug/", import.meta.url)), `${name}${suffix}`);
}

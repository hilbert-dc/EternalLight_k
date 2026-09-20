import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const productRoot = fileURLToPath(new URL("..", import.meta.url));

export function resolveBuildLayout(environment = process.env) {
  const resources = resolve(environment.NOVEL_CODEX_RELEASE_RESOURCES || resolve(productRoot, ".release", "resources"));
  const output = resolve(environment.NOVEL_CODEX_RELEASE_OUTPUT || resolve(productRoot, "release"));
  return Object.freeze({
    productRoot,
    repositoryRoot: resolve(productRoot, "../.."),
    resources,
    output,
    platform: environment.NOVEL_CODEX_TARGET_PLATFORM || process.platform,
    architecture: environment.NOVEL_CODEX_TARGET_ARCH || process.arch,
  });
}

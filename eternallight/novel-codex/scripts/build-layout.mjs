import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const productRoot = fileURLToPath(new URL("..", import.meta.url));

export function resolveBuildLayout(environment = process.env) {
  for (const name of ["NOVEL_CODEX_RELEASE_RESOURCES", "NOVEL_CODEX_RELEASE_OUTPUT"]) {
    if (environment[name]) throw new Error(`${name} 已移除；暂存固定在 .release/resources，最终产物固定在 release`);
  }
  const staging = resolve(productRoot, ".release");
  const platform = environment.NOVEL_CODEX_TARGET_PLATFORM || process.platform;
  const architecture = environment.NOVEL_CODEX_TARGET_ARCH || process.arch;
  if (!/^[a-z0-9]+$/.test(platform) || !/^[a-z0-9]+$/.test(architecture)) throw new Error("构建目标必须是平台和架构名称，不能包含路径");
  return Object.freeze({
    productRoot,
    repositoryRoot: resolve(productRoot, "../.."),
    staging,
    resources: resolve(staging, "resources"),
    mediaRuntime: resolve(staging, "media-runtime", `${platform}-${architecture}`),
    packageOutput: resolve(staging, "package"),
    output: resolve(productRoot, "release"),
    platform,
    architecture,
  });
}

// @ts-check
import { join } from "node:path";
import { existsSync } from "node:fs";
import { createServer } from "node:net";
import { spawn } from "node:child_process";
import { createRuntimeEnvironment } from "./codex-runtime.mjs";
import { startupFailure } from "./startup-coordinator.mjs";
import { waitForRuntimeReady } from "./runtime-readiness.mjs";
import { nativeRuntimeError } from "./runtime-request-error.mjs";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   readonly projectRoot: string,
 *   runtimePort: number | null,
 *   readonly configuredRuntimePort: number | null,
 *   runtimeUrl: string,
 *   runtimeError: string | null,
 *   readonly bundledRuntime: {creativeRuntime: string, novelWorkspaceMcp: string} | null,
 *   readonly runtimeAssetRoot: string,
 *   runtimeProcess: import("node:child_process").ChildProcess | null,
 *   readonly desktopLifecycle: import("./desktop-lifecycle.mjs").DesktopLifecycle,
 *   readonly runtimeCandidateToken: string,
 *   readonly runtimeAuthorToken: string,
 *   readonly mediaExecutionServer: Awaited<ReturnType<typeof import("./media-execution-server.mjs").createMediaExecutionServer>> | null,
 *   requestRuntimeStartupCheck: () => void,
 * }} dependencies
 */
export function createRuntimeService(dependencies) {
  function developmentRuntimeBinaries() {
    const suffix = process.platform === "win32" ? ".exe" : "";
    const directory = join(dependencies.projectRoot, "native", "target", "debug");
    const creativeRuntime = join(directory, `novel-codex-story-runtime${suffix}`);
    const novelWorkspaceMcp = join(directory, `novel-workspace-mcp${suffix}`);
    if (!existsSync(creativeRuntime) || !existsSync(novelWorkspaceMcp)) {
      throw new Error("Rust Story Runtime 尚未构建，请先运行 npm run build:story-runtime");
    }
  return { creativeRuntime, novelWorkspaceMcp };
  }

  function findFreeLoopbackPort() {
    return new Promise((resolvePort, reject) => {
      const probe = createServer();
      probe.once("error", reject);
      probe.listen(0, "127.0.0.1", () => {
        const address = probe.address();
        const port = typeof address === "object" && address ? address.port : null;
        probe.close((error) => error ? reject(error) : resolvePort(port));
      });
    });
  }

  async function startRuntime(runtimeDb, budgetDb, signal) {
    signal.throwIfAborted();
    dependencies.runtimePort = dependencies.configuredRuntimePort ?? await findFreeLoopbackPort();
    signal.throwIfAborted();
    dependencies.runtimeUrl = `http://127.0.0.1:${dependencies.runtimePort}`;
    dependencies.runtimeError = null;
    const runtimeCommand = dependencies.bundledRuntime.creativeRuntime;
    const runtimeArgs = ["--host", "127.0.0.1", "--port", String(dependencies.runtimePort), "--db", runtimeDb, "--engine-budget-db", budgetDb, "--token-stdin"];
    const child = spawn(runtimeCommand, runtimeArgs, {
      cwd: dependencies.runtimeAssetRoot,
      env: createRuntimeEnvironment(process.env),
      stdio: ["pipe", "pipe", "pipe"],
    });
    dependencies.runtimeProcess = child;
    child.stdin.on("error", error => {
      if (dependencies.runtimeProcess === child) {
        dependencies.runtimeError = error.message;
        dependencies.desktopLifecycle.coordinator.fail(startupFailure("本地创作服务无法接收启动信息，可以重试。", error.message));
      }
    });
    child.stdin.end(`${JSON.stringify({ candidateToken: dependencies.runtimeCandidateToken, authorToken: dependencies.runtimeAuthorToken, mediaHostUrl: dependencies.mediaExecutionServer?.url ?? null, mediaHostToken: dependencies.mediaExecutionServer?.token ?? null })}\n`);
    child.stdout.on("data", (chunk) => console.log(`[creative-runtime] ${chunk.toString().trim()}`));
    child.stderr.on("data", (chunk) => console.error(`[creative-runtime] ${chunk.toString().trim()}`));
    child.on("error", (error) => {
      if (dependencies.runtimeProcess === child) {
        dependencies.runtimeError = error.message;
        dependencies.desktopLifecycle.coordinator.fail(startupFailure("本地创作服务无法启动，可以重试。", error.message));
      }
    });
    child.on("exit", (code, signal) => {
      if (dependencies.runtimeProcess === child) {
        dependencies.runtimeError = `Creative Runtime exited (${code ?? signal ?? "unknown"})`;
        dependencies.desktopLifecycle.coordinator.fail(startupFailure("本地创作服务已退出，可以重试。", dependencies.runtimeError));
      }
    });
    await waitForRuntimeReady(child, dependencies.runtimeUrl, {
      signal, onCheckReady: check => { dependencies.requestRuntimeStartupCheck = check ?? (() => {}); },
    });
    signal.throwIfAborted();
    await runtimeRequest("/api/tasks/recover-orphans", { method: "POST", body: "{}", signal });
  }

  async function runtimeRequest(path, options = {}) {
    if (dependencies.runtimeError) throw new Error(dependencies.runtimeError);
    const response = await fetch(`${dependencies.runtimeUrl}${path}`, {
      ...options,
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${dependencies.runtimeAuthorToken}`, ...(options.headers ?? {}) },
    });
    const payload = await response.json();
    if (!response.ok) {
      const failure = nativeRuntimeError(payload);
      failure.runtimePath = path.split("?")[0];
      throw failure;
    }
    return payload;
  }

    return { developmentRuntimeBinaries, findFreeLoopbackPort, startRuntime, runtimeRequest };
}

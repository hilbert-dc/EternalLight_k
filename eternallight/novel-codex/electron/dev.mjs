// @ts-check
import { spawn } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import electronExecutable from "electron";
// Electron exports its executable path when loaded by Node, and its API inside Electron.
if (typeof electronExecutable !== "string") throw new Error("开发启动器必须由 Node 执行");
const electronPath = electronExecutable;

import { loadProjectDevelopmentEnvironment, resolveDevelopmentEnvironment, withoutProviderCredentials } from "./development-credentials.mjs";
import { availableDevelopmentPort } from "./development-server.mjs";

const projectRoot = fileURLToPath(new URL("..", import.meta.url));
const host = "127.0.0.1";
const projectEnvironment = loadProjectDevelopmentEnvironment(projectRoot, process.env);
const electronEnvironment = resolveDevelopmentEnvironment(projectEnvironment);
const rendererEnvironment = withoutProviderCredentials(projectEnvironment);

async function waitUntilReady(url) {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // Vite has not opened the loopback listener yet.
    }
    await new Promise((resolve) => setTimeout(resolve, 80));
  }
  throw new Error(`Vite did not become ready: ${url}`);
}

const port = await availableDevelopmentPort(projectEnvironment.NOVEL_CODEX_DEV_PORT);
const rendererUrl = `http://${host}:${port}`;
const vite = spawn(process.execPath, [join(projectRoot, "node_modules", "vite", "bin", "vite.js"), "--host", host, "--port", String(port), "--strictPort"], {
  cwd: projectRoot,
  env: rendererEnvironment,
  stdio: "inherit",
});

let electron = null;
let stopping = false;
function stop(exitCode = 0) {
  if (stopping) return;
  stopping = true;
  electron?.kill();
  vite.kill();
  process.exitCode = exitCode;
}

vite.once("exit", (code) => stop(code ?? 1));
process.once("SIGINT", () => stop(130));
process.once("SIGTERM", () => stop(143));

try {
  await waitUntilReady(rendererUrl);
  electron = spawn(electronPath, [join(projectRoot, "electron", "main.mjs")], {
    cwd: projectRoot,
    env: { ...electronEnvironment, NOVEL_CODEX_RENDERER_URL: rendererUrl },
    stdio: "inherit",
  });
  electron.once("exit", (code) => stop(code ?? 0));
} catch (error) {
  console.error(error);
  stop(1);
}

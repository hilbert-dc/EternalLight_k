/**
 * Create after adapter.startProcess(), before ensureSession(). Forward the
 * adapter's onEvent notifications to onEvent and await ready before model work.
 * Register dispose with test teardown to release an abandoned startup wait.
 * Only the configured server name and fixed diagnostics reach the rejection.
 */
export function createMcpStartupWaiter({ serverName, process: child, timeoutMs }) {
  if (typeof serverName !== "string" || !/^[A-Za-z0-9_-]{1,64}$/.test(serverName)) {
    throw new TypeError("MCP serverName must be a bounded server identifier");
  }
  if (!Number.isInteger(timeoutMs) || timeoutMs <= 0 || timeoutMs > 2_147_483_647) {
    throw new TypeError("MCP timeoutMs must be a positive timer-safe integer");
  }
  if (typeof child?.on !== "function" || typeof child?.off !== "function") {
    throw new TypeError("MCP startup requires the adapter child process");
  }

  let resolveReady;
  let rejectReady;
  const ready = new Promise((resolve, reject) => {
    resolveReady = resolve;
    rejectReady = reject;
  });
  // Startup can fail while the caller is still awaiting session/task creation.
  // Mark this promise handled now, retaining its rejection for the later await.
  ready.catch(() => {});

  let settled = false;
  let timer;
  function finish(code, reason) {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    child.off("error", onError);
    child.off("exit", onExit);
    if (code) {
      const error = new Error(`MCP server "${serverName}": ${reason}`);
      error.code = code;
      rejectReady(error);
    } else {
      resolveReady();
    }
  }
  function onError() {
    finish("MCP_PROCESS_ERROR", "adapter process error before ready");
  }
  function onExit() {
    finish("MCP_PROCESS_EXIT", "adapter process exited before ready");
  }
  function onEvent(event) {
    if (event?.method !== "mcpServer/startupStatus/updated" || event.params?.name !== serverName) return;
    if (event.params.status === "ready") finish();
    else if (event.params.status === "failed") finish("MCP_STARTUP_FAILED", "startup failed before ready");
  }
  function dispose() {
    finish("MCP_STARTUP_DISPOSED", "startup wait disposed before ready");
  }

  timer = setTimeout(() => {
    finish("MCP_STARTUP_TIMEOUT", `timed out after ${timeoutMs} ms waiting for ready`);
  }, timeoutMs);
  child.on("error", onError);
  child.on("exit", onExit);
  if (child.exitCode != null || child.signalCode != null) onExit();
  return { ready, onEvent, dispose };
}

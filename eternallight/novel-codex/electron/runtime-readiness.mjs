import { setTimeout as delay } from "node:timers/promises";

export async function waitForRuntimeReady(child, url, { timeoutMs = 90_000 } = {}) {
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120_000) throw new Error("Runtime 启动期限无效");
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (child.exitCode !== null || child.signalCode !== null) throw new Error(`Creative Runtime exited (${child.exitCode ?? child.signalCode})`);
    try {
      const response = await fetch(`${url}/health`, { signal: AbortSignal.timeout(Math.min(1000, Math.max(1, deadline - Date.now()))), redirect: "error" });
      const ready = response.ok;
      await response.body?.cancel();
      if (ready && child.exitCode === null && child.signalCode === null) return;
    } catch {
      // Listener startup and an unresponsive health endpoint both consume the
      // same bounded startup allowance, never an unbounded fetch wait.
    }
    const remaining = deadline - Date.now();
    if (remaining > 0) await delay(Math.min(80, remaining));
  }
  throw new Error("Creative Runtime 启动超时");
}

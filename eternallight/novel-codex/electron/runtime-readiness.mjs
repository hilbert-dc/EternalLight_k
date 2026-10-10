import { startupFailure } from "./startup-coordinator.mjs";

/** Waits for the same child until ready, explicitly failed, or cancelled. */
export async function waitForRuntimeReady(child, url, {
  signal, slowAfterMs = 90_000, onSlow = () => {}, onCheckReady = () => {},
  probeTimeoutMs = 1000, intervalMs = 80,
} = {}) {
  if (![slowAfterMs, probeTimeoutMs, intervalMs].every(value => Number.isSafeInteger(value) && value > 0)) {
    throw new Error("Runtime 启动检查配置无效");
  }
  const controller = new AbortController();
  const lifetime = signal ? AbortSignal.any([controller.signal, signal]) : controller.signal;
  const exited = () => controller.abort(startupFailure("本地创作服务已退出，可以重试。",
    `Creative Runtime exited (${child.exitCode ?? child.signalCode ?? "unknown"})`));
  const failed = error => controller.abort(startupFailure("本地创作服务无法启动，可以重试。", error?.message));
  const checkChild = () => {
    if (child.exitCode !== null || child.signalCode !== null) exited();
    lifetime.throwIfAborted();
  };
  let wake = null;
  const pause = () => new Promise((resolve, reject) => {
    const finish = error => {
      clearTimeout(timer);
      lifetime.removeEventListener("abort", cancelled);
      wake = null;
      if (error) reject(error); else resolve();
    };
    const cancelled = () => finish(lifetime.reason);
    const timer = setTimeout(() => finish(), intervalMs);
    wake = () => finish();
    lifetime.addEventListener("abort", cancelled, { once: true });
    if (lifetime.aborted) cancelled();
  });
  child.on?.("exit", exited);
  child.on?.("error", failed);
  const slowTimer = setTimeout(() => { if (!lifetime.aborted) onSlow(); }, slowAfterMs);
  onCheckReady(() => wake?.());
  try {
    for (;;) {
      checkChild();
      try {
        const response = await fetch(`${url}/health`, {
          signal: AbortSignal.any([lifetime, AbortSignal.timeout(probeTimeoutMs)]), redirect: "error",
        });
        const ready = response.ok;
        await response.body?.cancel();
        checkChild();
        if (ready) return;
      } catch {
        checkChild();
      }
      await pause();
    }
  } finally {
    clearTimeout(slowTimer);
    child.removeListener?.("exit", exited);
    child.removeListener?.("error", failed);
    onCheckReady(null);
  }
}

import { safeRuntimeDiagnostic } from "./runtime-request-error.mjs";

export function startupFailure(publicMessage, diagnostic = null) {
  return Object.assign(new Error(diagnostic ?? publicMessage), { publicMessage });
}

export function describeStartupFailure(error) {
  const message = typeof error?.publicMessage === "string"
    ? error.publicMessage : "本地服务启动失败，可以重试。";
  const diagnostic = safeRuntimeDiagnostic(error?.details ?? error?.message);
  return diagnostic && diagnostic !== message ? `${message}\n${diagnostic}` : message;
}

/** Owns one startup attempt; slow startup keeps its existing resources alive. */
export class StartupCoordinator {
  #initialize;
  #cleanup;
  #onError;
  #check;
  #slowAfterMs;
  #status = { state: "starting", mode: "local", error: null };
  #attempt = null;
  #controller = null;
  #timer = null;
  #cleaning = null;
  #closed = false;

  constructor({ initialize, cleanup, onError = () => {}, check = () => {}, slowAfterMs = 90_000 }) {
    if (typeof initialize !== "function" || typeof cleanup !== "function"
      || !Number.isSafeInteger(slowAfterMs) || slowAfterMs < 1) throw new Error("启动协调器配置无效");
    this.#initialize = initialize;
    this.#cleanup = cleanup;
    this.#onError = onError;
    this.#check = check;
    this.#slowAfterMs = slowAfterMs;
  }

  status() { return { ...this.#status }; }

  start() {
    if (this.#attempt) return this.#attempt;
    if (this.#closed || this.#status.state === "online") return Promise.resolve(this.status());
    this.#status = { state: "starting", mode: "local", error: null };
    this.#cleaning = null;
    const controller = new AbortController();
    this.#controller = controller;
    this.#timer = setTimeout(() => {
      if (!this.#closed && this.#controller === controller && this.#status.state === "starting") {
        this.#status = { state: "slow", mode: "local", error: null };
      }
    }, this.#slowAfterMs);
    const attempt = Promise.resolve().then(async () => {
      try {
        controller.signal.throwIfAborted();
        await this.#initialize({ signal: controller.signal });
        controller.signal.throwIfAborted();
        this.#status = { state: "online", mode: "local", error: null };
      } catch (error) {
        await this.#clean();
        if (!this.#closed) {
          this.#status = { state: "offline", mode: "local", error: describeStartupFailure(error) };
          this.#onError(error);
        }
      } finally {
        clearTimeout(this.#timer);
        this.#timer = null;
        if (this.#controller === controller) this.#controller = null;
        if (this.#attempt === attempt) this.#attempt = null;
      }
      return this.status();
    });
    this.#attempt = attempt;
    return attempt;
  }

  retry() {
    if (this.#closed) return this.status();
    if (this.#attempt) this.#check();
    else if (this.#status.state === "offline") void this.start();
    return this.status();
  }

  fail(error) {
    if (this.#closed) return;
    if (this.#controller) {
      this.#controller.abort(error);
      return;
    }
    if (this.#status.state !== "online") return;
    this.#status = { state: "offline", mode: "local", error: describeStartupFailure(error) };
    const attempt = this.#clean().then(() => {
      this.#onError(error);
      return this.status();
    }).finally(() => { if (this.#attempt === attempt) this.#attempt = null; });
    this.#attempt = attempt;
  }

  async stop() {
    this.#closed = true;
    clearTimeout(this.#timer);
    this.#timer = null;
    this.#controller?.abort(startupFailure("应用已关闭。"));
    await this.#clean();
  }

  #clean() {
    if (this.#cleaning) return this.#cleaning;
    const cleaning = Promise.resolve().then(() => this.#cleanup()).catch(error => {
      this.#onError(error);
    });
    this.#cleaning = cleaning;
    return cleaning;
  }
}

export function registerStartupHandlers(ipcMain, { coordinator, confirmClose, cancelClose }) {
  ipcMain.handle("runtime:status", () => coordinator.status());
  ipcMain.handle("runtime:retry-startup", () => coordinator.retry());
  ipcMain.handle("app:confirm-close", confirmClose);
  ipcMain.handle("app:cancel-close", cancelClose);
}

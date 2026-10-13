// @ts-check
import { StartupCoordinator, describeStartupFailure, registerStartupHandlers, startupFailure } from "./startup-coordinator.mjs";
import { safeSendToWebContents } from "./webcontents-safety.mjs";

/** Owns application startup and exit, independently of business IPC and execution.
 * Windows may close after author confirmation; application exit waits for cleanup.
 */
export class DesktopLifecycle {
  #started = false;
  allowWindowClose = false;
  quitRequested = false;
  shutdownComplete = false;
  /** @type {Promise<void> | null} */
  shutdownPromise = null;
  /** @type {Promise<void> | null} */
  ready = null;

  /** @param {{
   * app: Pick<import('electron').App, 'requestSingleInstanceLock' | 'quit' | 'on' | 'whenReady' | 'isPackaged'>,
   * windows: () => import('electron').BrowserWindow[],
   * registry: Pick<import('electron').IpcMain, 'handle'>,
   * initialize: (options: {signal: AbortSignal}) => Promise<void>,
   * cleanup: () => Promise<void>, check: () => void, createWindow: () => void,
   * readHealth: () => Promise<{ok?: boolean}>,
   * platform?: NodeJS.Platform, environment?: NodeJS.ProcessEnv,
   * reportError?: (error: unknown) => void, reportSmoke?: () => void
   * }} options */
  constructor({ app, windows, registry, initialize, cleanup, check, createWindow, readHealth,
    platform = process.platform, environment = process.env,
    reportError = error => console.error(`[novel-codex] startup failed: ${describeStartupFailure(error)}`),
    reportSmoke = () => console.log("NOVEL_CODEX_PACKAGE_SMOKE_OK") }) {
    this.app = app;
    this.windows = windows;
    this.registry = registry;
    this.createWindow = createWindow;
    this.readHealth = readHealth;
    this.platform = platform;
    this.environment = environment;
    this.reportError = reportError;
    this.reportSmoke = reportSmoke;
    this.coordinator = new StartupCoordinator({ initialize, cleanup, check, onError: reportError });
  }

  stop() {
    this.shutdownPromise ??= this.coordinator.stop().then(() => { this.shutdownComplete = true; });
    return this.shutdownPromise;
  }

  windowClosed() { if (!this.quitRequested) this.allowWindowClose = false; }

  async confirmClose() {
    this.allowWindowClose = true;
    if (this.quitRequested || this.coordinator.status().state !== "online") {
      await this.stop();
      this.app.quit();
    } else this.windows().forEach(window => window.close());
    return { closing: true };
  }

  cancelClose() { this.quitRequested = false; return { closing: false }; }

  async quitAfterCleanup() {
    this.allowWindowClose = true;
    await this.stop();
    this.app.quit();
  }

  start() {
    if (this.#started) return;
    this.#started = true;
    if (!this.app.requestSingleInstanceLock()) { this.app.quit(); return; }
    this.app.on("second-instance", () => {
      const window = this.windows()[0];
      if (!window) return;
      if (window.isMinimized()) window.restore();
      window.focus();
    });
    this.ready = this.app.whenReady().then(async () => {
      if (this.shutdownPromise) { await this.shutdownPromise; return; }
      registerStartupHandlers(this.registry, {
        coordinator: this.coordinator, confirmClose: () => this.confirmClose(), cancelClose: () => this.cancelClose(),
      });
      this.createWindow();
      this.app.on("activate", () => { if (this.windows().length === 0) this.createWindow(); });
      const status = await this.coordinator.start();
      if (!this.app.isPackaged || this.environment.NOVEL_CODEX_PACKAGE_SMOKE !== "1") return;
      if (status.state === "online") {
        if ((await this.readHealth())?.ok !== true) throw startupFailure("最终安装产物无法访问本地创作服务。");
        this.reportSmoke();
      }
      await this.quitAfterCleanup();
    }).catch(async error => {
      this.reportError(error);
      await this.quitAfterCleanup();
    });
    this.app.on("before-quit", event => {
      if (this.allowWindowClose) {
        if (!this.shutdownComplete) {
          event.preventDefault();
          void this.quitAfterCleanup().catch(this.reportError);
        }
        return;
      }
      this.quitRequested = true;
      event.preventDefault();
      const window = this.windows()[0];
      if (window) safeSendToWebContents(window.webContents, "app:close-requested", {});
      else void this.quitAfterCleanup().catch(this.reportError);
    });
    this.app.on("will-quit", () => { void this.stop().catch(this.reportError); });
    this.app.on("window-all-closed", () => { if (this.platform !== "darwin") this.app.quit(); });
  }
}

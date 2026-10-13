// @ts-check
import { isAllowedRendererNavigation, isTrustedIpcSender } from "./navigation-policy.mjs";
import { app, BrowserWindow, session } from "electron";
import { join } from "node:path";
import { safeSendToWebContents } from "./webcontents-safety.mjs";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   readonly rendererUrl: string,
 *   readonly packagedEntry: string,
 *   rendererWebContents: import("electron").WebContents | null,
 *   readonly __dirname: string,
 *   readonly desktopLifecycle: import("./desktop-lifecycle.mjs").DesktopLifecycle,
 *   readonly chapterStructureConsumer: ReturnType<typeof import("./automatic-work-consumers.mjs").createAutomaticWorkConsumers>["chapterStructureConsumer"],
 * }} dependencies
 */
export function createDesktopWindow(dependencies) {
  function isTrustedRendererUrl(url) {
    return isAllowedRendererNavigation(url, { isPackaged: app.isPackaged, rendererUrl: dependencies.rendererUrl, packagedEntry: dependencies.packagedEntry });
  }

  function assertTrustedIpcSender(event) {
    if (!isTrustedIpcSender(event, dependencies.rendererWebContents, { isPackaged: app.isPackaged, rendererUrl: dependencies.rendererUrl, packagedEntry: dependencies.packagedEntry })) {
      throw new Error("拒绝来自非受信 Renderer 的 IPC 调用");
    }
  }

  function createWindow() {
    const window = new BrowserWindow({
      width: 1540,
      height: 960,
      minWidth: 900,
      minHeight: 720,
      backgroundColor: "#f5f6f8",
      title: app.getName(),
      webPreferences: {
        preload: join(dependencies.__dirname, "preload.cjs"),
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
      },
    });

    const webContents = window.webContents;
    dependencies.rendererWebContents = webContents;
    window.on("closed", () => {
      dependencies.desktopLifecycle.windowClosed();
    });
    window.on("close", (event) => {
      if (dependencies.desktopLifecycle.allowWindowClose || window.webContents.isDestroyed()) return;
      event.preventDefault();
      safeSendToWebContents(window.webContents, "app:close-requested", {});
    });
    webContents.once("destroyed", () => {
      if (dependencies.rendererWebContents === webContents) { dependencies.rendererWebContents = null; dependencies.chapterStructureConsumer.select(null); }
    });
    window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
    window.webContents.on("will-navigate", (event, url) => {
      if (!isTrustedRendererUrl(url)) event.preventDefault();
    });
    window.webContents.on("will-redirect", (event, url) => {
      if (!isTrustedRendererUrl(url)) event.preventDefault();
    });

    session.defaultSession.setPermissionRequestHandler((_webContents, _permission, callback) => {
      callback(false);
    });

    if (app.isPackaged) {
      window.loadFile(dependencies.packagedEntry);
    } else {
      window.loadURL(dependencies.rendererUrl);
    }
  }

  return { isTrustedRendererUrl, assertTrustedIpcSender, createWindow };
}

const { app, BrowserWindow } = require("electron");
console.error("UI harness: starting");
app.setPath("userData", require("node:path").join(require("node:path").dirname(process.argv[2]), "user-data"));
// These DOM/screenshot fixtures do not require GPU acceleration. Keep their
// rendering backend deterministic and independent of the user's GPU session.
app.disableHardwareAcceleration();
const watchdog = setTimeout(() => { console.error("UI test timed out before completion"); app.exit(1); }, 25000);
app.whenReady().then(async () => {
  console.error("UI harness: creating window");
  const window = new BrowserWindow({ width: 1400, height: 1000, show: false, webPreferences: { backgroundThrottling: false } });
  console.error("UI harness: loading fixture");
  // Cover initial page loading too: a renderer startup failure must not leave
  // the harness waiting forever before the in-page result loop is installed.
  window.webContents.on("render-process-gone", (_event, details) => {
    console.error(`UI renderer exited: ${details.reason}`); app.exit(1);
  });
  try { await window.loadFile(process.argv[2]); }
  catch (error) { console.error(`UI page failed to load: ${error.message}`); app.exit(1); return; }
  console.error("UI harness: fixture loaded");
  let captured = false;
  let readingCaptured = false;
  let exiting = false;
  const timer = setInterval(async () => {
    if (exiting) return;
    const viewport = await window.webContents.executeJavaScript("window.testViewport");
    if (viewport) {
      window.setContentSize(viewport.width, viewport.height);
      await window.webContents.executeJavaScript("window.testViewport = null");
    }
    const screenshot = await window.webContents.executeJavaScript("window.testScreenshot");
    if (screenshot) {
      if (process.env.NOVEL_CODEX_UI_SNAPSHOT_DIR && /^[a-z0-9 -]{1,100}$/i.test(screenshot)) {
        const fs = require("node:fs"), path = require("node:path");
        fs.mkdirSync(process.env.NOVEL_CODEX_UI_SNAPSHOT_DIR, { recursive: true });
        fs.writeFileSync(path.join(process.env.NOVEL_CODEX_UI_SNAPSHOT_DIR, `${screenshot}.png`), (await window.webContents.capturePage()).toPNG());
      }
      await window.webContents.executeJavaScript("window.testScreenshot = null");
    }
    if (!captured && process.env.NOVEL_CODEX_UI_SCREENSHOT && await window.webContents.executeJavaScript("window.uiReady")) {
      captured = true;
      require("node:fs").writeFileSync(process.env.NOVEL_CODEX_UI_SCREENSHOT, (await window.webContents.capturePage()).toPNG());
    }
    if (!readingCaptured && process.env.NOVEL_CODEX_READING_SCREENSHOT && await window.webContents.executeJavaScript("window.readingUiReady")) {
      readingCaptured = true;
      require("node:fs").writeFileSync(process.env.NOVEL_CODEX_READING_SCREENSHOT, (await window.webContents.capturePage()).toPNG());
    }
    const result = await window.webContents.executeJavaScript("window.testResult");
    if (!result || exiting) return;
    exiting = true;
    clearInterval(timer); clearTimeout(watchdog); console.log(JSON.stringify(result)); app.exit(result.passed ? 0 : 1);
  }, 50);
});

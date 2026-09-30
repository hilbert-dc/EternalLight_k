const { app, BrowserWindow } = require("electron");
app.setPath("userData", require("node:path").join(require("node:path").dirname(process.argv[2]), "user-data"));
app.whenReady().then(async () => {
  const window = new BrowserWindow({ width: 1400, height: 1000, show: false, webPreferences: { backgroundThrottling: false } });
  await window.loadFile(process.argv[2]);
  let captured = false;
  let readingCaptured = false;
  const timer = setInterval(async () => {
    if (!captured && process.env.NOVEL_CODEX_UI_SCREENSHOT && await window.webContents.executeJavaScript("window.uiReady")) {
      captured = true;
      require("node:fs").writeFileSync(process.env.NOVEL_CODEX_UI_SCREENSHOT, (await window.webContents.capturePage()).toPNG());
    }
    if (!readingCaptured && process.env.NOVEL_CODEX_READING_SCREENSHOT && await window.webContents.executeJavaScript("window.readingUiReady")) {
      readingCaptured = true;
      require("node:fs").writeFileSync(process.env.NOVEL_CODEX_READING_SCREENSHOT, (await window.webContents.capturePage()).toPNG());
    }
    const result = await window.webContents.executeJavaScript("window.testResult");
    if (!result) return;
    clearInterval(timer); console.log(JSON.stringify(result)); app.exit(result.passed ? 0 : 1);
  }, 50);
  setTimeout(() => { console.error("UI test timed out"); app.exit(1); }, 25000);
});

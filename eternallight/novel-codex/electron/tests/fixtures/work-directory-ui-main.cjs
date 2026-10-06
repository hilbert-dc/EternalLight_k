const { app, BrowserWindow } = require("electron");
const path = require("node:path"), fs = require("node:fs");
app.setPath("userData", path.join(path.dirname(process.argv[2]), "user-data"));
app.whenReady().then(async () => {
  const window = new BrowserWindow({ width: 1400, height: 1000, show: false, webPreferences: { backgroundThrottling: false } });
  await window.loadFile(process.argv[2]);
  let polling = false;
  const timer = setInterval(async () => {
    if (polling) return;
    polling = true;
    try {
      const state = await window.webContents.executeJavaScript("({key:window.testKey,pointer:window.testPointer,screenshot:window.testScreenshot,result:window.testResult})");
      if (state.pointer) { window.webContents.sendInputEvent(state.pointer); await window.webContents.executeJavaScript("window.testPointer=null"); }
      if (state.key) {
        state.key.keyCode = { ArrowDown: "Down", ArrowUp: "Up" }[state.key.keyCode] ?? state.key.keyCode;
        window.webContents.sendInputEvent({ type: "keyDown", ...state.key });
        if (state.key.keyCode === "Return" || state.key.keyCode === "Space") window.webContents.sendInputEvent({ type: "char", keyCode: state.key.keyCode === "Return" ? "\r" : " " });
        window.webContents.sendInputEvent({ type: "keyUp", ...state.key });
        await window.webContents.executeJavaScript("window.testKey=null");
      }
      if (state.screenshot) {
        if (process.env.NOVEL_CODEX_UI_SNAPSHOT_DIR && /^[a-z0-9 -]{1,100}$/i.test(state.screenshot)) {
          fs.mkdirSync(process.env.NOVEL_CODEX_UI_SNAPSHOT_DIR, { recursive: true });
          fs.writeFileSync(path.join(process.env.NOVEL_CODEX_UI_SNAPSHOT_DIR, `${state.screenshot}.png`), (await window.webContents.capturePage()).toPNG());
        }
        await window.webContents.executeJavaScript("window.testScreenshot=null");
      }
      if (state.result) { clearInterval(timer); console.log(JSON.stringify(state.result)); app.exit(state.result.passed ? 0 : 1); }
    } finally { polling = false; }
  }, 20);
  setTimeout(() => { console.error("Work directory UI timed out"); app.exit(1); }, 25000);
});

const { app, BrowserWindow } = require("electron");
const path = require("node:path");
app.setPath("userData", path.join(path.dirname(process.argv[2]), "user-data"));
app.whenReady().then(async () => {
  const window = new BrowserWindow({ width: 1400, height: 1000, show: false, webPreferences: { backgroundThrottling: false } });
  await window.loadFile(process.argv[2]);
  let polling = false;
  const timer = setInterval(async () => {
    if (polling) return;
    polling = true;
    try {
      const control = await window.webContents.executeJavaScript("({key:window.testKey,viewport:window.testViewport,result:window.testResult})");
      if (control.viewport) { window.setContentSize(control.viewport.width, control.viewport.height); await window.webContents.executeJavaScript("window.testViewport=null"); }
      if (control.key) {
        window.webContents.sendInputEvent({ type: "keyDown", ...control.key });
        window.webContents.sendInputEvent({ type: "keyUp", ...control.key });
        await window.webContents.executeJavaScript("window.testKey=null");
      }
      if (control.result) { clearInterval(timer); console.log(JSON.stringify(control.result)); app.exit(control.result.passed ? 0 : 1); }
    } finally { polling = false; }
  }, 20);
  setTimeout(() => { console.error("Project information UI timed out"); app.exit(1); }, 20000);
});

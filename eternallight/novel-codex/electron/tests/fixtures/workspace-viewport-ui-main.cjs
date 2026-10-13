const { app, BrowserWindow } = require("electron");
const path = require("node:path");
app.setPath("userData", path.join(path.dirname(process.argv[2]), "user-data"));
app.whenReady().then(async () => {
  try {
    const window = new BrowserWindow({ show: false, webPreferences: { backgroundThrottling: false } });
    await window.loadFile(process.argv[2]);
    const checks = [];
    for (const [width, height] of [[1400, 1000], [1100, 760], [900, 600]]) {
      window.setContentSize(width, height);
      for (const mode of ["light", "dark"]) checks.push(await window.webContents.executeJavaScript(`window.checkViewport(${JSON.stringify(mode)})`));
    }
    console.log(JSON.stringify({ passed: true, checks }));
    app.exit(0);
  } catch (error) {
    console.log(JSON.stringify({ passed: false, error: error.stack }));
    app.exit(1);
  }
});
setTimeout(() => { console.error("Viewport test timed out"); app.exit(1); }, 35000);

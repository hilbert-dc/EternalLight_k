const { app, BrowserWindow, protocol, ipcMain } = require("electron");
const { join } = require("node:path");
const { pathToFileURL } = require("node:url");
protocol.registerSchemesAsPrivileged([{ scheme: "creative-media", privileges: { standard: true, secure: true, supportFetchAPI: true, stream: true } }]);
app.disableHardwareAcceleration(); app.on("window-all-closed", () => {});
app.whenReady().then(async () => {
  const html = process.argv[2], root = process.argv[3], directory = process.argv[4];
  const { MultimediaHost } = await import(pathToFileURL(join(root, "media-host.mjs")));
  const host = new MultimediaHost({ directory, connections: {}, executables: { fontPath: process.env.NOVEL_CODEX_MEDIA_FONT }, runtimeRequest: () => { throw Error("Font preview cannot access a story workspace"); } });
  let fontRequests = 0;
  protocol.handle("creative-media", request => { fontRequests++; return host.preview(request); });
  ipcMain.handle("test:subtitle-font", () => host.subtitleFont());
  const window = new BrowserWindow({ show: false, width: 400, height: 400, webPreferences: { preload: join(directory, "preload.cjs"), sandbox: true, contextIsolation: true, nodeIntegration: false } });
  const errors = [];
  window.webContents.on("console-message", (_event, _level, message) => { if (message.includes("Content Security Policy") || message.includes("CORS")) errors.push(message); });
  await window.loadFile(html); const deadline = Date.now() + 40_000; let proof = null;
  while (Date.now() < deadline) {
    if (!proof && await window.webContents.executeJavaScript("window.previewReady")) {
      const debuggerAdapter = window.webContents.debugger; debuggerAdapter.attach("1.3");
      await debuggerAdapter.sendCommand("DOM.enable"); await debuggerAdapter.sendCommand("CSS.enable");
      const rootNode = await debuggerAdapter.sendCommand("DOM.getDocument");
      const { nodeId } = await debuggerAdapter.sendCommand("DOM.querySelector", { nodeId: rootNode.root.nodeId, selector: ".mm-preview-caption" });
      proof = await debuggerAdapter.sendCommand("CSS.getPlatformFontsForNode", { nodeId }); debuggerAdapter.detach();
      await window.webContents.executeJavaScript("window.fontProofReceived=true");
    }
    const result = await window.webContents.executeJavaScript("window.testResult");
    if (result) { if (!result.passed) throw Error(JSON.stringify({ ...result, errors })); process.stdout.write(JSON.stringify({ ...result, fontRequests, fonts: proof.fonts })); app.quit(); return; }
    await new Promise(resolve => setTimeout(resolve, 30));
  }
  throw Error(`Preview test timed out: ${errors.join("; ")}`);
}).catch(error => { process.stderr.write(String(error.stack)); app.exit(1); });

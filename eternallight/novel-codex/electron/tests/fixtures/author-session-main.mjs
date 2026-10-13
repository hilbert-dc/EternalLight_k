import { app, BrowserWindow, ipcMain } from "electron";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { AuthorSessionStore, registerAuthorSessionHandlers } from "../../author-session-store.mjs";
import { isTrustedIpcSender } from "../../navigation-policy.mjs";

const [directory, pageUrl, command] = process.argv.slice(2);
app.setPath("userData", join(directory, "chromium"));
// Do not top-level await app readiness in an ESM entry: Electron must finish
// evaluating this module before it can emit ready.
app.whenReady().then(async () => {
const window = new BrowserWindow({ show: false, webPreferences: { preload: fileURLToPath(new URL("../../preload.cjs", import.meta.url)), contextIsolation: true, sandbox: true, nodeIntegration: false } });
const store = new AuthorSessionStore(join(app.getPath("userData"), "author-session.json"));
registerAuthorSessionHandlers({ handle(channel, handler) {
  ipcMain.handle(channel, (event, ...args) => {
    if (!isTrustedIpcSender(event, window.webContents, { isPackaged: pageUrl.startsWith("file:"), rendererUrl: pageUrl, packagedEntry: join(directory, "index.html") })) throw new Error("untrusted sender");
    return handler(event, ...args);
  });
} }, store);
const deadline = setTimeout(() => { console.error("author session fixture deadline"); app.exit(1); }, 25_000);
try {
  await window.loadURL(pageUrl);
  const result = await window.webContents.executeJavaScript(`(async () => {
    const api = window.novelAuthorSession;
    if (!api || typeof window.require !== 'undefined') throw Error('isolated preload missing');
    const before = await api.read();
    const command = ${JSON.stringify(command)};
    const after = command === 'save' ? await api.save({ name: '持久作者', email: 'author@example.invalid', passwordSalt: 'synthetic-test-salt', passwordHash: 'b'.repeat(64) })
      : command === 'logout' ? await api.signOut() : command === 'clear' ? await api.clear() : before;
    return { before, after, origin: location.origin };
  })()`);
  console.log("AUTHOR_RESULT:" + JSON.stringify(result));
} catch (error) { console.error(error); app.exitCode = 1; }
finally { clearTimeout(deadline); window.destroy(); app.exit(app.exitCode ?? 0); }
}).catch(error => { console.error(error); app.exit(1); });

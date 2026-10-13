import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { promisify } from "node:util";
import test from "node:test";
import electron from "electron";

test("real preload restores author across new processes, changed HTTP origins and file entry, preserving logout", { timeout: 120_000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-author-desktop-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const html = '<html><body>本机会话验收</body></html>';
  const page = join(directory, "index.html");
  await writeFile(page, html);
  async function origin() {
    const server = createServer((_request, response) => { response.writeHead(200, { "content-type": "text/html" }); response.end(html); });
    await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
    t.after(() => new Promise(resolve => server.close(resolve)));
    return `http://127.0.0.1:${server.address().port}/`;
  }
  const firstOrigin = await origin(), nextOrigin = await origin(), fileEntry = pathToFileURL(page).href;
  async function launch(url, command = "read") {
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/author-session-main.mjs", import.meta.url)), directory, url, command], { timeout: 30_000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const output = stdout.split("\n").find(line => line.startsWith("AUTHOR_RESULT:"));
    assert.ok(output, stdout);
    return JSON.parse(output.slice("AUTHOR_RESULT:".length));
  }
  const created = await launch(firstOrigin, "save");
  assert.deepEqual(created.before, { profile: null, signedIn: false });
  assert.equal(created.after.signedIn, true);
  assert.deepEqual((await launch(firstOrigin)).before, created.after);
  const differentOrigin = await launch(nextOrigin);
  assert.notEqual(differentOrigin.origin, created.origin);
  assert.deepEqual(differentOrigin.before, created.after);
  assert.deepEqual((await launch(fileEntry)).before, created.after);
  const loggedOut = await launch(fileEntry, "logout");
  assert.deepEqual(loggedOut.after, { profile: created.after.profile, signedIn: false });
  assert.deepEqual((await launch(nextOrigin)).before, loggedOut.after);
  await launch(fileEntry, "clear");
  assert.deepEqual((await launch(firstOrigin)).before, { profile: null, signedIn: false });
});

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createMediaExecutionServer } from "../media-execution-server.mjs";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";

// Exercise the real Host endpoint and Native startup contract. No creative
// content, provider connection or production database enters this test.
test("current Host callback permits Native startup and retains strict routing and authentication", { timeout: 60000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-autonomous-startup-"));
  const runtimeCalls = [], children = [];
  const callback = await createMediaExecutionServer({
    databasePath: join(directory, "engine.sqlite3"),
    runtimeRequest: async (...args) => { runtimeCalls.push(args); throw Error("unexpected domain call"); },
    multimediaHost: { connections: { resolve: async () => { throw Error("no provider in startup test"); } } },
    getExecution: () => null,
  });
  t.after(async () => {
    for (const child of children) {
      if (child.exitCode === null && child.signalCode === null) {
        const stopped = new Promise(resolve => child.once("exit", resolve));
        child.kill(); await stopped;
      }
    }
    await callback.close();
    await rm(directory, { recursive: true, force: true });
  });
  const endpoint = new URL(callback.url);
  assert.equal(endpoint.protocol, "http:");
  assert.equal(endpoint.hostname, "127.0.0.1");
  assert.equal(endpoint.pathname, "/media/execute");
  assert.ok(endpoint.port && callback.token.length >= 24);
  const post = (url, token) => fetch(url, { method: "POST", headers: {
    authorization: `Bearer ${token}`, "content-type": "application/json",
  }, body: JSON.stringify({}) });
  assert.equal((await post(endpoint.origin, callback.token)).status, 403);
  assert.equal((await post(callback.url, "wrong-callback-token")).status, 403);
  const routed = await post(callback.url, callback.token);
  assert.equal(routed.status, 409);
  assert.match((await routed.json()).error, /媒体执行请求无效/);
  assert.deepEqual(runtimeCalls, [], "invalid controls must not execute domain or provider actions");

  const launch = (label, mediaHostUrl) => {
    const child = spawn(nativeTestBinary("novel-codex-story-runtime"), ["--port", "0",
      "--db", join(directory, `${label}.sqlite3`), "--engine-budget-db", join(directory, "engine.sqlite3"), "--token-stdin"],
    { stdio: ["pipe", "pipe", "pipe"] });
    children.push(child);
    child.stdin.end(`${JSON.stringify({ authorToken: "startup-author-token-123456789", candidateToken: "startup-candidate-token-123456789",
      mediaHostUrl, mediaHostToken: callback.token })}\n`);
    return child;
  };
  const ready = launch("ready", callback.url);
  const origin = await new Promise((resolve, reject) => {
    let output = "", diagnostic = "";
    const timer = setTimeout(() => reject(Error(`Native did not start: ${diagnostic.slice(-1000)}`)), 20000);
    ready.stderr.on("data", chunk => { diagnostic += chunk; });
    ready.stdout.on("data", chunk => {
      output += chunk;
      const address = /Story Runtime listening on (http:\/\/127\.0\.0\.1:\d+)/.exec(output)?.[1];
      if (address) { clearTimeout(timer); resolve(address); }
    });
    ready.once("error", error => { clearTimeout(timer); reject(error); });
    ready.once("exit", code => { clearTimeout(timer); reject(Error(`Native startup exited ${code}: ${diagnostic.slice(-1000)}`)); });
  });
  assert.equal((await fetch(`${origin}/health`)).status, 200);
  const rejected = launch("wrong-path", endpoint.origin);
  let diagnostic = "";
  rejected.stderr.on("data", chunk => { diagnostic += chunk; });
  const code = await new Promise((resolve, reject) => { rejected.once("exit", resolve); rejected.once("error", reject); });
  assert.equal(code, 1);
  assert.match(diagnostic, /invalid media Host callback/);
});

import { test } from "node:test";
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { DesktopLifecycle } from "../desktop-lifecycle.mjs";
function fixture(overrides = {}) {
  const app = new EventEmitter();
  let quits = 0, creations = 0, cleanups = 0;
  Object.assign(app, { isPackaged: false, requestSingleInstanceLock: () => true,
    whenReady: () => Promise.resolve(), quit: () => { quits++; } });
  const handlers = new Map(), sent = [], failures = [];
  const window = { isMinimized: () => false, focus() {}, close() {},
    webContents: { isDestroyed: () => false, send: (...args) => sent.push(args) } };
  const lifecycle = new DesktopLifecycle({ app, windows: () => [window], registry: { handle: (name, fn) => handlers.set(name, fn) },
    initialize: async () => {}, cleanup: async () => { cleanups++; }, check() {},
    createWindow() { creations++; }, readHealth: async () => ({ ok: true }),
    reportError: error => failures.push(error), ...overrides });
  return { app, lifecycle, handlers, window, sent, failures, counts: () => ({ quits, creations, cleanups }) };
}
test("exit waits for one cleanup and author cancellation leaves execution alive", async () => {
  let release;
  const f = fixture({ cleanup: () => new Promise(resolve => { release = resolve; }) });
  f.lifecycle.start(); await f.lifecycle.ready;
  let prevented = 0;
  f.app.emit("before-quit", { preventDefault() { prevented++; } });
  assert.equal(f.lifecycle.quitRequested, true);
  assert.equal(f.sent[0][0], "app:close-requested");
  assert.deepEqual(f.handlers.get("app:cancel-close")(), { closing: false });
  assert.equal(f.lifecycle.quitRequested, false);
  assert.equal(f.lifecycle.shutdownPromise, null);
  f.app.emit("before-quit", { preventDefault() { prevented++; } });
  const closing = f.handlers.get("app:confirm-close")(); await Promise.resolve();
  assert.equal(f.counts().quits, 0);
  assert.equal(f.lifecycle.stop(), f.lifecycle.shutdownPromise);
  release(); await closing;
  assert.equal(f.lifecycle.shutdownComplete, true);
  assert.equal(f.counts().quits, 1); assert.equal(prevented, 2);
});
test("quit before Electron readiness cannot create a late window or start resources", async () => {
  let ready, initialized = 0;
  const f = fixture({ initialize: async () => { initialized++; } });
  f.app.whenReady = () => new Promise(resolve => { ready = resolve; });
  f.lifecycle.start(); await f.lifecycle.quitAfterCleanup(); ready(); await f.lifecycle.ready;
  assert.equal(initialized, 0);
  assert.deepEqual(f.counts(), { quits: 1, creations: 0, cleanups: 1 });
});
test("bootstrap installs once and startup exceptions clean resources before exit", async () => {
  const f = fixture(); f.app.whenReady = async () => { throw Error("desktop not ready"); };
  f.lifecycle.start(); f.lifecycle.start(); await f.lifecycle.ready;
  assert.equal(f.failures[0].message, "desktop not ready");
  assert.deepEqual(f.counts(), { quits: 1, creations: 0, cleanups: 1 });
  assert.equal(f.app.listenerCount("before-quit"), 1);
});
test("packaged smoke reports success only after live health and then cleans up", async () => {
  let smoke = 0;
  const f = fixture({ environment: { NOVEL_CODEX_PACKAGE_SMOKE: "1" }, reportSmoke: () => { smoke++; } });
  f.app.isPackaged = true; f.lifecycle.start(); await f.lifecycle.ready;
  assert.equal(smoke, 1); assert.deepEqual(f.counts(), { quits: 1, creations: 1, cleanups: 1 });
});

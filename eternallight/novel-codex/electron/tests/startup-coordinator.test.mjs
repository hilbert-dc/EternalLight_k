import assert from "node:assert/strict";
import { setTimeout as delay } from "node:timers/promises";
import test from "node:test";
import { StartupCoordinator, describeStartupFailure, registerStartupHandlers, startupFailure } from "../startup-coordinator.mjs";

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes,no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

test("slow startup and repeated retries retain one attempt, with recovery and configuration before online", async () => {
  const boot = deferred(), recovery = deferred(), events = [];
  let starts = 0, checks = 0, cleanups = 0;
  const host = new StartupCoordinator({slowAfterMs:15,check:() => { checks += 1; },cleanup:() => { cleanups += 1; },
    initialize:async () => { starts += 1; await boot.promise; events.push("ready"); await recovery.promise; events.push("recovered","configured"); }});
  const completion = host.start();
  assert.equal(host.status().state,"starting");
  assert.equal(host.start(),completion);
  await delay(30);
  assert.equal(host.status().state,"slow");
  for (let index=0;index<10;index+=1) assert.equal(host.retry().state,"slow");
  assert.equal(starts,1);
  assert.equal(checks,10);
  assert.equal(cleanups,0);
  boot.resolve();
  await delay(0);
  assert.equal(host.status().state,"slow");
  recovery.resolve();
  assert.deepEqual(await completion,{state:"online",mode:"local",error:null});
  assert.deepEqual(events,["ready","recovered","configured"]);
  assert.equal(host.retry().state,"online");
  assert.equal(starts,1);
  await host.stop();
  assert.equal(cleanups,1);
});

test("explicit failure cleans up before a single-flight retry, and retry IPC returns immediately", async () => {
  const next = deferred(), retired = deferred();
  let starts = 0, cleanups = 0;
  const host = new StartupCoordinator({slowAfterMs:500,initialize:async () => {
    starts += 1;
    if (starts === 1) throw startupFailure("本地服务已退出，可以重试。","process exit 7");
    await next.promise;
  },cleanup:async () => { cleanups += 1; if (cleanups === 1) await retired.promise; }});
  const first = host.start();
  await delay(0);
  host.retry();
  assert.equal(starts,1);
  retired.resolve();
  assert.equal((await first).state,"offline");
  assert.match(host.status().error,/process exit 7/);
  assert.equal(host.retry().state,"starting");
  for (let index=0;index<10;index+=1) host.retry();
  const second = host.start();
  await delay(0);
  assert.equal(starts,2);
  assert.equal(cleanups,1);
  next.resolve();
  assert.equal((await second).state,"online");
  await host.stop();
});

test("closing startup cancels its wait, retires child and bridge once, and cannot publish late online", async () => {
  const stopped = deferred();
  let signal, childStopped = 0, bridgeStopped = 0;
  const host = new StartupCoordinator({slowAfterMs:15,initialize:({signal:startupSignal}) => {
    signal = startupSignal;
    return new Promise((_resolve,reject) => signal.addEventListener("abort",() => reject(signal.reason),{once:true}));
  },cleanup:async () => { childStopped += 1; bridgeStopped += 1; await stopped.promise; }});
  const waiting = host.start();
  await delay(30);
  const closing = host.stop();
  const secondClose = host.stop();
  assert.equal(signal.aborted,true);
  await delay(0);
  assert.equal(childStopped,1);
  assert.equal(bridgeStopped,1);
  let closed = false;
  void closing.then(() => { closed = true; });
  await delay(0);
  assert.equal(closed,false,"close returned before owned resources stopped");
  stopped.resolve();
  await Promise.all([closing,secondClose,waiting]);
  assert.notEqual(host.status().state,"online");
  host.retry(); await host.start();
  assert.equal(childStopped,1);
  assert.equal(bridgeStopped,1);
});

test("an explicit child failure after readiness becomes offline and can restart without replacing handlers", async () => {
  let starts = 0, cleanups = 0;
  const host = new StartupCoordinator({initialize:async () => { starts += 1; },cleanup:() => { cleanups += 1; }});
  await host.start();
  host.fail(startupFailure("本地创作服务已退出，可以重试。","exit 1"));
  assert.equal(host.status().state,"offline");
  await delay(0);
  assert.equal(cleanups,1);
  host.retry();
  await host.start();
  assert.equal(starts,2);
  assert.equal(host.status().state,"online");
  await host.stop();
});

test("startup diagnostics are bounded and redact credentials, without inferring public text from error strings", () => {
  const error = new Error(`network refused sk-${"a".repeat(32)} Bearer secret-value ${"x".repeat(4000)}`);
  const message = describeStartupFailure(error);
  assert.match(message,/^本地服务启动失败，可以重试。/);
  assert.doesNotMatch(message,/sk-a|secret-value/);
  assert.ok(message.length < 2100);
  assert.match(describeStartupFailure(startupFailure("本地创作服务已退出，可以重试。","exit 8")),/^本地创作服务已退出/);
});

test("bootstrap IPC is usable before initialization, does not block retries, and delegates close cancellation", async () => {
  const boot = deferred(), handlers = new Map();
  let confirmed = 0, cancelled = 0;
  const host = new StartupCoordinator({initialize:() => boot.promise,cleanup:() => {}});
  registerStartupHandlers({handle:(channel,handler) => {
    assert.equal(handlers.has(channel),false); handlers.set(channel,handler);
  }},{coordinator:host,confirmClose:() => { confirmed += 1; },cancelClose:() => { cancelled += 1; }});
  assert.deepEqual([...handlers.keys()],["runtime:status","runtime:retry-startup","app:confirm-close","app:cancel-close"]);
  assert.equal(handlers.get("runtime:status")().state,"starting");
  const completion = host.start();
  assert.equal(handlers.get("runtime:retry-startup")().state,"starting");
  handlers.get("app:cancel-close")(); handlers.get("app:confirm-close")();
  assert.equal(confirmed,1); assert.equal(cancelled,1);
  boot.resolve(); await completion;
  assert.equal(handlers.get("runtime:status")().state,"online");
  await host.stop();
});

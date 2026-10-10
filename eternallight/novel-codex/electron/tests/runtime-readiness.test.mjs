import assert from "node:assert/strict";
import { createServer } from "node:http";
import { EventEmitter } from "node:events";
import { setTimeout as delay } from "node:timers/promises";
import test from "node:test";
import { waitForRuntimeReady } from "../runtime-readiness.mjs";

async function endpoint(t, respond) {
  const server = createServer(respond);
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  t.after(async () => {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  });
  return `http://127.0.0.1:${server.address().port}`;
}

function child() { return Object.assign(new EventEmitter(), { exitCode: null, signalCode: null }); }

test("Runtime becomes ready after transient health failures", async t => {
  let requests = 0;
  const url = await endpoint(t, (_request,response) => {
    response.writeHead(++requests < 3 ? 503 : 200);
    response.end("ok");
  });
  await waitForRuntimeReady(child(),url);
  assert.equal(requests,3);
});

test("slow startup keeps the same child alive and can become ready after the soft deadline", async t => {
  let ready = false, slow = 0, killed = 0;
  const process = child();
  process.kill = () => { killed += 1; };
  const url = await endpoint(t, (_request, response) => {
    response.writeHead(ready ? 200 : 503); response.end();
  });
  const waiting = waitForRuntimeReady(process,url,{slowAfterMs:25,intervalMs:10,onSlow:() => { slow += 1; }});
  await delay(70);
  assert.equal(slow,1);
  assert.equal(killed,0);
  ready = true;
  await waiting;
  assert.equal(slow,1);
  assert.equal(killed,0);
  assert.equal(process.listenerCount("error"),0);
  assert.equal(process.listenerCount("exit"),0);
});

test("a hanging health endpoint remains bounded per probe and can be cancelled promptly", async t => {
  let requests = 0;
  const url = await endpoint(t, () => { requests += 1; });
  const controller = new AbortController();
  const started = Date.now();
  const waiting = waitForRuntimeReady(child(),url,{signal:controller.signal,probeTimeoutMs:20,intervalMs:10});
  const rejected = assert.rejects(waiting,/cancel startup/);
  await delay(100);
  assert.ok(requests > 1,"one unresponsive probe blocked further checks");
  controller.abort(new Error("cancel startup"));
  await rejected;
  assert.ok(Date.now()-started < 1000);
});

test("an exited Runtime is never treated as ready even if its old endpoint returns success", async t => {
  const process = child();
  const url = await endpoint(t, (_request,response) => {
    process.signalCode = "SIGTERM";
    response.end("ok");
  });
  await assert.rejects(waitForRuntimeReady(process,url),/Runtime exited/);
});

test("spawn errors and child exits cancel an outstanding health fetch without waiting for its timeout", async t => {
  for (const event of ["error", "exit"]) {
    const process = child();
    const url = await endpoint(t, () => {});
    const started = Date.now();
    const waiting = waitForRuntimeReady(process,url,{probeTimeoutMs:5000});
    const rejected = assert.rejects(waiting, event === "error" ? /spawn denied/ : /Runtime exited/);
    await delay(20);
    if (event === "error") process.emit("error",new Error("spawn denied"));
    else { process.exitCode = 1; process.emit("exit",1,null); }
    await rejected;
    assert.ok(Date.now()-started < 1000);
    assert.equal(process.listenerCount("error"),0);
    assert.equal(process.listenerCount("exit"),0);
  }
});

test("retrying a startup check wakes its existing loop and never starts concurrent health probes", async t => {
  let checkNow, requests = 0, ready = false, cleared = false;
  const url = await endpoint(t, (_request,response) => {
    requests += 1; response.writeHead(ready ? 200 : 503); response.end();
  });
  const waiting = waitForRuntimeReady(child(),url,{intervalMs:5000,onCheckReady:check => {
    if (check) checkNow = check; else cleared = true;
  }});
  await delay(30);
  assert.equal(requests,1);
  ready = true;
  checkNow(); checkNow(); checkNow();
  await waiting;
  assert.equal(requests,2);
  assert.equal(cleared,true);
});

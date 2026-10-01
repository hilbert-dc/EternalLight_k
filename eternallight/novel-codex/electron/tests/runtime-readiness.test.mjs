import assert from "node:assert/strict";
import { createServer } from "node:http";
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

test("Runtime becomes ready after transient health failures within its startup deadline", async t => {
  let requests = 0;
  const url = await endpoint(t, (_request,response) => {
    response.writeHead(++requests < 3 ? 503 : 200);
    response.end("ok");
  });
  await waitForRuntimeReady({exitCode:null,signalCode:null},url,{timeoutMs:2000});
  assert.equal(requests,3);
});

test("a hanging health endpoint cannot escape the Runtime startup deadline", async t => {
  const url = await endpoint(t, () => {});
  const started = Date.now();
  await assert.rejects(waitForRuntimeReady({exitCode:null,signalCode:null},url,{timeoutMs:100}),/启动超时/);
  assert.ok(Date.now()-started < 2000,"health fetch did not obey the startup deadline");
});

test("an exited Runtime is never treated as ready even if its old endpoint returns success", async t => {
  const child = {exitCode:null,signalCode:null};
  const url = await endpoint(t, (_request,response) => {
    child.signalCode = "SIGTERM";
    response.end("ok");
  });
  await assert.rejects(waitForRuntimeReady(child,url,{timeoutMs:2000}),/Runtime exited/);
});

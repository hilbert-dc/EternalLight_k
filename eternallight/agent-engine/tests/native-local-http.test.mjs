import assert from "node:assert/strict";
import { test } from "node:test";
import { request as httpRequest } from "node:http";
import { LocalHttpServer } from "../src/bridge/local-http.mjs";

const options = { port: 0, token: "test-token", maxBodyBytes: 128, maxConcurrentRequests: 2, requestTimeoutMs: 1000 };
const send = (server, body = "{}") => fetch(`http://127.0.0.1:${server.address().port}/responses`, { method: "POST", headers: { authorization: "Bearer test-token" }, body });

test("Rust listener rejects oversized uploads and cancels incomplete uploads without closing the port", async (t) => {
  let dispatched = 0;
  const server = new LocalHttpServer(options, (_request, response) => { dispatched++; response.writeHead(200, {}); response.end("ok"); });
  t.after(() => server.close());
  assert.equal((await send(server, "x".repeat(129))).status, 413);
  assert.equal(dispatched, 0);
  let upload;
  const pending = new Promise((resolve, reject) => {
    upload = httpRequest({ hostname: "127.0.0.1", port: server.address().port, path: "/responses", method: "POST", headers: { authorization: "Bearer test-token", "content-length": "100" } }, response => { response.resume(); response.once("end", () => resolve(response.statusCode)); });
    upload.on("error", reject);
    upload.write("{");
  });
  // Repeated cancellation also covers the upload arriving after the first tick.
  const cancellation = setInterval(() => server.cancelUploads(), 5);
  try { assert.equal(await pending, 499); } finally { clearInterval(cancellation); upload.destroy(); }
  assert.equal(await (await send(server)).text(), "ok");
  assert.equal(dispatched, 1);
});

test("Rust listener abort is a transport failure, never successful stream EOF", async (t) => {
  let response;
  const server = new LocalHttpServer(options, (_request, target) => {
    response = target; target.writeHead(200, { "content-type": "text/event-stream" }); target.write("data: pending\n\n");
  });
  t.after(() => server.close());
  const client = await send(server);
  const reader = client.body.getReader();
  assert.equal((await reader.read()).done, false);
  response.destroy();
  await assert.rejects(reader.read());
});

test("Rust listener response queue drains without losing frames", async (t) => {
  const frames = Array.from({ length: 40 }, (_, i) => `${i}:` + "字".repeat(2048));
  const server = new LocalHttpServer(options, async (_request, response) => {
    response.writeHead(200, {});
    for (const frame of frames) {
      if (!response.write(frame)) await new Promise((resolve, reject) => { response.once("drain", resolve); response.once("close", () => reject(new Error("closed"))); });
    }
    response.end();
  });
  t.after(() => server.close());
  assert.equal(await (await send(server)).text(), frames.join(""));
});

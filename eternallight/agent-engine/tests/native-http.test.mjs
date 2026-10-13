import assert from "node:assert/strict";
import { test } from "node:test";
import { createServer } from "node:http";
import { once } from "node:events";
import { nativeFetch } from "../src/http-transport.mjs";

async function server(t, handler) {
  const server = createServer(handler);
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  t.after(() => new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }));
  return `http://127.0.0.1:${server.address().port}`;
}

test("Rust HTTP streams Unicode bytes and preserves request and response metadata", async t => {
  const url = await server(t, async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    response.writeHead(201, { "content-type": "application/json", "x-engine-test": "native" });
    const bytes = Buffer.from(JSON.stringify({ method: request.method, authorization: request.headers.authorization, body: Buffer.concat(chunks).toString(), text: "故事🌙" }));
    for (const byte of bytes) response.write(Buffer.from([byte]));
    response.end();
  });
  const response = await nativeFetch(url, { method: "POST", headers: { authorization: "Bearer fixture-only" }, body: "hello" });
  assert.equal(response.status, 201);
  assert.equal(response.headers.get("x-engine-test"), "native");
  assert.deepEqual(await response.json(), { method: "POST", authorization: "Bearer fixture-only", body: "hello", text: "故事🌙" });
});

test("Rust HTTP bounds declared and streaming bodies", async t => {
  const url = await server(t, (request, response) => {
    if (request.url === "/declared") response.writeHead(200, { "content-length": "2048" });
    else response.writeHead(200, { "transfer-encoding": "chunked" });
    response.end("x".repeat(2048));
  });
  await assert.rejects(nativeFetch(`${url}/declared`, { maxBytes: 1024 }), /exceeds bridge limit/);
  const response = await nativeFetch(`${url}/stream`, { maxBytes: 1024 });
  await assert.rejects(response.text(), /exceeds bridge limit/);
});

test("Rust HTTP cancels before headers, midstream and on consumer cancellation", async t => {
  const disconnected = [];
  const url = await server(t, (request, response) => {
    request.on("close", () => disconnected.push(request.url));
    if (request.url !== "/headers") { response.writeHead(200); response.write("prefix"); }
  });
  const before = new AbortController();
  const pending = nativeFetch(`${url}/headers`, { signal: before.signal });
  setTimeout(() => before.abort(), 40);
  await assert.rejects(pending, { name: "AbortError" });
  const during = new AbortController();
  const response = await nativeFetch(`${url}/body`, { signal: during.signal });
  const text = response.text();
  during.abort();
  await assert.rejects(text, { name: "AbortError" });
  const cancelled = await nativeFetch(`${url}/cancel`);
  await cancelled.body.cancel();
  const deadline = Date.now() + 2000;
  while (disconnected.length < 3 && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 5));
  assert.deepEqual(disconnected.sort(), ["/body", "/cancel", "/headers"]);
});

test("Rust HTTP deadline covers stalled headers and body", async t => {
  const url = await server(t, (request, response) => {
    if (request.url === "/body") { response.writeHead(200); response.write("prefix"); }
  });
  await assert.rejects(nativeFetch(url, { timeoutMs: 60 }), /timed out/);
  const response = await nativeFetch(`${url}/body`, { timeoutMs: 60 });
  await assert.rejects(response.text(), /timed out/);
});

test("Rust HTTP does not follow redirects or forward credentials to another route", async t => {
  let forwarded = false;
  const destination = await server(t, (_request, response) => { forwarded = true; response.end("wrong"); });
  const url = await server(t, (_request, response) => { response.writeHead(302, { location: destination }); response.end("redirect"); });
  const response = await nativeFetch(url, { headers: { authorization: "Bearer fixture-only" } });
  assert.equal(response.status, 302);
  assert.equal(await response.text(), "redirect");
  assert.equal(forwarded, false);
});

test("Rust HTTP deadline remains effective while a slow consumer fills the native queue", async t => {
  const url = await server(t, (_request, response) => response.end("x".repeat(512 * 1024)));
  const response = await nativeFetch(url, { timeoutMs: 150 });
  // Larger than the bounded handoff queue: a stalled consumer must not allow
  // the transport to accumulate the entire body and report success later.
  await new Promise(resolve => setTimeout(resolve, 250));
  await assert.rejects(response.text(), /timed out/);
});

test("idle timeout permits a healthy exchange longer than its idle window", async t => {
  const url = await server(t, (_request, response) => {
    response.writeHead(200);
    let count = 0;
    const timer = setInterval(() => {
      response.write("进展");
      if (++count === 8) { clearInterval(timer); response.end(); }
    }, 30);
    response.on("close", () => clearInterval(timer));
  });
  const response = await nativeFetch(url, { timeoutMs: null, idleTimeoutMs: 150 });
  assert.equal(await response.text(), "进展".repeat(8));
  const capped = await nativeFetch(url, { timeoutMs: 150, idleTimeoutMs: 150 });
  await assert.rejects(capped.text(), /request timed out/);
});

test("idle timeout bounds stalled headers, body and consumer without an absolute deadline", async t => {
  const url = await server(t, (request, response) => {
    if (request.url === "/body") response.write("prefix");
    if (request.url === "/consumer") response.end("x".repeat(512 * 1024));
  });
  const options = { timeoutMs: null, idleTimeoutMs: 100 };
  await assert.rejects(nativeFetch(url, options), /idle timed out/);
  const response = await nativeFetch(`${url}/body`, options);
  await assert.rejects(response.text(), /idle timed out/);
  const consumer = await nativeFetch(`${url}/consumer`, options);
  await new Promise(resolve => setTimeout(resolve, 200));
  await assert.rejects(consumer.text(), /idle timed out/);
  await assert.rejects(nativeFetch(url, { timeoutMs: null }), /invalid engine HTTP limits/);
});

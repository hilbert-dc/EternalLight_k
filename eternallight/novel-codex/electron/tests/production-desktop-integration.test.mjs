import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import test from "node:test";
import electron from "electron";
import { PersistentTaskBudget } from "@eternallight/agent-engine";
import { fixtureCredential, prepareProductionDesktop } from "./helpers/production-desktop.mjs";

const dist = fileURLToPath(new URL("../../dist/", import.meta.url));
const listen = async server => {
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(0, "127.0.0.1", resolve); });
  return `http://127.0.0.1:${server.address().port}`;
};
const stream = (response, delta, tool = false) => {
  response.writeHead(200, { "content-type": "text/event-stream" });
  response.write(`data: ${JSON.stringify({ id: "desktop-fixture", choices: [{ delta }] })}\n\n`);
  response.end(`data: ${JSON.stringify({ id: "desktop-fixture", choices: [{ delta: {}, finish_reason: tool ? "tool_calls" : "stop" }], usage: { prompt_tokens: 100, completion_tokens: 30, total_tokens: 130 } })}\n\ndata: [DONE]\n\n`);
};

test("production Electron preload/main runs a read-only Codex/MCP task with real persistent budget", { timeout: 60000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-production-desktop-"));
  let providerFailure = null, requests = 0, contextReads = 0;
  const provider = createServer(async (request, response) => {
    try {
      assert.equal(request.headers.authorization, `Bearer ${fixtureCredential}`);
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks).toString());
      assert.equal(body.model, "desktop-scripted-v1");
      assert.ok(++requests <= 4, "fixture request cap exceeded");
      const output = body.messages.find(message => message.role === "tool" && message.tool_call_id === "desktop-read");
      if (output) {
        assert.match(output.content, /抄写员决定为未来留下证词/);
        contextReads++;
        stream(response, { content: "DESKTOP_CONTEXT_READ" });
        return;
      }
      const transcript = body.messages.map(message => message.content ?? "").join("\n");
      const binding = transcript.match(/\{\s*"task":\s*\{\s*"taskId"[\s\S]*?\n  \}\n\}/)?.[0];
      assert.ok(binding, "actual production prompt lacks task binding");
      const { task } = JSON.parse(binding);
      const name = body.tools.find(tool => tool.function?.name.endsWith("__read_context"))?.function.name;
      assert.ok(name, "actual production MCP tool is missing");
      stream(response, { tool_calls: [{ index: 0, id: "desktop-read", function: { name,
        arguments: JSON.stringify({ taskId: task.taskId, projectId: task.projectId, threadId: task.threadId,
          expectedRevision: task.expectedRevision, purpose: "planner", budget: 24, byteBudget: 60000 }) } }] }, true);
    } catch (error) {
      providerFailure = error;
      response.writeHead(500, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: "fixture provider assertion failed" } }));
    }
  });
  const renderer = createServer(async (request, response) => {
    try {
      const pathname = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
      const path = resolve(dist, "." + (pathname === "/" ? "/index.html" : pathname));
      assert.ok(path.startsWith(dist));
      const content = await readFile(path);
      response.writeHead(200, { "content-type": { ".html": "text/html", ".js": "text/javascript", ".css": "text/css" }[extname(path)] ?? "application/octet-stream" });
      response.end(content);
    } catch { response.writeHead(404); response.end(); }
  });
  t.after(async () => {
    for (const server of [provider, renderer]) await new Promise(resolve => { server.closeAllConnections(); server.close(resolve); });
    await rm(directory, { recursive: true, force: true });
  });
  const endpoint = await listen(provider);
  const rendererUrl = await listen(renderer);
  const { appRoot } = await prepareProductionDesktop(directory, endpoint);
  const { stdout, stderr } = await promisify(execFile)(electron,
    [fileURLToPath(new URL("./fixtures/production-desktop-main.mjs", import.meta.url)), appRoot], {
      timeout: 55000, killSignal: "SIGKILL", env: { ...process.env, ELECTRON_RUN_AS_NODE: "", DESKTOP_FIXTURE_KEY: fixtureCredential,
        NOVEL_CODEX_USER_DATA_DIR: join(directory, "user-data"), NOVEL_CODEX_RENDERER_URL: rendererUrl,
        NOVEL_CODEX_MODEL: "desktop-scripted-v1", NOVEL_CODEX_PROVIDER: "Fixture", NOVEL_CODEX_CLI: "" },
    }).catch(error => { throw new Error(`production desktop child failed: ${(error.stdout ?? "").slice(-3000)}\n${(error.stderr ?? "").slice(-1500)}`); });
  assert.ifError(providerFailure);
  const line = stdout.split("\n").find(line => line.startsWith("DESKTOP_RESULT:"));
  assert.ok(line, `production desktop emitted no result: ${stderr.slice(-1500)}`);
  const result = JSON.parse(line.slice("DESKTOP_RESULT:".length));
  assert.equal(result.passed, true, `${result.error}\n${stderr.slice(-2500)}`);
  assert.equal(contextReads, 1);
  assert.equal(requests, 2);
  const budgetReceipt = PersistentTaskBudget.receipt(join(directory, "user-data", "agent-engine", "engine-state.sqlite3"), result.taskId);
  assert.equal(budgetReceipt.status, "completed");
  assert.equal(budgetReceipt.usage.requests, requests);
  t.diagnostic(`production main/preload task ${result.taskId}; ${requests} local requests; ${result.approvals} author budget confirmation`);
});

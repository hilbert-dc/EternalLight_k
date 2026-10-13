import { mcpRequestIdentity } from "../mcp-authority.mjs";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdtemp, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { ModelBridge, TaskBudget, compileCodexConfig, compileCodexModelCatalog, createExecutionLease,
  defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { createMcpStartupWaiter } from "./helpers/mcp-startup.mjs";
import { executorValidatedTools } from "../executor-validation-policy.mjs";

// Real Core, bridge and MCP; the provider and read-only Runtime receiver are local
// fixtures. This proves error delivery/correction, not a real model's judgement.
for (const toolName of ["query_story", "read_context", "read_work_issues"]) {
  test(`real Core corrects ${toolName} through a genuine MCP error in one author turn`, { timeout: 120000 }, async t => {
    const root = await mkdtemp(join(tmpdir(), "novel-query-correction-"));
    let adapter, bridge, startup, failure;
    const received = [], requests = [];
    const receiver = createServer(async (request, response) => {
      const chunks = []; for await (const chunk of request) chunks.push(chunk);
      if (request.url === "/api/tool-binding") {
        assert.equal(request.headers["x-novel-root-thread"], binding.threadId);
        response.writeHead(200, { "content-type": "application/json" });
        response.end(JSON.stringify({ ...binding, operation: "advisory" })); return;
      }
      received.push({ path: request.url, body: chunks.length ? JSON.parse(Buffer.concat(chunks)) : null });
      response.writeHead(200, { "content-type": "application/json" }); response.end(JSON.stringify({ fixtureResult: "corrected-query" }));
    });
    const plan = { question: "找信", byteBudget: 9000, ranking: { type: "literal" },
      steps: [{ id: "source", operation: { type: "text", query: "信", limit: 1 } }] };
    const parameters = valid => {
      if (toolName === "read_work_issues") return { issueId: null, cursor: 0, limit: valid ? 1 : 0 };
      const query = structuredClone(plan);
      if (!valid) delete query.ranking;
      return toolName === "read_context" ? { budget: 24, byteBudget: 60000, queryPlan: query } : { plan: query };
    };
    let binding;
    const provider = createServer(async (request, response) => {
      try {
        const chunks = []; for await (const chunk of request) chunks.push(chunk);
        const body = JSON.parse(Buffer.concat(chunks)); requests.push(body);
        const sequence = requests.length;
        const namespace = body.tools.find(tool => tool.type === "namespace" && tool.name === "mcp__novel_workspace");
        const tool = namespace?.tools.find(tool => tool.type === "function" && tool.name === toolName);
        assert.ok(tool, `real MCP must expose ${toolName}; exposed: ${body.tools.map(tool => tool.name ?? tool.type).join(", ")}`);
        const output = body.input.filter(item => item.type === "function_call_output");
        let item;
        if (sequence === 1) {
          assert.equal(received.length, 0);
          if (toolName === "read_work_issues") assert.equal(tool.parameters.properties.limit.minimum, 1);
          item = { type: "function_call", id: "bad-item", call_id: "invalid-query", namespace: namespace.name, name: tool.name,
            arguments: JSON.stringify({ expectedRevision: binding.expectedRevision, ...parameters(false) }) };
        } else if (sequence === 2) {
          assert.equal(received.length, 0, "invalid plan must never reach Runtime");
          const error = output.find(item => item.call_id === "invalid-query");
          assert.ok(error, "Core must preserve the original call ID and genuine MCP error");
          if (toolName === "read_work_issues") {
            const rendered = /^Wall time: [0-9.]+ seconds\nOutput:\s*([\s\S]*)$/.exec(error.output);
            assert.ok(rendered, "Core must retain its actual MCP output wrapper");
            const feedback = JSON.parse(rendered[1]);
            assert.equal(feedback.code, "invalid_tool_arguments");
            assert.equal(feedback.instancePath, "/limit");
            assert.deepEqual(feedback.expected, { minimum: 1 });
          } else assert.match(JSON.stringify(error.output), /invalid_query_plan/);
          item = { type: "function_call", id: "good-item", call_id: "corrected-query", namespace: namespace.name, name: tool.name,
            arguments: JSON.stringify({ expectedRevision: binding.expectedRevision, ...parameters(true) }) };
        } else {
          assert.equal(sequence, 3, "exactly one correction and one final response");
          assert.equal(received.length, 1);
          assert.match(JSON.stringify(output.find(item => item.call_id === "corrected-query")?.output), /corrected-query/);
          item = { type: "message", id: "final-item", role: "assistant", status: "completed",
            content: [{ type: "output_text", text: "已完成查询。", annotations: [] }] };
        }
        const id = `response-${sequence}`;
        const events = [{ type: "response.created", response: { id } },
          { type: "response.output_item.done", output_index: 0, item },
          { type: "response.completed", response: { id, status: "completed", output: [item], usage: { input_tokens: 3, output_tokens: 2, total_tokens: 5 } } }];
        response.writeHead(200, { "content-type": "text/event-stream" });
        response.end(events.map(event => `data: ${JSON.stringify(event)}\n\n`).join(""));
      } catch (error) { failure = error; response.writeHead(400); response.end(JSON.stringify({ error: { message: "fixture assertion failed" } })); }
    });
    t.after(async () => {
      startup?.dispose();
      if (adapter) { await adapter.finishTaskOperation(); adapter.close(); }
      if (bridge) await bridge.close();
      for (const server of [provider, receiver]) await new Promise(resolve => { server.closeAllConnections(); server.close(resolve); });
      await rm(root, { recursive: true, force: true });
    });
    await Promise.all([receiver, provider].map(server => new Promise(resolve => server.listen(0, "127.0.0.1", resolve))));
    const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture-v1", endpoint: `http://127.0.0.1:${provider.address().port}`,
      credentialRef: "FIXTURE_KEY", protocol: "responses", declaredLimits: { contextTokens: 131072, outputTokens: 8192 } });
    const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection),
      policy: { allowedMcpServerIds: ["novel_workspace"] } });
    bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture", executorValidatedTools, mcpAuthority:true, requestConfigurationProvider:({threadId})=>mcpRequestIdentity(adapter,threadId) });
    const address = await bridge.start();
    const budget = new TaskBudget({ maxRequests: 3, maxTokens: 200000 });
    await bridge.beginTaskBudget(budget);
    await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "query-correction", budgetId: budget.id }));
    const suffix = process.platform === "win32" ? ".exe" : "";
    const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/codex${suffix}`, import.meta.url));
    const mcp = nativeTestBinary("novel-workspace-mcp");
    const catalog = join(root, "catalog.json"); await mkdir(join(root, "sqlite"));
    await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Use the bound query tool. Correct a rejected plan without changing the task binding." })));
    const keys = ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION", "NOVEL_CODEX_ENGINE_AUTHORITY_URL", "NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN"];
    await writeFile(join(root, "config.toml"), compileCodexConfig({ sessionPlan, modelCatalogPath: catalog,
      bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
      mcpServers: [{ id: "novel_workspace", command: mcp, args: [], envVars: keys, approvalMode: "approve", required: true, startupTimeoutMs: 90000 }] }));
    const environment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL"]
      .filter(key => process.env[key]).map(key => [key, process.env[key]]));
    adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: root, env: { ...environment,
      CODEX_HOME: root, CODEX_SQLITE_HOME: join(root, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token, NOVEL_CODEX_ENGINE_AUTHORITY_URL:address.baseUrl,NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN:address.token,
      NOVEL_CODEX_RUNTIME_URL: `http://127.0.0.1:${receiver.address().port}`, NOVEL_CODEX_CANDIDATE_TOKEN: "fixture-candidate",
      NOVEL_CODEX_TASK_CAPABILITY: "fixture-capability", NOVEL_CODEX_OPERATION: "advisory" }, model: connection.id });
    adapter.startProcess();
    startup = createMcpStartupWaiter({ serverName: "novel_workspace", process: adapter.process, timeoutMs: 90000 });
    adapter.setCallbacks({ onEvent: startup.onEvent });
    await adapter.ensureSession(); await startup.ready;
    binding = { taskId: "task", projectId: "project", threadId: adapter.threadId, expectedRevision: 0 };
    let result;
    try { result = await adapter.sendTurn([{ type: "text", text: "找出信的相关内容。" }], { disableEnvironment: true }); }
    catch (error) { throw failure ?? error; }
    assert.ifError(failure); assert.equal(result.turn.status, "completed");
    assert.equal(requests.length, 3); assert.equal(budget.snapshot().requests, 3);
    assert.equal(budget.snapshot().tokens, 15); assert.equal(budget.snapshot().unknownTokens, 0);
    assert.equal(received.length, 1);
    if (toolName === "query_story") {
      assert.equal(received[0].path, "/api/projects/project/tasks/task/query");
      assert.deepEqual(received[0].body, { threadId: binding.threadId, expectedRevision: 0, plan });
    } else {
      const url = new URL(received[0].path, "http://fixture");
      assert.equal(url.searchParams.get("threadId"), binding.threadId);
      if (toolName === "read_work_issues") {
        assert.equal(url.pathname, "/api/projects/project/tasks/task/work-issues");
        assert.equal(url.searchParams.get("expectedRevision"), "0");
        assert.equal(url.searchParams.get("limit"), "1");
        assert.equal(url.searchParams.get("cursor"), "0");
      } else {
        assert.equal(url.pathname, "/api/projects/project/context");
        assert.equal(url.searchParams.get("taskId"), binding.taskId);
        assert.equal(url.searchParams.get("projectRevision"), "0");
        assert.deepEqual(JSON.parse(url.searchParams.get("queryPlan")), plan);
      }
    }
    await adapter.finishTaskOperation();
    const rollouts = (await readdir(join(root, "sessions"), { recursive: true })).filter(path => path.endsWith(".jsonl"));
    assert.equal(rollouts.length, 1);
    const history = (await readFile(join(root, "sessions", rollouts[0]), "utf8")).trim().split("\n").map(line => JSON.parse(line));
    const items = history.filter(entry => entry.type === "response_item").map(entry => entry.payload);
    assert.ok(items.some(item => item.type === "function_call" && item.call_id === "invalid-query"));
    assert.ok(items.some(item => item.type === "function_call_output" && item.call_id === "invalid-query"
      && JSON.stringify(item.output).includes(toolName === "read_work_issues" ? "invalid_tool_arguments" : "invalid_query_plan")));
  });
}

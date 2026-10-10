import assert from "node:assert/strict";
import { mkdtemp, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { ModelBridge, TaskBudget, compileCodexConfig, compileCodexModelCatalog,
  createExecutionLease, defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { resolveCodexCli } from "../codex-runtime.mjs";
import { executorValidatedTools } from "../executor-validation-policy.mjs";
import { createMcpStartupWaiter } from "./helpers/mcp-startup.mjs";

const projectRoot = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const sse = events => events.map(event => `data: ${JSON.stringify(event)}\n\n`).join("");
const listen = server => new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const endpoint = server => `http://127.0.0.1:${server.address().port}`;

// The shipped Codex, native bridge and native MCP are real. The provider and
// Runtime receiver are fixtures: this isolates the correction protocol from
// story-domain persistence, which has separate Runtime integration coverage.
const scenarios = [
  {
    tool: "publish_knowledge_batch", operation: "modelKnowledge", field: "value",
    path: "/api/projects/project/modeling-batches/batch/publish", prompt: "整理本章设定并保存。",
    arguments: (binding, corrected) => ({ ...binding, batchId: "batch", entities: [], assertions: [
      { kind: "assertion", subjectId: "person", predicate: "name", value: "沈遥", evidence: { quote: "沈遥是钟表修理师。" } },
      { kind: "assertion", subjectId: "person", predicate: "occupation", evidence: { quote: "沈遥是钟表修理师。" }, ...(corrected ? { value: "钟表修理师" } : {}) },
    ] }),
    receipt: { batchId: "batch", publicationState: "committed", nextAction: "complete_task", receiptRef: "fixture-receipt" },
    assertWrite: body => assert.equal(body.assertions[1].value, "钟表修理师"),
  },
  {
    tool: "save_draft_checkpoint", operation: "generateChapter", field: "title",
    path: "/api/projects/project/drafts/checkpoints", prompt: "保存本章正文，保留绑定范围。",
    arguments: (binding, corrected) => ({ ...binding, expectedCandidateRevisionId: null, idempotencyKey: "draft-checkpoint",
      payload: { chapterId: "chapter", chapterNumber: 1, completeness: "chapterComplete",
        sceneGroups: [{ id: "opening", sceneId: null, body: "沈遥推开钟楼的门。", locked: false }],
        ...(corrected ? { title: "雨夜钟楼" } : {}) } }),
    receipt: { checkpointId: "draft-checkpoint", revisionId: "draft-revision", chapterId: "chapter", version: 1 },
    assertWrite: body => {
      assert.equal(body.payload.title, "雨夜钟楼");
      assert.equal(body.payload.body, "沈遥推开钟楼的门。");
      assert.equal(body.expectedCandidateRevisionId, null);
      assert.equal(body.idempotencyKey, "draft-checkpoint");
    },
  },
];
for (const scenario of scenarios) for (const protocol of ["responses", "chat-completions"]) for (const exhausted of [false, true]) {
  test(`real Codex ${protocol} ${scenario.tool} ${exhausted ? "stops at correction limit" : "corrects arguments in the same turn"}`,
    { timeout: 180_000 }, async t => {
    const resourcesPath = process.env.NOVEL_CODEX_TEST_RESOURCES;
    const cli = resolveCodexCli({ explicitPath: process.env.NOVEL_CODEX_CLI, isPackaged: Boolean(resourcesPath),
      projectRoot, resourcesPath: resourcesPath ?? projectRoot });
    if (!cli.path) {
      if (resourcesPath || process.env.NOVEL_CODEX_CLI) assert.fail(cli.reason);
      return t.skip(`Codex unavailable: ${cli.reason}`);
    }
    const root = await mkdtemp(join(tmpdir(), "novel-knowledge-correction-"));
    const mcp = process.env.NOVEL_CODEX_TEST_MCP
      ?? join(projectRoot, "native/target/debug", `novel-workspace-mcp${process.platform === "win32" ? ".exe" : ""}`);
    let adapter, bridge, startup, failure, binding;
    const requests = [], writes = [], events = [];
    const runtime = createServer(async (request, response) => {
      let raw = ""; for await (const chunk of request) raw += chunk;
      writes.push({ method: request.method, path: request.url, body: JSON.parse(raw) });
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify(scenario.receipt));
    });
    const provider = createServer(async (request, response) => {
      try {
        let raw = ""; for await (const chunk of request) raw += chunk;
        const body = JSON.parse(raw); requests.push(body);
        const sequence = requests.length;
        assert.ok(sequence <= 3, "correction must not restart the task or exceed three requests");
        const tools = body.tools.flatMap(tool => tool.tools?.map(nested => ({ ...nested, namespace: tool.name })) ?? [tool]);
        const tool = tools.find(tool => (tool.name ?? tool.function?.name ?? "").endsWith(scenario.tool));
        assert.ok(tool, "real MCP target tool must be advertised");
        const outputs = protocol === "responses" ? body.input.filter(item => item.type === "function_call_output")
          : body.messages.filter(item => item.role === "tool").map(item => ({ call_id: item.tool_call_id, output: item.content }));
        if (sequence > 1) {
          const output = outputs.find(item => item.call_id === `correction-${sequence - 1}`);
          assert.ok(output, "MCP result must retain the actual call ID");
          if (exhausted || sequence === 2) {
            assert.equal(writes.length, 0, "rejected arguments must not reach Runtime HTTP");
            assert.match(JSON.stringify(output.output), /invalid_tool_arguments/);
            assert.match(JSON.stringify(output.output), new RegExp(scenario.field));
            if (scenario.tool === "publish_knowledge_batch") {
              assert.match(JSON.stringify(output.output), /missingField/);
              assert.match(JSON.stringify(output.output), /instancePath/);
              assert.match(JSON.stringify(output.output), /assertions\/1/);
            }
          } else assert.equal(writes.length, 1, "corrected call must be dispatched exactly once");
        }
        const args = scenario.arguments(binding, sequence === 2 && !exhausted);
        const final = !exhausted && sequence === 3;
        if (protocol === "responses") {
          const item = final ? { type: "message", id: "final", role: "assistant", status: "completed",
            content: [{ type: "output_text", text: "设定已保存。", annotations: [] }] }
            : { type: "function_call", id: `item-${sequence}`, call_id: `correction-${sequence}`,
              name: tool.name, ...(tool.namespace ? { namespace: tool.namespace } : {}), arguments: JSON.stringify(args), status: "completed" };
          const id = `response-${sequence}`;
          response.writeHead(200, { "content-type": "text/event-stream" });
          response.end(sse([{ type: "response.created", response: { id } },
            { type: "response.output_item.done", output_index: 0, item },
            { type: "response.completed", response: { id, status: "completed", output: [item], usage: { input_tokens: 3, output_tokens: 2, total_tokens: 5 } } }]));
        } else {
          const id = `chat-${sequence}`;
          const delta = final ? { content: "设定已保存。" } : { tool_calls: [{ index: 0, id: `correction-${sequence}`, type: "function",
            function: { name: tool.function.name, arguments: JSON.stringify(args) } }] };
          response.writeHead(200, { "content-type": "text/event-stream" });
          response.end(sse([{ id, choices: [{ index: 0, finish_reason: final ? "stop" : "tool_calls", delta }] },
            { id, choices: [], usage: { prompt_tokens: 3, completion_tokens: 2, total_tokens: 5 } }]) + "data: [DONE]\n\n");
        }
      } catch (error) { failure = error; response.writeHead(400); response.end(JSON.stringify({ error: { message: "fixture assertion failed" } })); }
    });
    t.after(async () => {
      startup?.dispose();
      if (adapter) { await adapter.finishTaskOperation(); adapter.close(); }
      await bridge?.close();
      for (const server of [provider, runtime]) if (server.listening) await new Promise(resolve => { server.closeAllConnections(); server.close(resolve); });
      await rm(root, { recursive: true, force: true });
    });
    await Promise.all([listen(provider), listen(runtime)]);
    const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", protocol, endpoint: endpoint(provider),
      credentialRef: "FIXTURE", declaredLimits: { contextTokens: 131072, outputTokens: 8192 } });
    const plan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: { allowedMcpServerIds: ["novel_workspace"] } });
    bridge = new ModelBridge({ sessionPlan: plan, connections: [connection], credentialResolver: async () => "fixture", executorValidatedTools });
    const address = await bridge.start(), budget = new TaskBudget({ maxRequests: 4, maxTokens: 200000 });
    await bridge.beginTaskBudget(budget);
    await bridge.beginExecutionLease(createExecutionLease({ sessionPlan: plan, runId: "knowledge-task", budgetId: budget.id }));
    await mkdir(join(root, "sqlite"));
    const catalog = join(root, "catalog.json");
    await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(plan, { baseInstructions: "Save the bound content. Correct invalid tool parameters in this same task." })));
    const keys = ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"];
    await writeFile(join(root, "config.toml"), compileCodexConfig({ sessionPlan: plan, modelCatalogPath: catalog,
      bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
      mcpServers: [{ id: "novel_workspace", command: mcp, args: [], envVars: keys, approvalMode: "approve", required: true, startupTimeoutMs: 90_000 }] }));
    adapter = new CodexAppServerAdapter({ command: [cli.path, "app-server"], cwd: root,
      env: { PATH: process.env.PATH, CODEX_HOME: root, CODEX_SQLITE_HOME: join(root, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token,
        NOVEL_CODEX_RUNTIME_URL: endpoint(runtime), NOVEL_CODEX_CANDIDATE_TOKEN: "fixture-candidate", NOVEL_CODEX_TASK_CAPABILITY: "fixture-capability", NOVEL_CODEX_OPERATION: scenario.operation },
      model: connection.id, provider: connection.provider });
    adapter.startProcess(); startup = createMcpStartupWaiter({ serverName: "novel_workspace", process: adapter.process, timeoutMs: 90_000 });
    adapter.setCallbacks({ onEvent: event => { startup.onEvent(event); events.push(event); } });
    await adapter.ensureSession(); await startup.ready;
    binding = { projectId: "project", taskId: "task", threadId: adapter.threadId, expectedRevision: 0 };
    try {
      const turn = adapter.sendTurn([{ type: "text", text: scenario.prompt }], { disableEnvironment: true });
      if (exhausted) await assert.rejects(turn, error => error.code === "tool_argument_correction_limit");
      else assert.equal((await turn).turn.status, "completed");
    } catch (error) { throw failure ?? error; }
    assert.ifError(failure);
    assert.equal(requests.length, 3);
    assert.equal(writes.length, exhausted ? 0 : 1);
    if (!exhausted) {
      assert.equal(writes[0].path, scenario.path);
      assert.equal(writes[0].method, "POST");
      assert.equal(writes[0].body.taskId, binding.taskId);
      assert.equal(writes[0].body.threadId, binding.threadId);
      scenario.assertWrite(writes[0].body);
    }
    const settlementDeadline = Date.now() + 2_000;
    while (budget.snapshot().reservedTokens && Date.now() < settlementDeadline) await new Promise(resolve => setTimeout(resolve, 10));
    assert.deepEqual({ requests: budget.snapshot().requests, tokens: budget.snapshot().tokens, unknown: budget.snapshot().unknownTokens, reserved: budget.snapshot().reservedTokens },
      { requests: 3, tokens: 15, unknown: 0, reserved: 0 });
    assert.equal(events.filter(event => event.method === "turn/started").length, 1);
    const failed = events.filter(event => event.method === "item/completed" && event.params.item.type === "mcpToolCall" && event.params.item.status === "failed");
    assert.equal(failed.length, exhausted ? 3 : 1);
    assert.equal(failed[0].params.item.result.structuredContent.code, "invalid_tool_arguments");
    assert.equal(failed[0].params.item.result.structuredContent.tool, scenario.tool);
    if (exhausted) {
      assert.equal(failed.at(-1).params.item.result.structuredContent.code, "tool_argument_correction_limit");
      assert.equal(failed.at(-1).params.item.result.structuredContent.tool, scenario.tool);
      assert.equal(failed.at(-1).params.item.result.structuredContent.originTool, scenario.tool);
      assert.equal(adapter.process, null);
    }
    await adapter.finishTaskOperation();
    const files = (await readdir(join(root, "sessions"), { recursive: true })).filter(path => path.endsWith(".jsonl"));
    assert.equal(files.length, 1, "one original Codex session");
    const rollout = await readFile(join(root, "sessions", files[0]), "utf8");
    assert.match(rollout, /invalid_tool_arguments/);
    assert.match(rollout, /correction-1/);
  });
}

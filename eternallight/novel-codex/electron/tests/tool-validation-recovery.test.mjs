import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { executorValidatedToolNames, executorValidatedTools, isExecutorValidatedTool } from "../executor-validation-policy.mjs";
import { taskFailureDiagnostic, toolCorrectionLimitError } from "../tool-validation-recovery.mjs";

const waiter = { threadId: "thr-test", turnId: "turn-test" };
const exhausted = () => ({ method: "item/completed", params: { ...waiter, item: {
  id: "rejected-call", type: "mcpToolCall", server: "novel_workspace", tool: "publish_knowledge_batch", status: "failed",
  result: { content: [{ type: "text", text: "Correction limit reached" }],
    structuredContent: { code: "tool_argument_correction_limit", tool: "publish_knowledge_batch", originTool: "publish_knowledge_batch", limit: 3 } },
} } });

test("persistent failure retains the executor code while public explanation stays readable", () => {
  const error = toolCorrectionLimitError(exhausted(), waiter);
  assert.equal(taskFailureDiagnostic(error), "tool_argument_correction_limit");
  assert.match(error.message, /已保存的内容仍保留/);
  for (const other of [new Error("unknown failure"), Object.assign(new Error("invalid_tool_arguments; toolValidation=diagnostic"), {code:"invalid_tool_arguments"})]) {
    assert.equal(taskFailureDiagnostic(other), other.message);
  }
});

test("executor delegation lists exact bundled names in both canonical formats", () => {
  assert.equal(new Set(executorValidatedToolNames).size, executorValidatedToolNames.length);
  assert.equal(executorValidatedTools.length, executorValidatedToolNames.length * 2);
  for (const name of executorValidatedToolNames) {
    assert.match(name, /^[a-z][a-z0-9_]*$/);
    assert.equal(isExecutorValidatedTool(name), true);
    assert.deepEqual(executorValidatedTools.filter(entry => entry.name === name || entry.name === `mcp__novel_workspace__${name}`), [
      { namespace: "mcp__novel_workspace", name },
      { namespace: null, name: `mcp__novel_workspace__${name}` },
    ]);
  }
  for (const name of ["*", "save_*", "mcp__novel_workspace__save_draft_checkpoint", "save_draft_checkpoint_extra", "unknown_tool", null]) {
    assert.equal(isExecutorValidatedTool(name), false);
  }
  assert.ok(Object.isFrozen(executorValidatedTools));
  assert.ok(executorValidatedTools.every(Object.isFrozen));
});

test("only the bound executor terminal result stops parameter correction", () => {
  assert.equal(toolCorrectionLimitError(exhausted(), waiter)?.code, "tool_argument_correction_limit");
  for (const change of [
    m => { m.params.threadId = "other"; }, m => { m.params.turnId = "old"; },
    m => { m.params.item.server = "untrusted"; }, m => { m.params.item.tool = "query_story"; },
    m => { m.params.item.status = "completed"; }, m => { m.method = "item/started"; },
    m => { m.params.item.result.structuredContent.limit = 2; },
    m => { delete m.params.item.result.structuredContent.originTool; },
    m => { m.params.item.result.structuredContent.originTool = "unknown_tool"; },
    m => { m.params.item.tool = "unknown_tool"; m.params.item.result.structuredContent.tool = "unknown_tool"; },
    m => { m.params.item.result.structuredContent.code = "invalid_tool_arguments"; },
    m => { m.params.item.result.structuredContent.tool = "query_story"; },
    m => { m.params.item.result.content[0].text = JSON.stringify(m.params.item.result.structuredContent); delete m.params.item.result.structuredContent; },
  ]) {
    const message = exhausted(); change(message);
    assert.equal(toolCorrectionLimitError(message, waiter), null);
  }
  assert.equal(toolCorrectionLimitError(exhausted(), null), null);
});

test("all bundled tools and cross-tool latch results preserve the trusted origin", () => {
  for (const [tool, originTool] of [
    ["save_draft_checkpoint", "save_draft_checkpoint"],
    ["query_story", "save_draft_checkpoint"],
    ["complete_task", "publish_knowledge_batch"],
    ["update_work_plan", "update_work_plan"],
    ["read_work_plan", "update_work_plan"],
    ["read_source_content", "read_source_content"],
  ]) {
    const message = exhausted();
    message.params.item.tool = tool;
    Object.assign(message.params.item.result.structuredContent, { tool, originTool });
    assert.equal(toolCorrectionLimitError(message, waiter)?.code, "tool_argument_correction_limit");
  }
});

test("terminal correction result fails the managed turn and closes its process without restarting", async () => {
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const adapter = new CodexAppServerAdapter({ command: `${process.execPath} ${fixture}` });
  const events = [];
  let started;
  const ready = new Promise(resolve => { started = resolve; });
  adapter.setCallbacks({ onEvent: event => {
    events.push(event);
    if (event.method === "turn/started") started();
  } });
  const completion = adapter.sendTurn([{ type: "text", text: "interrupt" }]);
  const rejected = assert.rejects(completion, error => error.code === "tool_argument_correction_limit");
  await ready;
  const child = adapter.process;
  adapter.handleLine(JSON.stringify(exhausted()));
  await rejected;
  assert.equal(adapter.process, null);
  assert.equal(adapter.turnWaiter, null);
  assert.equal(adapter.turnOperation, null);
  assert.ok(child.exitCode !== null || child.signalCode !== null);
  assert.equal(events.filter(event => event.method === "turn/started").length, 1);
  assert.equal(events.filter(event => event.method === "agent/error").length, 1);
  adapter.close();
});

test("ordinary parameter errors leave the same managed turn available for correction", async () => {
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const adapter = new CodexAppServerAdapter({ command: `${process.execPath} ${fixture}` });
  let started;
  const ready = new Promise(resolve => { started = resolve; });
  adapter.setCallbacks({ onEvent: event => { if (event.method === "turn/started") started(); } });
  const completion = adapter.sendTurn([{ type: "text", text: "interrupt" }]);
  await ready;
  const originalWaiter = adapter.turnWaiter, originalProcess = adapter.process;
  const rejection = exhausted();
  rejection.params.item.result.structuredContent.code = "invalid_tool_arguments";
  adapter.handleLine(JSON.stringify(rejection));
  assert.equal(adapter.turnWaiter, originalWaiter);
  assert.equal(adapter.process, originalProcess);
  assert.equal(adapter.lastError, null);
  await adapter.interruptTurn();
  const result = await completion;
  assert.equal(result.turn.status, "interrupted");
  adapter.close();
});

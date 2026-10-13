import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { CapabilityEvidenceStore, CapabilityReportStore, TrustedEvidenceRepository, ModelBridge, TaskBudget, createExecutionLease, resolveSessionPlan } from "../../src/index.mjs";

const root = process.argv[2];
const connection = JSON.parse(await readFile(join(root, "connection.json"), "utf8"));
const records = await new CapabilityEvidenceStore(join(root, "evidence.json")).read();
const repository = await TrustedEvidenceRepository.load({ records, reportStore: new CapabilityReportStore(join(root, "reports")) });
const plan = resolveSessionPlan({ connections: [connection], evidenceSnapshot: repository.resolveSnapshot(), policy: { allowedChildConnectionIds: [] } });
assert.ok(plan.routes[0].enabledExtensions.includes("toolLoop"));
const bridge = new ModelBridge({ sessionPlan: plan, connections: [connection], credentialResolver: async () => "fixture", fetchImpl: async () => new Response(JSON.stringify({ id: "reloaded", choices: [{ finish_reason: "stop", message: { role: "assistant", content: "reloaded execution succeeded" } }], usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 } }), { headers: { "content-type": "application/json" } }) });
try {
  const address = await bridge.start();
  const budget = new TaskBudget();
  bridge.beginTaskBudget(budget);
  bridge.beginExecutionLease(createExecutionLease({ sessionPlan: plan, runId: "reloaded", budgetId: budget.id }));
  const response = await fetch(`${address.baseUrl}/v1/responses`, { method: "POST", headers: { authorization: `Bearer ${address.token}`, "content-type": "application/json", "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }) }, body: JSON.stringify({ model: connection.id, stream: false, input: [{ type: "message", role: "user", content: "hello" }], tools: [] }) });
  const payload = await response.json();
  assert.equal(response.status, 200, JSON.stringify(payload));
  assert.equal(payload.output[0].content[0].text, "reloaded execution succeeded");
  process.stdout.write("reloaded execution succeeded");
} finally { await bridge.close(); }

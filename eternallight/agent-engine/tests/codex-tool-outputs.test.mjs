import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { CodexAppServerClient, prepareCodexToolOutputs } from "../src/index.mjs";

test("tool retention requires explicit intent and copies original results", () => {
  const outputs = [{ name: "confirmed_receipt", namespace: null, output: "saved", retain: true },
    { name: "diagnostic", namespace: "engine", output: [{ type: "input_text", text: "temporary" }] }];
  const prepared = prepareCodexToolOutputs(outputs);
  assert.deepEqual(prepared, [outputs[0], { ...outputs[1], retain: false }]);
  prepared[1].output[0].text = "changed";
  assert.equal(outputs[1].output[0].text, "temporary");
  for (const invalid of [null, {}, [null], [{ ...outputs[0], retain: "true" }], [{ ...outputs[0], output: {} }]]) {
    assert.throws(() => prepareCodexToolOutputs(invalid));
  }
});

test("native App Server dispatch preserves retention and revoked executions never dispatch", async t => {
  const client = new CodexAppServerClient({ command: [process.execPath,
    fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))], model: "fixture" });
  t.after(() => client.close());
  const sent = [];
  client.subscribeEvents(event => { if (event.method === "fixture/turn-start-requested") sent.push(event.params); });
  const toolOutputs = [{ name: "receipt", namespace: null, output: "confirmed", retain: true }];
  await client.sendTurn([{ type: "text", text: "hello" }], { toolOutputs });
  assert.deepEqual(sent, [{ threadId: client.threadId, toolOutputs }]);
  await client.bindExecution({ lease: { runId: "run", leaseId: "lease", epoch: 1 } });
  await client.revokeExecution({ runId: "run", leaseId: "lease", expectedEpoch: 1, revokedEpoch: 2 });
  await assert.rejects(client.sendTurn([{ type: "text", text: "hello" }], { toolOutputs }), /revoked/);
  assert.equal(sent.length, 1);
});

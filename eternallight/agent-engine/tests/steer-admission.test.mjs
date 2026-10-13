import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { CodexAppServerClient } from "../src/index.mjs";

for (const code of [-32600, -32603]) {
  test(`native directed input preserves definitive rejection vs unknown error ${code}`, { timeout: 5000 }, async t => {
    const client = new CodexAppServerClient({ command: [process.execPath,
      fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))], model: "fixture",
      env: { ...process.env, FIXTURE_STEER_ERROR_CODE: String(code) } });
    t.after(() => client.close());
    const lease = { runId: "run", leaseId: "lease", epoch: 1 };
    await client.bindExecution({ lease });
    const started = new Promise(resolve => client.subscribeEvents(event => {
      if (event.method === "turn/started") resolve();
    }));
    const completion = client.sendTurn([{ type: "text", text: "interrupt" }]);
    void completion.catch(() => {});
    await started;
    const steer = client.steerTurn([{ type: "text", text: "same task only" }], {
      ...lease, expectedThreadId: client.threadId, expectedTurnId: client.turnId });
    if (code === -32600) assert.deepEqual(await steer, { steered: false, delivery: "notSent", reason: "turnRejected" });
    else await assert.rejects(steer, /no active turn/);
    await client.interruptTurn();
    await completion;
  });
}

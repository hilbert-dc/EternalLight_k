import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
const command = `${process.execPath} ${fixture}`;

test("desktop hypothesis bridge creates and reads a pinned simulation", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    requests.push({ path, body, method: options?.method ?? "GET", query: Object.fromEntries(url.searchParams) });
    return { id: "hypothesis-12345678901234567890123456789012", baseSnapshotId: "snapshot-one" };
  }, command);
  try {
    const payload = {
      baseSnapshotId: "snapshot-one", intent: "如果师父未死",
      assumptions: [{ kind: "assumeClaim", subjectId: "master", predicate: "alive", value: true, storyTime: 1 }],
      idempotencyKey: "request-one",
    };
    await handlers.get("runtime:get-historical-knowledge-state")(rendererEvent(), "project-a", "snapshot-one");
    await handlers.get("runtime:create-hypothesis")(rendererEvent(), "project-a", payload);
    await handlers.get("runtime:get-hypothesis")(rendererEvent(), "project-a", "hypothesis-12345678901234567890123456789012");
    assert.deepEqual(requests, [
      { path: "/api/projects/project-a/knowledge/state", body: null, method: "GET", query: { perspective: "author", snapshotId: "snapshot-one" } },
      { path: "/api/projects/project-a/hypotheses", body: payload, method: "POST", query: {} },
      { path: "/api/projects/project-a/hypotheses/hypothesis-12345678901234567890123456789012", body: null, method: "GET", query: {} },
    ]);
    assert.throws(() => handlers.get("runtime:create-hypothesis")(rendererEvent(), "project-a", { ...payload, assumptions: [] }), /无效/);
    assert.throws(() => handlers.get("runtime:get-hypothesis")(rendererEvent(), "project-a", "../other"), /无效/);
    assert.equal(requests.length, 3);
  } finally {
    adapter.close();
  }
});

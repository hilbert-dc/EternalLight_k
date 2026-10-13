import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
const command = `${process.execPath} ${fixture}`;
const proposalId = `proposal-set-${"a".repeat(32)}`;
const applicationId = `proposal-application-${"b".repeat(32)}`;

test("desktop proposal bridge keeps preview, application and undo author-scoped", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    requests.push({ path, body, method: options?.method ?? "GET", query: Object.fromEntries(url.searchParams) });
    return { ok: true };
  }, command);
  try {
    const event = rendererEvent();
    await handlers.get("runtime:list-proposal-sets")(event, "project-a", "17");
    await handlers.get("runtime:create-proposal-set")(event, "project-a", {
      baseSnapshotId: "snapshot-a", intent: "一起修订", idempotencyKey: "create-a",
      operations: [
        { operationId: "brief", reviewId: "review-brief", dependsOn: [] },
        { operationId: "outline", reviewId: "review-outline", dependsOn: ["brief"] },
      ],
    });
    await handlers.get("runtime:get-proposal-set")(event, "project-a", proposalId);
    await handlers.get("runtime:preview-proposal-set")(event, "project-a", proposalId, ["brief", "outline"], { brief: {}, outline: {} });
    await handlers.get("runtime:apply-proposal-set")(event, "project-a", proposalId, {
      selectedOperationIds: ["brief", "outline"], selectionsByOperationId: { brief: {}, outline: {} }, idempotencyKey: "apply-a",
    });
    await handlers.get("runtime:undo-proposal-set")(event, "project-a", applicationId, 2, "undo-a");
    assert.deepEqual(requests.map(({ path, method }) => ({ path, method })), [
      { path: "/api/projects/project-a/proposal-sets", method: "GET" },
      { path: "/api/projects/project-a/proposal-sets", method: "POST" },
      { path: `/api/projects/project-a/proposal-sets/${proposalId}`, method: "GET" },
      { path: `/api/projects/project-a/proposal-sets/${proposalId}/preview`, method: "POST" },
      { path: `/api/projects/project-a/proposal-sets/${proposalId}/apply`, method: "POST" },
      { path: `/api/projects/project-a/proposal-applications/${applicationId}/undo`, method: "POST" },
    ]);
    assert.deepEqual(requests[0].query, { cursor: "17" });
    assert.deepEqual(requests[3].body, { selectedOperationIds: ["brief", "outline"], selectionsByOperationId: { brief: {}, outline: {} } });
    assert.deepEqual(requests[4].body.selectedOperationIds, ["brief", "outline"]);
    assert.deepEqual(requests[5].body, { expectedProjectRevision: 2, idempotencyKey: "undo-a" });
    assert.throws(() => handlers.get("runtime:preview-proposal-set")(event, "project-a", "bad-id", []), /编号|选择/);
    assert.throws(() => handlers.get("runtime:apply-proposal-set")(event, "project-a", proposalId, { selectedOperationIds: [], selectionsByOperationId: {}, idempotencyKey: "x" }), /参数无效/);
    assert.equal(requests.length, 6);
  } finally {
    adapter.close();
  }
});

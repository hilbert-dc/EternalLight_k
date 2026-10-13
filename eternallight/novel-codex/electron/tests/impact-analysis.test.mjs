import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
const command = `${process.execPath} ${fixture}`;

test("desktop author impact check forwards one validated read-only revision pair", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    requests.push({ path, body, method: options?.method ?? "GET", query: Object.fromEntries(url.searchParams) });
    return { canonicalEntityId: "linxi", findings: [], coverage: { globalImpactComplete: false } };
  }, command);
  try {
    const report = await handlers.get("runtime:get-impact-analysis")(rendererEvent(), "project-a", {
      entityId: "linxi", fromRevisionId: "draft-before", toRevisionId: "draft-after", maxHops: 2, limit: 20,
    });
    assert.equal(report.coverage.globalImpactComplete, false);
    assert.deepEqual(requests, [{
      path: "/api/projects/project-a/impact-analysis", body: null, method: "GET",
      query: { entityId: "linxi", maxHops: "2", limit: "20", fromRevisionId: "draft-before", toRevisionId: "draft-after" },
    }]);
    assert.throws(
      () => handlers.get("runtime:get-impact-analysis")(rendererEvent(), "project-a", { entityId: "linxi", fromRevisionId: "draft-before" }),
      /成对提供/,
    );
    assert.throws(
      () => handlers.get("runtime:get-impact-analysis")(rendererEvent(), "project-a", { entityId: "linxi", maxHops: 10 }),
      /范围无效/,
    );
    assert.equal(requests.length, 1);
  } finally {
    adapter.close();
  }
});

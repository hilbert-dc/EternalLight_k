import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;

test("character reads use current Author knowledge and an exact entity in read-only Runtime requests", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    requests.push({ path, body, method: options?.method ?? "GET", query: Object.fromEntries(url.searchParams) });
    return { snapshotId: null, entries: [] };
  }, command);
  try {
    const catalog = await handlers.get("runtime:get-identity-catalog")(rendererEvent(), "project:one");
    const knowledge = await handlers.get("runtime:get-character-knowledge")(rendererEvent(), "project:one", "entity:岑灯");
    assert.deepEqual([catalog, knowledge], [{ snapshotId: null, entries: [] }, { snapshotId: null, entries: [] }]);
    assert.deepEqual(requests, [
      { path: "/api/projects/project%3Aone/knowledge/entities", body: null, method: "GET", query: {} },
      { path: "/api/projects/project%3Aone/knowledge/state", body: null, method: "GET", query: { perspective: "author", entityId: "entity:岑灯" } },
    ]);
    const before = requests.length;
    for (const args of [["", "entity"], ["project", ""], ["project", "界".repeat(54)], ["project", null], ["project", " entity"], ["project", "entity\n"]]) {
      assert.throws(() => handlers.get("runtime:get-character-knowledge")(rendererEvent(), ...args), /无效/);
    }
    assert.equal(requests.length, before, "Invalid selectors must not reach Runtime or silently become global reads");
  } finally {
    adapter.close();
  }
});

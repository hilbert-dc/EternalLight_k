import assert from "node:assert/strict";
import { test } from "node:test";
import { registerActionPermissionHandlers } from "../action-permission-handlers.mjs";

test("permission IPC validates identities and only reads the bound author resource", async () => {
  let read;
  const paths = [];
  registerActionPermissionHandlers({ handle: (name, handler) => {
    assert.equal(name, "agent:read-action-permission"); read = handler;
  } }, async (...args) => { paths.push(args); return { permission: null }; });
  const source = `task-${"a".repeat(32)}`, action = `action-${"b".repeat(32)}`;
  assert.deepEqual(await read({}, "作品/a", source, action), { permission: null });
  assert.deepEqual(paths, [[`/api/projects/${encodeURIComponent("作品/a")}/tasks/${source}/actions/${action}/permission`]]);
  for (const args of [["", source, action], ["a\nb", source, action], ["p", "../other", action], ["p", source, "action-bad"]]) {
    assert.throws(() => read({}, ...args), /许可身份无效/);
  }
  assert.equal(paths.length, 1);
});

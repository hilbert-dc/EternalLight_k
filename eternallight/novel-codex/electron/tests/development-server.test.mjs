import assert from "node:assert/strict";
import { createServer } from "node:net";
import test from "node:test";
import { availableDevelopmentPort } from "../development-server.mjs";

test("renderer port is stable across restarts and conflicts do not select another origin", async () => {
  const server = createServer();
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  const port = server.address().port;
  try { await assert.rejects(availableDevelopmentPort(String(port)), /开发端口 .* 不可用/); }
  finally { await new Promise(resolve => server.close(resolve)); }
  assert.equal(await availableDevelopmentPort(String(port)), port);
  assert.equal(await availableDevelopmentPort(String(port)), port);
});

test("invalid explicit development port is rejected before launching a renderer", async () => {
  for (const input of ["", "abc", "1.5", "0", "65536"]) await assert.rejects(availableDevelopmentPort(input), /NOVEL_CODEX_DEV_PORT/);
});

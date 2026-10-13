import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;

test("main history restore projects persisted author messages without starting inference", async () => {
  const reads = [];
  let failRead = false;
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    reads.push({ path, body });
    if (path === "/api/projects/project/conversations/thr-history") return {
      id: "thr-history", projectId: "project", title: "整理第一章", isPinned: false, isArchived: false,
      createdAt: 1730831111000, updatedAt: 1730832111000, version: 0, forkedFromId: null, lastTurnId: null,
    };
    if (path.includes("/conversations/")) throw Error("对话不属于当前作品");
    if (path.endsWith("/author-messages")) {
      if (failRead) throw new Error("本地历史读取故障");
      return { threadId: "thr-history", messages: [{ turnId: "turn-history", text: "作者真实历史指令" }] };
    }
    throw new Error(`unexpected route ${path}`);
  }, command);
  let modelTurns = 0;
  const readThread = adapter.readThread.bind(adapter);
  adapter.readThread = async (...args) => {
    const detail = await readThread(...args);
    return { ...detail, turns: detail.turns.map(turn => ({ ...turn, items: [...turn.items,
      { id: "private-reasoning", type: "reasoning", text: "PRIVATE_RAW_HISTORY", encrypted_content: "PRIVATE_OPAQUE_HISTORY" },
      { id: "public-progress", type: "agentMessage", phase: "commentary", text: "正在检查本章", encrypted_content: "PRIVATE_OPAQUE_HISTORY" },
    ] })) };
  };
  const request = adapter.request.bind(adapter);
  adapter.request = async (method, params) => { if (method === "turn/start") modelTurns++; return request(method, params); };
  try {
    const detail = await handlers.get("agent:read-conversation")(rendererEvent(), "project", "thr-history");
    assert.equal(detail.turns[0].items[0].text, "作者真实历史指令");
    assert.equal(detail.turns[0].items.filter(item => item.type === "userMessage").length, 1);
    assert.equal(JSON.stringify(detail).includes("PRIVATE_"), false);
    assert.equal(detail.turns[0].items.find(item => item.id === "public-progress").text, "正在检查本章");
    assert.deepEqual(JSON.parse(JSON.stringify(reads[1])), { path: "/api/projects/project/threads/thr-history/author-messages", body: { turnIds: ["turn-history"] } });
    await assert.rejects(handlers.get("agent:read-conversation")(rendererEvent(), "foreign-project", "thr-history"), /不属于/);
    failRead = true;
    await assert.rejects(handlers.get("agent:read-conversation")(rendererEvent(), "project", "thr-history"), /本地历史读取故障/);
    failRead = false;
    const retry = await handlers.get("agent:read-conversation")(rendererEvent(), "project", "thr-history");
    assert.equal(retry.turns[0].items[0].text, "作者真实历史指令");
    assert.equal(JSON.stringify(retry).includes("PRIVATE_"), false);
    assert.equal(modelTurns, 0);
  } finally { await adapter.close(); }
});

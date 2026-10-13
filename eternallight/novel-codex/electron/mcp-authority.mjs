// @ts-check

import { createHash } from "node:crypto";

/** Resolve execution facts through the official App Server, never model text. */
export async function mcpRequestIdentity(adapter, threadId) {
  const capability = adapter?.env?.NOVEL_CODEX_TASK_CAPABILITY;
  if (typeof capability !== "string" || !capability) throw new Error("MCP execution scope is unavailable");
  const result = await adapter.request("thread/read", { threadId, includeTurns: false });
  if (result?.thread?.id !== threadId) throw new Error("MCP thread identity lookup mismatch");
  return {
    mcpScope: createHash("sha256").update(capability).digest("hex"),
    mcpIdentity: { threadId, agentRole: result.thread.agentRole ?? null },
  };
}

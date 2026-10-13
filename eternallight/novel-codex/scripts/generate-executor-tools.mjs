// Derive the Engine validation allowlist from the compiled Native MCP catalog.
// Discovery is static/read-only: no Runtime server, task, or author data is used.
import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const binary = process.env.NOVEL_MCP_BINARY || resolve(process.env.CARGO_TARGET_DIR || resolve(root, "native/target"), "debug", process.platform === "win32" ? "novel-workspace-mcp.exe" : "novel-workspace-mcp");
function rpc(method, params) {
  const run = spawnSync(binary, [], {
    env: { ...process.env, NOVEL_CODEX_RUNTIME_URL: "http://127.0.0.1:1", NOVEL_CODEX_CANDIDATE_TOKEN: "static-catalog", NOVEL_CODEX_TASK_CAPABILITY: "static-catalog", NOVEL_CODEX_OPERATION: "advisory" },
    input: `${JSON.stringify({ jsonrpc: "2.0", id: 1, method, params })}\n`, encoding: "utf8", timeout: 10000, maxBuffer: 4 * 1024 * 1024,
  });
  if (run.error || run.status !== 0) throw new Error(`Native MCP catalog failed: ${run.error || run.stderr}`);
  const response = JSON.parse(run.stdout.trim());
  if (response.error || response.result?.isError) throw new Error(JSON.stringify(response));
  return response.result;
}
const names = new Set(rpc("tools/list", {}).tools.map(tool => tool.name));
let cursor = 0;
const seen = new Set();
do {
  if (seen.has(cursor)) throw new Error("Native catalog cursor repeated");
  seen.add(cursor);
  const response = rpc("tools/call", {
    name: "discover_creative_capabilities", arguments: { cursor },
    _meta: { rootThreadId: "catalog", callerThreadId: "catalog", sessionId: "catalog", turnId: "catalog", callId: `catalog-${cursor}`, samplingRequestId: `catalog-${cursor}` },
  });
  const page = JSON.parse(response.content.find(item => item.type === "text").text);
  for (const item of page.items) if (item.kind === "tool") names.add(item.id);
  cursor = page.nextCursor;
} while (cursor !== null);
if (!names.size || names.size > 64 || [...names].some(name => !/^[a-z][a-z0-9_]*$/.test(name))) throw new Error("Invalid Native tool catalog");
const text = `${JSON.stringify([...names].sort(), null, 2)}\n`;
const destination = resolve(root, "electron/executor-validated-tools.json");
if (process.argv.includes("--check")) {
  if (readFileSync(destination, "utf8") !== text) throw new Error("Executor tool contract drift; run npm run generate:executor-tools");
} else writeFileSync(destination, text);
console.log(`Native executor tool contract: ${names.size} tools (${process.argv.includes("--check") ? "checked" : "generated"})`);

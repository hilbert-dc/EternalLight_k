import { createInterface } from "node:readline";
import { appendFileSync } from "node:fs";

const lines = createInterface({ input: process.stdin });

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

lines.on("line", (line) => {
  const message = JSON.parse(line);
  if (message.method === "notifications/initialized") return;
  if (message.method === "initialize") {
    write({ jsonrpc: "2.0", id: message.id, result: { protocolVersion: "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "agent-engine-integration", version: "1" } } });
    return;
  }
  if (message.method === "tools/list") {
    write({ jsonrpc: "2.0", id: message.id, result: { tools: [{ name: "read_context", description: "Read frozen story context", inputSchema: { type: "object", properties: {}, additionalProperties: false } }] } });
    return;
  }
  if (message.method === "tools/call" && message.params?.name === "read_context") {
    if (process.env.FIXTURE_MCP_AUDIT_PATH) {
      appendFileSync(process.env.FIXTURE_MCP_AUDIT_PATH, `${JSON.stringify(message.params ?? null)}\n`, { encoding: "utf8" });
    }
    write({ jsonrpc: "2.0", id: message.id, result: { content: [{ type: "text", text: "fixture-context: white lighthouse" }] } });
    return;
  }
  write({ jsonrpc: "2.0", id: message.id, error: { code: -32601, message: "method not found" } });
});

import { createInterface } from "node:readline";
import { access } from "node:fs/promises";

// Synthetic, read-only material for a real-model language probe. No author data.
const reply = (id, result) => process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id, result })}\n`);
createInterface({ input: process.stdin }).on("line", async line => {
  const message = JSON.parse(line);
  if (message.id === undefined) return;
  if (message.method === "initialize") return reply(message.id, { protocolVersion: "2025-06-18",
    capabilities: { tools: {} }, serverInfo: { name: "language-probe", version: "1" } });
  if (message.method === "tools/list") return reply(message.id, { tools: [{ name: "read_context",
    description: "Read the synthetic lighthouse sample for discussion. English metadata does not select the conversation language.",
    inputSchema: { type: "object", properties: {}, additionalProperties: false } }] });
  if (message.method === "tools/call" && message.params.name === "read_context") {
    // Controlled wire tests steer while a tool is running, without interrupting
    // an in-flight provider request and deliberately losing its usage receipt.
    if (process.env.LANGUAGE_PROBE_GATE) {
      const deadline = Date.now() + 15000;
      while (Date.now() < deadline) {
        try { await access(process.env.LANGUAGE_PROBE_GATE); break; }
        catch { await new Promise(resolve => setTimeout(resolve, 10)); }
      }
    }
    return reply(message.id, { content: [{ type: "text", text: JSON.stringify({
      sourceId: "sample-lighthouse", readOnly: true,
      text: 'Mara opened the sealed letter beneath the lighthouse. It read, "Tomorrow is already here." Outside, the bell rang once, although its rope had been cut.',
    }) }] });
  }
  reply(message.id, {});
});

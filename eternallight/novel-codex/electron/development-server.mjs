import { createServer } from "node:net";

export async function availableDevelopmentPort(configuredPort = "5173") {
  if (!/^\d+$/.test(configuredPort)) throw new Error("NOVEL_CODEX_DEV_PORT 必须是有效端口");
  const port = Number(configuredPort);
  if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("NOVEL_CODEX_DEV_PORT 必须在 1–65535 之间");
  // A different origin loses the author's localStorage identity and preferences.
  // Check availability, but never fall back to a random renderer port.
  await new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", error => reject(new Error(`开发端口 ${port} 不可用；请关闭占用进程后重试，或明确设置 NOVEL_CODEX_DEV_PORT。`, { cause: error })));
    server.listen(port, "127.0.0.1", () => server.close(error => error ? reject(error) : resolve()));
  });
  return port;
}

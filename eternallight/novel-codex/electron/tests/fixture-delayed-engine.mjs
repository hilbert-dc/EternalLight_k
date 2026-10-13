import readline from "node:readline";

const threads = new Map();
const input = readline.createInterface({ input: process.stdin });
input.on("line", line => {
  const message = JSON.parse(line);
  if (message.id === undefined) return;
  const delay = Number(message.method === "initialize" ? process.env.FIXTURE_INITIAL_DELAY : process.env.FIXTURE_REQUEST_DELAY) || 0;
  const session = ["thread/start", "thread/resume"].includes(message.method);
  const result = session
    ? { thread: { id: message.params?.threadId ?? process.env.FIXTURE_THREAD_ID ?? "delayed-thread", turns: [] } } : { method: message.method };
  if (session) threads.set(result.thread.id, result.thread);
  if (message.method === "thread/name/set") {
    const thread = threads.get(message.params.threadId);
    if (thread) thread.name = message.params.name;
  }
  if (message.method === "thread/read") result.thread = threads.get(message.params.threadId);
  const response = session && process.env.FIXTURE_SESSION_FAILURE === "1"
    ? { id: message.id, error: { code: -32000, message: "required MCP servers failed to initialize" } }
    : { id: message.id, result };
  setTimeout(() => process.stdout.write(JSON.stringify(response) + "\n"),delay);
});

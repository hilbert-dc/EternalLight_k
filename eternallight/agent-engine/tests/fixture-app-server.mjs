import readline from "node:readline";
import { existsSync, writeFileSync } from "node:fs";

const output = (message) => process.stdout.write(`${JSON.stringify(message)}\n`);
const input = readline.createInterface({ input: process.stdin });
let threadStartCount = 0;

if (process.env.FIXTURE_SIGTERM_DELAY_MS) {
  process.on("SIGTERM", () => setTimeout(() => process.exit(0), Number(process.env.FIXTURE_SIGTERM_DELAY_MS)));
}

input.on("line", (line) => {
  const message = JSON.parse(line);
  if (message.method === "initialize") {
    if (process.env.FIXTURE_INVALID_JSONL === "1") {
      process.stdout.write("{not-json}\n");
      return;
    }
    if (process.env.FIXTURE_INITIALIZE_STALL === "1") return;
    if (process.env.FIXTURE_OVERSIZED_JSONL === "1") {
      process.stdout.write("x".repeat(300 * 1024));
      return;
    }
    const marker = process.env.FIXTURE_INITIALIZE_FAIL_ONCE_MARKER;
    if (marker && !existsSync(marker)) {
      writeFileSync(marker, "failed", "utf8");
      output({ id: message.id, error: { code: -32000, message: "fixture initialization failed" } });
      return;
    }
    output({ id: message.id, result: { platformFamily: "test" } });
  }
  if (message.method === "thread/start") {
    threadStartCount += 1;
    output({ method: "fixture/thread-start-requested", params: { count: threadStartCount, pid: process.pid } });
    if (process.env.FIXTURE_CHILD_THREAD_DURING_START === "1") {
      output({ method: "thread/started", params: { thread: { id: "thread-child" } } });
    }
    const respond = () => output({ id: message.id, result: { thread: { id: "thread-engine" } } });
    const delay = Number(process.env.FIXTURE_THREAD_START_DELAY_MS ?? 0);
    if (delay > 0) setTimeout(respond, delay);
    else respond();
  }
  if (message.method === "thread/resume") output({ id: message.id, result: { thread: { id: message.params.threadId } } });
  if (message.method === "thread/name/set") output({ id: message.id, result: {} });
  if (message.method === "turn/start") {
    output({ method: "fixture/turn-start-requested", params: { threadId: message.params.threadId, toolOutputs: message.params.toolOutputs } });
    const start = () => {
      output({ id: message.id, result: { turn: { id: "turn-engine", status: "inProgress" } } });
      output({ method: "turn/started", params: { threadId: message.params.threadId, turn: { id: "turn-engine" } } });
      if (message.params.input[0].text === "delayed-start") return;
      if (message.params.input[0].text === "slow-host-request") {
        output({ id: 92, method: "item/tool/requestUserInput", params: { threadId: message.params.threadId, turnId: "turn-engine" } });
        return;
      }
      if (message.params.input[0].text === "approval") {
        output({ id: 91, method: "item/commandExecution/requestApproval", params: { threadId: message.params.threadId, turnId: "turn-engine" } });
        return;
      }
      if (message.params.input[0].text === "interrupt") return;
      if (message.params.input[0].text === "fragmented-unicode") {
        const bytes = Buffer.from(`${JSON.stringify({ method: "item/completed", params: { threadId: message.params.threadId, turnId: "turn-engine", item: { id: "unicode", type: "agentMessage", text: "故事🌙继续" } } })}\r\n`);
        for (const byte of bytes) process.stdout.write(Buffer.from([byte]));
        output({ method: "turn/completed", params: { threadId: message.params.threadId, turn: { id: "turn-engine", status: "completed" } } });
        return;
      }
      output({ method: "item/agentMessage/delta", params: { threadId: message.params.threadId, turnId: "turn-engine", itemId: "agent-1", delta: "engine " } });
      output({ method: "item/completed", params: { threadId: message.params.threadId, turnId: "turn-engine", item: { id: "agent-1", type: "agentMessage", text: "engine ready" } } });
      output({ method: "turn/completed", params: { threadId: message.params.threadId, turn: { id: "turn-engine", status: "completed" } } });
    };
    if (message.params.input[0].text === "delayed-start") setTimeout(start, 80);
    else start();
  }
  if (message.method === "turn/steer") {
    if (process.env.FIXTURE_STEER_ERROR_CODE) {
      output({ id: message.id, error: { code: Number(process.env.FIXTURE_STEER_ERROR_CODE), message: "no active turn to steer" } });
      return;
    }
    if (message.params.expectedTurnId !== "turn-engine") { output({id:message.id,error:{message:"wrong turn"}}); return; }
    output({id:message.id,result:{}});
    output({method:"item/completed",params:{threadId:message.params.threadId,turnId:"turn-engine",item:{id:"steered",type:"agentMessage",text:message.params.input[0].text}}});
    output({method:"turn/completed",params:{threadId:message.params.threadId,turn:{id:"turn-engine",status:"completed"}}});
  }
  if (message.method === "turn/interrupt") {
    output({ id: message.id, result: {} });
    output({ method: "turn/completed", params: { threadId: message.params.threadId, turn: { id: message.params.turnId, status: "interrupted" } } });
  }
  if (message.id === 91) {
    const declined = message.result?.decision === "decline";
    output({ method: "item/completed", params: { threadId: "thread-engine", turnId: "turn-engine", item: { id: "agent-2", type: "agentMessage", text: declined ? "approval declined" : "approval leaked" } } });
    output({ method: "turn/completed", params: { threadId: "thread-engine", turn: { id: "turn-engine", status: "completed" } } });
  }
});

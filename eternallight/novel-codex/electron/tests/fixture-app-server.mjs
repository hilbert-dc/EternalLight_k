import readline from "node:readline";
import { existsSync, readFileSync, writeFileSync, renameSync } from "node:fs";

const history = [{ id: "turn-history", status: "completed", items: [{ id: "item-user", type: "userMessage", text: "整理第一章", status: "completed" }, { id: "item-agent", type: "agentMessage", text: "我会先检查白塔灯光的伏笔。", status: "completed" }, { id: "item-tool", type: "commandExecution", command: "echo test", status: "completed" }] }];
const threads = new Map(["thr-test", "thr-history", "thr-history-2", "thr-archived"].map(id => [id, {
  id, name: "整理第一章", preview: "继续完善灯塔场景", status: { type: "notLoaded" }, createdAt: 1730831111, updatedAt: 1730832111, modelProvider: "deepseek", isPinned: false, ephemeral: false,
  turns: id === "thr-test" ? [] : structuredClone(history),
}]));
let threadSequence = 0;
let currentTurnThreadId = null;
const threadStatePath = process.env.NOVEL_CODEX_TEST_THREAD_STATE;
const refreshThreads = () => {
if (threadStatePath && existsSync(threadStatePath)) {
  const state = JSON.parse(readFileSync(threadStatePath, "utf8"));
  threadSequence = state.threadSequence;
  threads.clear();
  for (const [id, thread] of state.threads) threads.set(id, thread);
}
};
refreshThreads();
const persistThreads = () => {
  if (!threadStatePath) return;
  const temporary = `${threadStatePath}.tmp-${process.pid}`;
  writeFileSync(temporary, JSON.stringify({ threadSequence, threads: [...threads] }));
  renameSync(temporary, threadStatePath);
};
const nextThread = () => {
  const id = ++threadSequence === 1 ? "thr-test" : `thr-test-${threadSequence}`;
  const thread = { id, name: null, preview: "", status: { type: "idle" }, createdAt: 1730833111 + threadSequence, updatedAt: 1730833111 + threadSequence, modelProvider: "deepseek", isPinned: false, ephemeral: false, turns: [] };
  threads.set(id, thread);
  return thread;
};
const output = (message) => {
  const params = message.params;
  const thread = threads.get(params?.threadId ?? currentTurnThreadId);
  if (thread && message.method === "item/completed" && params?.item) {
    const turn = thread.turns.find(item => item.id === (params.turnId ?? "turn-test"));
    if (turn) turn.items.push(structuredClone(params.item));
  }
  if (thread && message.method === "turn/completed") {
    const turn = thread.turns.find(item => item.id === params.turn.id);
    if (turn) Object.assign(turn, { status: params.turn.status });
  }
  persistThreads();
  process.stdout.write(`${JSON.stringify(message)}\n`);
};
const input = readline.createInterface({ input: process.stdin });

input.on("line", (line) => {
  refreshThreads();
  const message = JSON.parse(line);
  if (message.method === "initialize") output({ id: message.id, result: { platformFamily: "test" } });
  if (message.method === "thread/start") {
    const thread = nextThread();
    output({ id: message.id, result: { thread } });
    output({ method: "thread/started", params: { thread } });
  }
  if (message.method === "thread/fork") {
    const source = threads.get(message.params.threadId);
    const index = source?.turns.findIndex(turn => turn.id === message.params.lastTurnId) ?? -1;
    if (!source || index < 0) output({ id: message.id, error: { message: "fork source turn missing" } });
    else {
      const thread = nextThread();
      thread.turns = structuredClone(source.turns.slice(0, index + 1));
      output({ id: message.id, result: { thread } });
    }
  }
  if (message.method === "thread/list") {
    if (message.params?.cursor === "empty-page") {
      output({ id: message.id, result: { data: [], nextCursor: null } });
      return;
    }
    const thread = message.params?.archived === true
      ? { id: "thr-archived", name: "旧稿回收站", preview: "保留旧版灯塔结局", status: { type: "notLoaded" }, createdAt: 1730821111, updatedAt: 1730822111, modelProvider: "deepseek", section: { id: "01984de2-8f74-7c91-a3b2-5c5e937cf318", name: "Pinned" }, ephemeral: false }
      : message.params?.cursor
        ? { id: "thr-history-2", name: "检查第二章", preview: "补齐潮汐档案线索", status: { type: "notLoaded" }, createdAt: 1730821111, updatedAt: 1730822111, modelProvider: "deepseek", isPinned: false, ephemeral: false }
        : { id: "thr-history", name: "整理第一章", preview: "继续完善灯塔场景", status: { type: "notLoaded" }, createdAt: 1730831111, updatedAt: 1730832111, modelProvider: "deepseek", isPinned: false, ephemeral: false };
    output({ id: message.id, result: { data: [thread], nextCursor: message.params?.archived === true || message.params?.cursor ? null : "cursor-page-2" } });
  }
  if (message.method === "thread/resume") {
    const thread = threads.get(message.params.threadId);
    output(thread ? { id: message.id, result: { thread: { ...thread, status: { type: "idle" } } } } : { id: message.id, error: { message: "thread missing" } });
  }
  if (message.method === "thread/read") {
    const thread = threads.get(message.params.threadId);
    output(thread ? { id: message.id, result: { thread } } : { id: message.id, error: { message: "thread missing" } });
  }
  if (message.method === "thread/name/set") {
    if (threads.has(message.params.threadId)) threads.get(message.params.threadId).name = message.params.name;
    output({ id: message.id, result: { thread: { id: message.params.threadId, name: message.params.name } } });
    output({ method: "thread/name/updated", params: { threadId: message.params.threadId, name: message.params.name } });
  }
  if (message.method === "thread/section/move") {
    if (message.params.sectionId !== "01984de2-8f74-7c91-a3b2-5c5e937cf318" && message.params.sectionId !== null) output({ id: message.id, error: { message: "invalid section" } });
    else output({ id: message.id, result: {} });
  }
  if (message.method === "thread/archive") {
    output({ id: message.id, result: {} });
    output({ method: "thread/archived", params: { threadId: message.params.threadId } });
  }
  if (message.method === "thread/unarchive") {
    output({ id: message.id, result: { thread: { id: message.params.threadId, name: "旧稿回收站", preview: "保留旧版灯塔结局", status: { type: "notLoaded" }, createdAt: 1730821111, updatedAt: 1730822111, modelProvider: "deepseek", isPinned: true, ephemeral: false } } });
    output({ method: "thread/unarchived", params: { threadId: message.params.threadId } });
  }
  if (message.method === "review/start") {
    output({ id: message.id, result: { turn: { id: "review-turn", status: "inProgress" } } });
    output({ method: "turn/started", params: { threadId: message.params.threadId, turn: { id: "review-turn", status: "inProgress" } } });
    output({ method: "item/started", params: { threadId: message.params.threadId, item: { id: "review-item", type: "enteredReviewMode", status: "inProgress" } } });
    output({ method: "item/completed", params: { threadId: message.params.threadId, item: { id: "review-item", type: "exitedReviewMode", status: "completed", summary: "工作区变更审查完成" } } });
    output({ method: "turn/completed", params: { threadId: message.params.threadId, turn: { id: "review-turn", status: "completed", items: [] } } });
  }
  if (message.method === "thread/compact/start") {
    output({ id: message.id, result: { turn: { id: "compact-turn", status: "inProgress" } } });
    output({ method: "turn/started", params: { threadId: message.params.threadId, turn: { id: "compact-turn", status: "inProgress" } } });
    output({ method: "item/started", params: { threadId: message.params.threadId, item: { id: "compact-item", type: "contextCompaction" } } });
    output({ method: "item/completed", params: { threadId: message.params.threadId, item: { id: "compact-item", type: "contextCompaction" } } });
    output({ method: "turn/completed", params: { threadId: message.params.threadId, turn: { id: "compact-turn", status: "completed", items: [] } } });
  }
  if (message.method === "thread/backgroundTerminals/list") {
    output({ id: message.id, result: { data: [{ processId: "process-42", command: "npm run watch", cwd: "/tmp/novel", status: "running" }] } });
  }
  if (message.method === "thread/backgroundTerminals/terminate") {
    output({ id: message.id, result: {} });
  }
  if (message.method === "turn/start") {
    currentTurnThreadId = message.params.threadId;
    const thread = threads.get(currentTurnThreadId);
    if (thread) thread.turns.push({ id: "turn-test", status: "inProgress", items: [{ id: "input-test", type: "userMessage", text: message.params.input.map(item => item.text ?? "").join("\n"), status: "completed" }] });
    output({ id: message.id, result: { turn: { id: "turn-test", status: "inProgress" } } });
      output({ method: "turn/started", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "inProgress" } } });
    output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "active" } } });
    if (message.params.input[0].text === "approval") {
      output({ id: 99, method: "item/commandExecution/requestApproval", params: { reason: "test permission", command: "echo test", cwd: "/tmp" } });
      output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "active", activeFlags: ["waitingOnApproval"] } } });
    } else if (message.params.input[0].text === "user-input-free-answer") {
      output({ id: 102, method: "item/tool/requestUserInput", params: { threadId: currentTurnThreadId, turnId: "turn-test", questions: [{ id: "direction", header: "创作方向", question: "你希望怎样继续？", options: [{ label: "白塔谜团" }, { label: "墨鸦线" }], isOther: false }] } });
      output({ method: "thread/status/changed", params: { threadId: currentTurnThreadId, status: { type: "active", activeFlags: ["waitingOnUserInput"] } } });
    } else if (message.params.input[0].text === "user-input") {
      output({ id: 100, method: "item/tool/requestUserInput", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-test", questions: [{ id: "direction", header: "创作方向", question: "这一轮重点推进哪条线？", options: [{ label: "白塔谜团", description: "推进白塔与潮汐记录" }, { label: "墨鸦线", description: "推进信使的秘密" }], isOther: true }] } });
      output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "active", activeFlags: ["waitingOnUserInput"] } } });
    } else if (message.params.input[0].text === "bounded-output" || message.params.input[0].text === "failed-turn") {
      const bounded = message.params.input[0].text === "bounded-output";
      output({ method: "turn/completed", params: { threadId: currentTurnThreadId, turn: { id: "turn-test", status: "failed",
        error: { codexErrorInfo: "other", message: bounded
          ? 'provider_output_item_limit; outputItem={"type":"function_call","utf8Bytes":31700,"conservativeTokens":14000,"limitKind":"conservativeTokens","limit":10000}'
          : "unrelated failure" } } } });
    } else if (message.params.input[0].text === "clean-exit") {
      process.exit(0);
    } else if (message.params.input[0].text === "interrupt") {
      // Wait for the client to send turn/interrupt before ending this turn.
    } else if (message.params.input[0].text === "steer" || message.params.input.map((item) => item.text).join("").includes("fixture-wait-for-steer")) {
      // Wait for the client to steer this active turn before ending it.
    } else if (message.params.input[0].text === "completed-message") {
      output({ method: "item/completed", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-test", item: { id: "agent-completed", type: "agentMessage", text: "最终讨论结论", status: "completed" } } });
      output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "idle" } } });
      output({ method: "turn/completed", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "completed" } } });
    } else if (message.params.input[0].text === "protocol-leak") {
      const leaked = '<｜｜DSML｜｜ calls><｜｜DSML｜｜ invoke name="exec">internal tool template</｜｜DSML｜｜ invoke></｜｜DSML｜｜ calls>';
      output({ method: "item/completed", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-test", item: { id: "agent-protocol-leak", type: "agentMessage", text: leaked, status: "completed" } } });
      output({ method: "turn/completed", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "completed", items: [{ id: "agent-protocol-leak", type: "agentMessage", text: leaked }] } } });
    } else if (message.params.input.map((item) => item.text).join("").includes("integration-long-reply")) {
      output({ method: "item/completed", params: { threadId: message.params.threadId, turnId: "turn-test", item: { id: "long-completed", type: "agentMessage", text: "🌙".repeat(5000) } } });
      output({ method: "turn/completed", params: { threadId: message.params.threadId, turn: { id: "turn-test", status: "completed" } } });
    } else if (message.params.input[0].text === "long-message") {
      for (let index = 0; index < 70; index += 1) {
        output({ method: "item/agentMessage/delta", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-test", itemId: "agent-long", delta: "x".repeat(1024) } });
      }
      output({ method: "item/completed", params: { threadId: message.params.threadId, turnId: "turn-test", item: { id: "agent-long", type: "agentMessage", phase: "final_answer", text: "x".repeat(70 * 1024) } } });
      output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "idle" } } });
      output({ method: "turn/completed", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "completed" } } });
    } else {
      output({ method: "turn/plan/updated", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-test", plan: [{ step: "读取故事上下文", status: "completed" }, { step: "整理章节修改", status: "inProgress" }, { step: "生成回复", status: "pending" }] } });
      output({ method: "item/started", params: { item: { id: "cmd-test", type: "commandExecution", command: "echo test" } } });
      output({ method: "item/commandExecution/outputDelta", params: { itemId: "cmd-test", delta: "test output" } });
      output({ method: "item/completed", params: { item: { id: "cmd-test", type: "commandExecution", command: "echo test", status: "completed" } } });
      output({ method: "item/completed", params: { item: { id: "file-test", type: "fileChange", path: "chapters/chapter-01.md", diff: "+新段落", status: "completed" } } });
      output({ method: "turn/diff/updated", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-test", diff: "@@ chapters/chapter-01.md\n+新段落" } });
      output({ method: "item/agentMessage/delta", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-old", itemId: "agent-old", delta: "旧轮次内容" } });
      output({ method: "item/completed", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-old", item: { id: "agent-old", type: "agentMessage", text: "旧轮次最终内容", status: "completed" } } });
      output({ method: "item/agentMessage/delta", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-test", itemId: "agent-current", delta: "hello" } });
      output({ method: "item/completed", params: { threadId: message.params.threadId, turnId: "turn-test", item: { id: "agent-current", type: "agentMessage", phase: "final_answer", text: "hello" } } });
      output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "idle" } } });
      output({ method: "turn/completed", params: { threadId: "foreign-thread", turn: { id: "foreign-turn", status: "completed" } } });
      output({ method: "turn/completed", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "completed", receivedParams: message.params } } });
    }
  }
  if (message.method === "turn/interrupt") {
    if (message.params.turnId !== "turn-test") {
      output({ id: message.id, error: { message: "turnId is required" } });
    } else {
      output({ id: message.id, result: {} });
      output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "idle" } } });
      output({ method: "turn/completed", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "interrupted" } } });
    }
  }
  if (message.method === "turn/steer") {
    output({ id: message.id, result: { turnId: "turn-test" } });
    output({ method: "item/agentMessage/delta", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-test", itemId: "agent-steered", delta: " steered" } });
    output({ method: "item/completed", params: { threadId: message.params.threadId, turnId: "turn-test", item: { id: "agent-steered", type: "agentMessage", phase: "final_answer", text: " steered" } } });
    output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "idle" } } });
    output({ method: "turn/completed", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "completed", receivedParams: message.params } } });
  }
  if (message.id === 99) {
    output({ method: "serverRequest/resolved", params: { threadId: message.params?.threadId ?? "thr-test", requestId: 99 } });
    output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "idle" } } });
    output({ method: "turn/completed", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "completed" } } });
  }
  if (message.id === 102) {
    output({ method: "item/completed", params: { threadId: currentTurnThreadId, turnId: "turn-test", item: { id: "free-answer", type: "agentMessage", text: message.result.answers.direction.answers[0], phase: "final_answer", status: "completed" } } });
    output({ method: "serverRequest/resolved", params: { threadId: currentTurnThreadId, requestId: 102 } });
    output({ method: "turn/completed", params: { threadId: currentTurnThreadId, turn: { id: "turn-test", status: "completed" } } });
  }
  if (message.id === 100) {
    const answer = message.result?.answers?.direction?.answers?.[0];
    const multiline = "节奏\t克制\r\n推进白塔\n保留悬念".padEnd(240, "月");
    if (answer !== "白塔谜团" && answer !== multiline) {
      output({ id: message.id, error: { message: "direction answer is required" } });
    } else {
      if (answer === multiline) output({ method: "item/completed", params: { threadId: "thr-test", turnId: "turn-test", item: { id: "multiline-answer", type: "agentMessage", text: answer, phase: "final_answer", status: "completed" } } });
      output({ method: "serverRequest/resolved", params: { threadId: message.params?.threadId ?? "thr-test", requestId: 100 } });
      output({ id: 101, method: "item/tool/requestUserInput", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-test", questions: [{ id: "tone", header: "叙事语气", question: "这一段要保持什么语气？", options: [{ label: "克制", description: "保持冷静、留白的叙述" }, { label: "明快", description: "提升推进感和节奏" }] }] } });
      output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "active", activeFlags: ["waitingOnUserInput"] } } });
    }
  }
  if (message.id === 101) {
    if (message.result?.answers?.tone?.answers?.[0] !== "克制") {
      output({ id: message.id, error: { message: "tone answer is required" } });
    } else {
      output({ method: "serverRequest/resolved", params: { threadId: message.params?.threadId ?? "thr-test", requestId: 101 } });
      output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "idle" } } });
      output({ method: "turn/completed", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "completed", summary: "已根据你的回答继续创作。" } } });
    }
  }
});

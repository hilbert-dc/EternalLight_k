import readline from "node:readline";

const output = (message) => process.stdout.write(`${JSON.stringify(message)}\n`);
const input = readline.createInterface({ input: process.stdin });

input.on("line", (line) => {
  const message = JSON.parse(line);
  if (message.method === "initialize") output({ id: message.id, result: { platformFamily: "test" } });
  if (message.method === "thread/start") {
    output({ id: message.id, result: { thread: { id: "thr-test" } } });
    output({ method: "thread/started", params: { thread: { id: "thr-test" } } });
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
    output({ id: message.id, result: { thread: { id: message.params.threadId, name: "整理第一章", status: { type: "idle" }, ephemeral: false } } });
  }
  if (message.method === "thread/read") {
    output({ id: message.id, result: { thread: { id: message.params.threadId, name: "整理第一章", preview: "继续完善灯塔场景", status: { type: "notLoaded" }, createdAt: 1730831111, updatedAt: 1730832111, modelProvider: "deepseek", isPinned: false, ephemeral: false, turns: [{ id: "turn-history", status: "completed", items: [{ id: "item-user", type: "userMessage", text: "整理第一章", status: "completed" }, { id: "item-agent", type: "agentMessage", text: "我会先检查白塔灯光的伏笔。", status: "completed" }, { id: "item-tool", type: "commandExecution", command: "echo test", status: "completed" }] }] } } });
  }
  if (message.method === "thread/name/set") {
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
    output({ id: message.id, result: { turn: { id: "turn-test", status: "inProgress" } } });
      output({ method: "turn/started", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "inProgress" } } });
    output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "active" } } });
    if (message.params.input[0].text === "approval") {
      output({ id: 99, method: "item/commandExecution/requestApproval", params: { reason: "test permission", command: "echo test", cwd: "/tmp" } });
      output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "active", activeFlags: ["waitingOnApproval"] } } });
    } else if (message.params.input[0].text === "user-input") {
      output({ id: 100, method: "item/tool/requestUserInput", params: { threadId: message.params?.threadId ?? "thr-test", turnId: "turn-test", questions: [{ id: "direction", header: "创作方向", question: "这一轮重点推进哪条线？", options: [{ label: "白塔谜团", description: "推进白塔与潮汐记录" }, { label: "墨鸦线", description: "推进信使的秘密" }], isOther: true }] } });
      output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "active", activeFlags: ["waitingOnUserInput"] } } });
    } else if (message.params.input[0].text === "clean-exit") {
      process.exit(0);
    } else if (message.params.input[0].text === "interrupt") {
      // Wait for the client to send turn/interrupt before ending this turn.
    } else if (message.params.input[0].text === "steer") {
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
    output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "idle" } } });
    output({ method: "turn/completed", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "completed", receivedParams: message.params } } });
  }
  if (message.id === 99) {
    output({ method: "serverRequest/resolved", params: { threadId: message.params?.threadId ?? "thr-test", requestId: 99 } });
    output({ method: "thread/status/changed", params: { threadId: message.params?.threadId ?? "thr-test", status: { type: "idle" } } });
    output({ method: "turn/completed", params: { threadId: message.params?.threadId ?? "thr-test", turn: { id: "turn-test", status: "completed" } } });
  }
  if (message.id === 100) {
    if (message.result?.answers?.direction?.answers?.[0] !== "白塔谜团") {
      output({ id: message.id, error: { message: "direction answer is required" } });
    } else {
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

import assert from "node:assert/strict";
import { test } from "node:test";
import { registerQualityNotebookHandlers, qualityReaderInstructions, QUALITY_READER_INTENT, QUALITY_NOTEBOOK_INSTRUCTIONS } from "../quality-notebook.mjs";
import { taskExecutionPrompt, taskBoundInstructions } from "../workflow-handoff.mjs";
import { fileURLToPath } from "node:url";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";

test("reader prompt discards author notes and source mapping rather than requesting pretend blindness", () => {
  const task = { id: "reader", taskId: "reader", operation: "reviewQuality", projectId: "book", inputRevision: 0, threadId: "fresh",
    target: { type: "project", qualityPacketId: "packet", secret: "FUTURE_TWIST" }, inputManifest: { authorIntent: "SECRET_EXPLANATION" } };
  assert.equal(taskExecutionPrompt(task), QUALITY_READER_INTENT);
  const prompt = taskBoundInstructions({ task });
  assert.equal(prompt, qualityReaderInstructions(task));
  assert.doesNotMatch(prompt, /FUTURE_TWIST|SECRET_EXPLANATION|read_context|request_candidate_task/);
  for (const pattern of [/read_quality_packet/, /submit_quality_reading/, /packetHash/, /局部材料不能认证整书/, /不请求外部工具或其他智能体/]) assert.match(prompt, pattern);
});
test("notebook covers preferences, fidelity, strengths, replay, real trials and calibrated feedback", () => {
  for (const pattern of [/read_work_status/, /request_work_action/, /同Work剩余额度/, /不要求作者点击UI/, /不新建根任务绕过限制/]) assert.match(QUALITY_NOTEBOOK_INSTRUCTIONS, pattern);
  for (const pattern of [/作者确认/, /来源有效/, /本次偏好不跨任务/, /修改前后证据/, /observation.relatesTo/, /goal\/strength\/method/, /Native同Work关系按需回读/, /方法实效或失败观察也保持scope=task/, /不把局部判断当普遍有效/, /desiredOutcomes\/expectedEffects/, /跨章重复/, /有限试写/, /准备不等于审阅/, /read_quality_review/, /没有实测就不宣称质量提升/]) assert.match(QUALITY_NOTEBOOK_INSTRUCTIONS, pattern);
});
test("host validates project identifiers and starts only a fresh, sanitized reader request", async () => {
  const handlers = new Map(), calls = [], readers = [];
  registerQualityNotebookHandlers({ handle: (name, callback) => handlers.set(name, callback) }, {
    runtimeRequest: async (...args) => { calls.push(args); return { version: 1 }; }, runReader: async (...args) => readers.push(args),
  });
  assert.throws(() => handlers.get("runtime:get-quality-notebook")({}, "../escape"));
  assert.throws(() => handlers.get("runtime:get-quality-notebook")({}, "book", -1));
  await handlers.get("runtime:get-quality-notebook")({}, "book", 3);
  assert.equal(calls[0][0], "/api/projects/book/quality-notebook?before=3");
  await handlers.get("agent:review-quality-packet")({}, "book", "packet", "SECRET_AUTHOR_NOTES");
  assert.deepEqual(readers[0].slice(1), ["book", QUALITY_READER_INTENT, { operation: "reviewQuality" }, { type: "project", qualityPacketId: "packet" }]);
});

test("production reader launch binds an independent budget and sends no author material", async () => {
  const runtime = runtimeFixture(), sent = [], requests = [], budgets = [];
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    requests.push({ path, body });
    const result = await runtime.request(path, body);
    if (path.endsWith("/tasks")) {
      result.purpose = "critic";
      result.inputManifest = { authorIntent: "SECRET_AUTHOR_PLAN", unrelatedNotes: "SECRET_STYLE_EXPLANATION" };
    }
    return result;
  }, `${process.execPath} ${fixture}`, { budgets });
  const send = adapter.sendTurn.bind(adapter);
  adapter.sendTurn = async (input, options) => { sent.push({ input, options }); return send(input, options); };
  try {
    const result = await handlers.get("agent:review-quality-packet")(rendererEvent(), "project", "packet");
    assert.equal(result.completion.status, "completed");
    assert.equal(sent.length, 1); assert.equal(sent[0].options.disableEnvironment, true);
    const text = sent[0].input.map(item => item.text).join("") + "\n" + Object.values(sent[0].options.additionalContext).map(item=>item.value).join("\n");
    assert.doesNotMatch(text, /SECRET_AUTHOR_PLAN|SECRET_STYLE_EXPLANATION|read_context/);
    assert.match(text, /read_quality_packet/);
    const creation = requests.find(r => r.path.endsWith("/tasks")).body;
    assert.equal(creation.operation, "reviewQuality");
    assert.deepEqual(JSON.parse(JSON.stringify(creation.target)), { type: "project", qualityPacketId: "packet" });
    assert.ok(creation.initialBudget.authorizationId); assert.ok(budgets.length > 0);
  } finally { adapter.close(); }
});

test("reader cannot request author explanations or accept later steering outside its packet", async () => {
  const adapter = new CodexAppServerAdapter({ command: null }), writes = [], questions = [];
  adapter.prepareTaskOperation("reviewQuality", "reader-isolation-capability-123456789");
  adapter.write = message => writes.push(message);
  adapter.setCallbacks({ onUserInputRequest: request => questions.push(request) });
  adapter.handleLine(JSON.stringify({ id: 7, method: "item/tool/requestUserInput", params: { questions: [{ id: "intent", question: "作者结局是什么？" }] } }));
  assert.deepEqual(writes, [{ id: 7, result: { answers: {} } }]);
  assert.equal(questions.length, 0); assert.equal(adapter.userInputs.size, 0);
  await assert.rejects(adapter.steerTurn([{ type: "text", text: "秘密结局" }]), /冻结阅读材料/);
  adapter.close();
});

import assert from "node:assert/strict";
import { test } from "node:test";
import { acknowledgedMessage, textDigest, visibleTextDigests } from "../src/observability/message-evidence.mjs";

test("message selection is not acknowledgement; only the matching parent delivery releases the gate", () => {
  const hash = textDigest("FOLLOWUP_TOKEN=test");
  const call = { name: "send_message", callId: "send-1", argumentValueDigests: { message: hash } };
  const responses = [{ threadId: "parent", toolCalls: [call] }];
  const event = { method: "item/completed", params: { threadId: "parent", item: { type: "subAgentActivity", kind: "interacted", id: "send-1", agentThreadId: "child" } } };
  assert.equal(acknowledgedMessage([], responses, hash), null);
  for (const wrong of [
    { ...event, method: "item/started" },
    { ...event, params: { ...event.params, threadId: "other" } },
    { ...event, params: { ...event.params, item: { ...event.params.item, id: "other" } } },
    { ...event, params: { ...event.params, item: { ...event.params.item, kind: "started" } } },
  ]) assert.equal(acknowledgedMessage([wrong], responses, hash), null);
  assert.equal(acknowledgedMessage([event], responses, textDigest("different")), null);
  assert.deepEqual(acknowledgedMessage([event], responses, hash), { ...call, threadId: "parent" });
});

test("request evidence sees readable child text but not ciphertext or tool arguments", () => {
  const marker = "FOLLOWUP_TOKEN=test";
  const digest = textDigest(marker);
  assert.deepEqual(visibleTextDigests({ input: [{ type: "agent_message", content: [{ type: "input_text", text: marker }] }] }), [digest]);
  assert.deepEqual(visibleTextDigests({ messages: [{ role: "user", content: `Header\n${marker}\nFooter` }] }), [textDigest("Header"), digest, textDigest("Footer")]);
  assert.deepEqual(visibleTextDigests({ input: [
    { type: "agent_message", content: [{ type: "encrypted_content", encrypted_content: marker }] },
    { type: "function_call", arguments: JSON.stringify({ message: marker }) },
  ] }), []);
  assert.equal(visibleTextDigests({ messages: [{ content: Array.from({ length: 3000 }, (_, i) => `line-${i}`).join("\n") }] }).length, 2048);
});

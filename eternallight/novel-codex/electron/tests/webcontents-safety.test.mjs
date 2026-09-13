import assert from "node:assert/strict";
import { test } from "node:test";

import { safeSendToWebContents } from "../webcontents-safety.mjs";

test("sends to a live WebContents target", () => {
  const messages = [];
  const target = { isDestroyed: () => false, send: (...message) => messages.push(message) };
  assert.equal(safeSendToWebContents(target, "agent:event", { method: "test" }), true);
  assert.deepEqual(messages, [["agent:event", { method: "test" }]]);
});

test("skips an already destroyed WebContents target", () => {
  const target = { isDestroyed: () => true, send: () => { throw new Error("Object has been destroyed"); } };
  assert.equal(safeSendToWebContents(target, "agent:event", { method: "test" }), false);
});

test("contains a destroy race during send", () => {
  let destroyed = false;
  const target = {
    isDestroyed: () => destroyed,
    send: () => {
      destroyed = true;
      throw new Error("Object has been destroyed");
    },
  };
  assert.equal(safeSendToWebContents(target, "agent:event", { method: "test" }), false);
});

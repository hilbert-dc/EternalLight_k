import assert from "node:assert/strict";
import { test } from "node:test";
import { parseModelCliOptions } from "../model-cli-options.mjs";

test("model CLI preserves the requested connection with and without a profile", () => {
  assert.deepEqual(parseModelCliOptions(["writer"]), { connectionId: "writer", profileId: null, help: false });
  for (const args of [["writer", "--profile", "text-v1"], ["--profile", "text-v1", "writer"]]) {
    assert.deepEqual(parseModelCliOptions(args), { connectionId: "writer", profileId: "text-v1", help: false });
  }
  for (const args of [["--profile"], ["--profile", "--unknown"], ["a", "b"], ["--profile", "a", "--profile", "b"], ["--unknown"]]) assert.throws(() => parseModelCliOptions(args));
  assert.deepEqual(parseModelCliOptions(["--help"]), { connectionId: null, profileId: null, help: true });
});

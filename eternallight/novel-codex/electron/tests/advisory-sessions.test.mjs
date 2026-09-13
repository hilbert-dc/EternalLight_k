import assert from "node:assert/strict";
import { test } from "node:test";

import { AdvisorySessions } from "../advisory-sessions.mjs";

test("advisory sessions preserve same-project continuity and reject cross-project reuse", () => {
  const sessions = new AdvisorySessions();
  sessions.remember("project-a", "thread-a");
  sessions.remember("project-b", "thread-b");
  assert.equal(sessions.preferredThread("project-a"), "thread-a");
  assert.equal(sessions.canReuseCurrent("project-a", "thread-a", "project-a", "advisory"), true);
  assert.equal(sessions.canReuseCurrent("project-b", "thread-a", "project-a", "advisory"), false);
  assert.equal(sessions.canReuseCurrent("project-a", "thread-a", "project-a", "planner"), false);
});

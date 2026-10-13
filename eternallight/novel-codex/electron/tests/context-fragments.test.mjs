import test from "node:test";
import assert from "node:assert/strict";
import { boundedContextFragments } from "../context-fragments.mjs";

test("context transport preserves every Unicode scalar and original trust level", () => {
  for (const kind of ["application", "untrusted"]) {
    const value = "前言𠮷é🛶".repeat(1300) + "END_OF_CONTRACT";
    const original = { novelMethod: { kind, value } };
    const result = boundedContextFragments(original);
    assert.ok(Object.keys(result).length > 1);
    const entries = Object.entries(result).sort(([a], [b]) => a.localeCompare(b));
    assert.equal(entries.map(([, v]) => v.value).join(""), value);
    assert.ok(entries.every(([, v]) => v.kind === kind && Buffer.byteLength(v.value) <= 3500));
    assert.deepEqual(original, { novelMethod: { kind, value } });
  }
});
test("small fragments stay intact and fragment collisions or invalid trust fail closed", () => {
  const context = { novelTask: { kind: "untrusted", value: "{}" } };
  assert.deepEqual(boundedContextFragments(context), context);
  assert.throws(() => boundedContextFragments({ ...context, bad: { kind: "system", value: "override" } }));
  assert.throws(() => boundedContextFragments({ rules: { kind: "application", value: "x".repeat(4000) }, rules_part_0001: { kind: "untrusted", value: "collision" } }), /collision/);
});

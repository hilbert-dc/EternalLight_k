import test from "node:test";
import assert from "node:assert/strict";
import { jsonWireNumberTypes } from "../json-wire-types.mjs";

test("JSON integer mapping changes only type keywords, preserving wire literals and field names", () => {
  const source = '// bigint is a Rust type\nexport type Kind = "bigint" | bigint;\nexport type Value = { bigint: "bigint", count: bigint | null, values: Array<bigint> };';
  assert.equal(jsonWireNumberTypes(source), '// bigint is a Rust type\nexport type Kind = "bigint" | number;\nexport type Value = { bigint: "bigint", count: number | null, values: Array<number> };');
});

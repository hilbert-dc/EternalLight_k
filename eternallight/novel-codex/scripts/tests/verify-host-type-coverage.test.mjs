import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { verifyHostTypeCoverage } from "../verify-host-type-coverage.mjs";

test("Host gate checks nested production sources and rejects silence while excluding fixtures", async () => {
  const directory = await mkdtemp(join(tmpdir(), "host-type-coverage-"));
  try {
    await mkdir(join(directory, "lifecycle"));
    await mkdir(join(directory, "tests"));
    await writeFile(join(directory, "checked.mjs"), "// @ts-check\nexport const ready = true;\n");
    await writeFile(join(directory, "lifecycle", "missed.cjs"), "module.exports = {};\n");
    await writeFile(join(directory, "silent.mjs"), "// @ts-check\n// @ts-ignore\nunknown();\n");
    await writeFile(join(directory, "tests", "fixture.mjs"), "const unchecked = true;\n");
    const result = await verifyHostTypeCoverage(pathToFileURL(directory + "/"));
    assert.equal(result.files, 3);
    assert.deepEqual(result.problems.sort(), ["lifecycle/missed.cjs: missing @ts-check", "silent.mjs: type checking suppression"]);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

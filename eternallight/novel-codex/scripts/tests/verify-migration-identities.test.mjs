import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { assertCodexMigrationVersionsUnique } from "../verify-migration-identities.mjs";

test("different filenames cannot claim the same migration version within a database", () => {
  const root = mkdtempSync(join(tmpdir(), "migration-identity-"));
  try {
    const directory = join(root, "codex-rs", "state", "migrations");
    mkdirSync(directory, { recursive: true });
    writeFileSync(join(directory, "0060_guardian.sql"), "SELECT 1;");
    writeFileSync(join(directory, "060_read_state.sql"), "SELECT 1;");
    assert.throws(() => assertCodexMigrationVersionsUnique(root), /Duplicate Codex migration version 60.*0060_guardian.sql and 060_read_state.sql/);
    rmSync(join(directory, "060_read_state.sql"));
    writeFileSync(join(directory, "0061_read_state.sql"), "SELECT 1;");
    const other = join(root, "codex-rs", "state", "logs_migrations");
    mkdirSync(other);
    writeFileSync(join(other, "0060_logs.sql"), "SELECT 1;");
    assert.doesNotThrow(() => assertCodexMigrationVersionsUnique(root));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

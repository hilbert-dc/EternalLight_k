import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { verifyCompiledMethods } from '../verify-release-runtime.mjs';

test('release rejects a healthy but stale Native method build and missing registry entries', t => {
  const root = mkdtempSync(join(tmpdir(), 'compiled-methods-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'novel-workflow'));
  const hash = text => createHash('sha256').update(text).digest('hex');
  writeFileSync(join(root, 'novel-workflow', 'SKILL.md'), 'current contract');
  assert.doesNotThrow(() => verifyCompiledMethods({ 'novel-workflow': hash('current contract') }, root));
  assert.throws(() => verifyCompiledMethods({ 'novel-workflow': hash('stale contract') }, root), /摘要或版本不匹配/);
  for (const registry of [undefined, {}, { 'novel-workflow': hash('current contract'), extra: hash('extra') }]) {
    assert.throws(() => verifyCompiledMethods(registry, root), /目录与发布资源不一致/);
  }
});

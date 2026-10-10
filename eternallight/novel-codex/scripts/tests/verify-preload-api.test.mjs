import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { verifyPreloadApi } from '../verify-preload-api.mjs';
const types = readFileSync(new URL('../../src/types.ts', import.meta.url), 'utf8');
const preload = readFileSync(new URL('../../electron/preload.cjs', import.meta.url), 'utf8');
test('actual preload implements the complete public API, including imported interface groups', () => {
  assert.deepEqual(verifyPreloadApi(types, preload), { missing: [], undeclared: [] });
});
test('a missing real outbox bridge fails the contract gate', () => {
  const broken = preload.replace(/\s*listMessages: invoke\("agent:list-messages"\),/, '');
  assert.deepEqual(verifyPreloadApi(types, broken).missing, ['agent.listMessages']);
});
test('an unreviewed exposed method is rejected', () => {
  const broken = preload.replace('agent: {', 'agent: { unreviewed: () => {},');
  assert.deepEqual(verifyPreloadApi(types, broken).undeclared, ['agent.unreviewed']);
});

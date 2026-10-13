import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { connectionDefinitionDigest, defineConnection } from '@eternallight/agent-engine';
import { modelReasoning, modelConnectionDefinition, selectModelReasoning, requestReasoningConfiguration } from '../model-reasoning.mjs';
import { readModelSelection, saveModelSelection, withModelConfigurationRollback } from '../model-selection-store.mjs';
const entry = JSON.parse(readFileSync(new URL('../../codex-runtime/model-catalog.json', import.meta.url))).connections.find(item => item.id === 'deepseek-flash');

test('product floor and selected effort are distinct from immutable qualification scope', () => {
  const low = defineConnection(modelConnectionDefinition(entry, 'low'));
  for (const effort of ['high', 'max']) assert.equal(connectionDefinitionDigest(defineConnection(modelConnectionDefinition(entry, effort))), connectionDefinitionDigest(low));
  assert.deepEqual(low.protocolOptions.allowedReasoningEfforts, ['low', 'high', 'max']);
  for (const effort of ['none', 'minimal', 'medium', null, 'invalid']) {
    if (effort === null) assert.equal(modelReasoning(entry, effort).effort, 'low');
    else assert.throws(() => modelReasoning(entry, effort), /未通过/);
  }
  assert.equal(modelReasoning({ id: 'another-provider' }), null);
  assert.throws(() => modelReasoning({ id: 'another-provider' }, 'low'), /尚未开放/);
});

test('selection persists before publication, failed writes and stale model choices leave live execution intact', () => {
  const current = { ...entry, reasoning: modelReasoning(entry), sessionPlan: { id: 'frozen' }, execution: { active: true, runId: 'original' } };
  const before = structuredClone(current), saved = [];
  const args = { current, provider: entry.provider, model: entry.model, effort: 'high' };
  const next = selectModelReasoning({ ...args, persist: value => saved.push(value) });
  assert.equal(next.reasoning.effort, 'high'); assert.deepEqual(current, before);
  assert.equal(next.sessionPlan, current.sessionPlan); assert.equal(next.execution, current.execution);
  assert.deepEqual(saved, [{ provider: entry.provider, model: entry.id, reasoningEffort: 'high' }]);
  assert.throws(() => selectModelReasoning({ ...args, persist: () => { throw Error('disk full'); } }), /disk full/);
  for (const patch of [{ model: 'stale' }, { effort: 'none' }, { effort: null }]) assert.throws(() => selectModelReasoning({ ...args, ...patch, persist: () => assert.fail('must not write') }));
  assert.deepEqual(current, before);
  const connections = [defineConnection(modelConnectionDefinition(entry)), { id: 'other', protocolOptions: { reasoningEffort: 'medium', allowedReasoningEfforts: ['medium', 'high'] } }, { id: 'fixed', protocolOptions: { reasoningEffort: 'medium' } }];
  assert.deepEqual(requestReasoningConfiguration(connections, next, entry.id), { reasoningEffort: 'high' });
  assert.deepEqual(requestReasoningConfiguration(connections, next, 'other'), { reasoningEffort: 'medium' });
  assert.equal(requestReasoningConfiguration(connections, next, 'fixed'), null);
  assert.throws(() => requestReasoningConfiguration(connections, next, 'unrouted'), /执行范围/);
});

test('restart reads selected effort and failed configuration restores files without author data access', t => {
  const root = mkdtempSync(join(tmpdir(), 'reasoning-store-')); t.after(() => rmSync(root, { recursive: true, force: true }));
  const selection = join(root, 'selection.json'), config = join(root, 'config.toml');
  assert.equal(readModelSelection(selection), null);
  const original = { provider: entry.provider, model: entry.id, reasoningEffort: 'max' };
  saveModelSelection(selection, original); writeFileSync(config, 'original');
  assert.deepEqual(readModelSelection(selection), original);
  assert.throws(() => withModelConfigurationRollback([config, selection], () => { writeFileSync(config, 'new'); saveModelSelection(selection, { ...original, reasoningEffort: 'low' }); throw Error('failed commit'); }), /failed commit/);
  assert.deepEqual(readModelSelection(selection), original); assert.equal(readFileSync(config, 'utf8'), 'original');
  writeFileSync(selection, 'invalid'); assert.throws(() => readModelSelection(selection), /无法读取/);
});

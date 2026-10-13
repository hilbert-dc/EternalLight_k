import assert from 'node:assert/strict';
import test from 'node:test';
import { listGenerationRunsWithUsage } from '../generation-usage-status.mjs';
import { generationUsageDescription, workStopDescription } from '../../shared/work-recovery-presentation.mjs';
import { WorkExecutionEventConsumer } from '../work-execution-events.mjs';

for (const phase of ['taskRead', 'usageSettlement']) test(`unavailable ${phase} preserves facts and its actual stage`, async () => {
  const run = { id: 'run', status: 'running', activeTaskId: 'task', completedChapters: 3 };
  const request = async path => {
    if (path.endsWith('generation-runs')) return { runs: [run] };
    if (phase === 'taskRead') throw Object.assign(new Error('private payload'), { code: 'storage_unavailable' });
    return { status: 'failed' };
  };
  let settled = 0;
  const result = await listGenerationRunsWithUsage(request, 'project', async () => { settled++; throw new Error('private payload'); });
  assert.equal(settled, phase === 'taskRead' ? 0 : 1);
  assert.deepEqual(result.runs[0], { ...run, usageCheck: { state: 'unavailable', phase, code: phase === 'taskRead' ? 'storage_unavailable' : null } });
  assert.match(generationUsageDescription(result.runs[0]), phase === 'taskRead' ? /无法读取/ : /费用登记/);
  assert.equal(JSON.stringify(result).includes('private payload'), false);
});
test('settled and live tasks are not projected as settlement failures', async () => {
  for (const status of ['running', 'completed']) {
    const run = { id: 'run', status: 'running', activeTaskId: 'task' };
    const result = await listGenerationRunsWithUsage(async path => path.endsWith('generation-runs') ? { runs: [run] } : { status }, 'project', async () => run);
    assert.deepEqual(result.runs[0].usageCheck, { state: 'current' });
  }
});
test('write, usage and combined pending reasons stay distinct', () => {
  assert.equal(new Set(['pendingRecovery', 'pendingUsage', 'pendingWriteAndUsage'].map(workStopDescription)).size, 3);
  assert.equal(workStopDescription('private_code').includes('private_code'), false);
});
test('background scan failure backs off, publishes a recoverable snapshot and clears after successful read', async () => {
  let now = 0, calls = 0, failing = true; const updates = [];
  const consumer = new WorkExecutionEventConsumer({ now: () => now, available: async () => true,
    request: async () => { calls++; if (failing) throw new Error('private text'); return { projects: [] }; },
    healthChanged: value => updates.push(value) });
  await consumer.tick();
  assert.equal(consumer.health()[0].phase, 'scan');
  assert.equal(consumer.health()[0].nextCheckAt, 2000);
  await consumer.tick(); assert.equal(calls, 1);
  now = 2000; await consumer.tick(); assert.equal(calls, 2);
  assert.equal(consumer.health()[0].nextCheckAt, 6000);
  failing = false; now = 6000; await consumer.tick();
  assert.deepEqual(consumer.health(), []);
  assert.deepEqual(updates.at(-1), []);
  assert.equal(JSON.stringify(updates).includes('private text'), false);
});

test('condition identity ignores certificate timestamps and IDs but tracks actual routes and schema', async () => {
  const { executionConditionDigest } = await import('../execution-condition.mjs');
  const plan = { rootRoute: 'root', routes: [{ logicalModel: 'root', provider: 'test', model: 'a', enabledExtensions: ['tools', 'streaming'] }] };
  const first = executionConditionDigest(plan, 'schema-a');
  assert.equal(executionConditionDigest({ ...plan, id: 'new-random', expiresAt: Date.now(), evidenceRefs: ['new'] }, 'schema-a'), first);
  assert.notEqual(executionConditionDigest(plan, 'schema-b'), first);
  assert.notEqual(executionConditionDigest({ ...plan, routes: [{ ...plan.routes[0], model: 'b' }] }, 'schema-a'), first);
});

test('condition records the actual Host power policy, not just provider configuration', async () => {
  const { executionConditionDigest } = await import('../execution-condition.mjs');
  const { EXECUTION_POWER_POLICY } = await import('../execution-domains.mjs');
  const { createHash } = await import('node:crypto');
  const plan = {rootRoute:'root', routes:[{logicalModel:'root',model:'a',provider:'test'}]};
  const routes = [{logicalModel:'root',model:'a',provider:'test',enabledExtensions:[]}];
  const previous = ['schema-a','root',undefined,undefined,routes];
  const hash = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
  assert.equal(EXECUTION_POWER_POLICY, 'prevent-app-suspension');
  assert.notEqual(executionConditionDigest(plan,'schema-a'), hash(previous));
  assert.equal(executionConditionDigest(plan,'schema-a'), hash([...previous,{executionPowerPolicy:EXECUTION_POWER_POLICY}]));
});

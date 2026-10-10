import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import test from 'node:test';
import { ModelBridge, ModelBridgeProcess, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from '../src/index.mjs';
import { verifiedBaselineEvidence } from '../src/testing/index.mjs';

for (const Bridge of [ModelBridge, ModelBridgeProcess]) test(`${Bridge.name}: effort changes after dispatch preserve active response, identity, and cumulative usage`, async t => {
  let selected = 'low', releaseFirst, firstDispatched;
  const observed = [], snapshots = [], wire = [];
  const dispatched = new Promise(resolve => { firstDispatched = resolve; });
  const held = new Promise(resolve => { releaseFirst = resolve; });
  const server = createServer(async (request, response) => {
    let bytes = ''; for await (const chunk of request) bytes += chunk;
    const body = JSON.parse(bytes); wire.push(body);
    if (wire.length === 1) { firstDispatched(); await held; }
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ id: `r${wire.length}`, status: 'completed', output: [{ type: 'message', id: `m${wire.length}`, role: 'assistant', content: [{ type: 'output_text', text: 'done' }] }], usage: { input_tokens: 2, output_tokens: 3, total_tokens: 5 } }));
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  t.after(() => { releaseFirst(); server.closeAllConnections(); return new Promise(resolve => server.close(resolve)); });
  const connection = defineConnection({ id: 'writer', provider: 'fixture', model: 'fixture', endpoint: `http://127.0.0.1:${server.address().port}`,
    credentialRef: 'KEY', protocol: 'responses', declaredLimits: { contextTokens: 64000, outputTokens: 4000 },
    protocolOptions: { reasoningEffort: 'low', allowedReasoningEfforts: ['low', 'high', 'max'] } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: { allowedChildConnectionIds: ['writer'] } });
  const originalPlan = structuredClone(sessionPlan);
  const bridge = new Bridge({ sessionPlan, connections: [connection], credentialResolver: () => 'fixture', heartbeatIntervalMs: 0,
    requestConfigurationProvider: context => { snapshots.push(context); if (selected === 'throw') throw new Error('selection unavailable'); return selected === 'shape' ? { reasoningEffort: 'high', model: 'other' } : { reasoningEffort: selected }; },
    requestObserver: event => observed.push(event) });
  t.after(() => bridge.close());
  const { baseUrl, token } = await bridge.start();
  const budget = new TaskBudget();
  await bridge.beginTaskBudget(budget);
  const lease = createExecutionLease({ sessionPlan, runId: 'same-run', budgetId: budget.id });
  await bridge.beginExecutionLease(lease);
  const input = [{ type: 'message', role: 'user', content: [{ type: 'input_text', text: 'Keep the original request' }] }];
  const send = (child = false) => fetch(`${baseUrl}/v1/responses`, { method: 'POST', headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json',
    'x-codex-turn-metadata': JSON.stringify({ thread_id: child ? 'child' : 'root', ...(child ? { parent_thread_id: 'root' } : {}) }) },
    body: JSON.stringify({ model: 'writer', input, tools: [], stream: false, reasoning: { effort: 'none', summary: 'auto' } }) });
  const first = send(); await dispatched;
  selected = 'high'; releaseFirst();
  const firstResponse = await first; assert.equal(firstResponse.status, 200, await firstResponse.text());
  const second = await send(); assert.equal(second.status, 200, await second.text());
  selected = 'max'; const child = await send(true); assert.equal(child.status, 200, await child.text());
  assert.deepEqual(wire.map(body => body.reasoning), ['low', 'high', 'max'].map(effort => ({ effort, summary: 'auto' })));
  for (const body of wire) assert.deepEqual(body.input, input);
  assert.deepEqual(sessionPlan, originalPlan);
  assert.equal(observed.length, 3);
  assert.ok(snapshots.every(item => item.runId === lease.runId && item.connectionId === 'writer'));
  assert.deepEqual(snapshots.map(item => item.threadId), ['root', 'root', 'child']);
  const before = budget.snapshot();
  assert.equal(before.requests, 3); assert.equal(before.tokens, 15);
  for (selected of ['none', 'invalid', 'shape', 'throw']) {
    const rejected = await send(); assert.ok(rejected.status >= 400, await rejected.text());
    assert.equal(wire.length, 3); assert.deepEqual(budget.snapshot(), before);
  }
});

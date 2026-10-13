import test from 'node:test';
import assert from 'node:assert/strict';
import { CodexAppServerAdapter } from '../codex-adapter.mjs';
import { deliverAuthorQuestion } from '../author-question-delivery.mjs';

function fixture() {
  const adapter = new CodexAppServerAdapter({ command: process.execPath });
  adapter.threadId = 'thread'; adapter.turnId = 'turn';
  adapter.userInputs.set(1, { method: 'item/tool/requestUserInput', params: { threadId: 'thread', turnId: 'turn',
    questions: [{ id: 'direction', question: '结局方向？', options: [{ label: '团圆', description: '积极收束' }] }] } });
  const writes = [], intents = [];
  adapter.write = message => writes.push(message);
  const task = { id: 'task', projectId: 'project', operation: 'advisory' };
  const domain = { id: 'execution', projectId: 'project', activeRuntimeTask: task, activeAgentExecution: { task }, codexAdapter: adapter };
  const target = { executionId: domain.id, projectId: 'project', threadId: 'thread', turnId: 'turn', requestId: 1 };
  const response = { answers: { direction: '开放结局，保留悬念。' } };
  const request = async (path, options) => {
    if (!options) return { intents: structuredClone(intents) };
    if (path.endsWith('/delivered')) { intents[0].status = 'delivered'; return { ...intents[0] }; }
    assert.equal(writes.length, 0, 'persist before writing Core response');
    const body = JSON.parse(options.body);
    assert.equal(body.expectedIntentRevision, 0);
    const record = { taskId: task.id, authorMessageId: 'author-answer', intentRevision: 1, authorText: body.authorText, status: 'recorded' };
    intents.push(record); return { ...record };
  };
  return { domain, adapter, target, response, request, writes, intents };
}

test('persist exact question context and author answer before Core; retain Native ownership', async () => {
  const f = fixture();
  assert.deepEqual(await deliverAuthorQuestion(f.domain, f.target, f.response, f.request), { resolved: true });
  assert.equal(f.intents[0].status, 'delivered');
  const record = JSON.parse(f.intents[0].authorText.split('\n').slice(1).join('\n'));
  assert.equal(record.binding.taskId, 'task');
  assert.equal(record.questions[0].question, '结局方向？');
  assert.deepEqual(record.questions[0].answers, ['开放结局，保留悬念。']);
  assert.equal(f.writes.length, 1); assert.equal(f.adapter.userInputs.size, 0);
  assert.equal(f.domain.advisorySteerInFlight, false);
});

test('save failure keeps question available and never sends Core response', async () => {
  const f = fixture();
  await assert.rejects(deliverAuthorQuestion(f.domain, f.target, f.response, async (path, options) => {
    if (options) throw Error('save failed'); return f.request(path);
  }), /save failed/);
  assert.equal(f.writes.length, 0); assert.equal(f.adapter.userInputs.size, 1);
});

test('delivery receipt failure retries same saved answer without a second Core response', async () => {
  const f = fixture(); let failed = false;
  const transport = async (path, options) => {
    if (path.endsWith('/delivered') && !failed) { failed = true; throw Error('receipt unavailable'); }
    return f.request(path, options);
  };
  await assert.rejects(deliverAuthorQuestion(f.domain, f.target, f.response, transport), /receipt unavailable/);
  assert.equal(f.intents.length, 1); assert.equal(f.adapter.userInputs.size, 1); assert.equal(f.writes.length, 1);
  await assert.rejects(deliverAuthorQuestion(f.domain, f.target, { answers: { direction: '改答案' } }, transport), /不能覆盖/);
  await deliverAuthorQuestion(f.domain, f.target, f.response, transport);
  assert.equal(f.intents.length, 1); assert.equal(f.writes.length, 1); assert.equal(f.intents[0].status, 'delivered');
});

test('Core synchronous write failure preserves the pending question and exact persisted answer', async () => {
  const f = fixture(); f.adapter.write = () => { throw Error('stdin unavailable'); };
  await assert.rejects(deliverAuthorQuestion(f.domain, f.target, f.response, f.request), /stdin unavailable/);
  assert.equal(f.intents[0].status, 'recorded'); assert.equal(f.adapter.userInputs.size, 1);
  f.adapter.write = message => f.writes.push(message);
  await deliverAuthorQuestion(f.domain, f.target, f.response, f.request);
  assert.equal(f.intents.length, 1); assert.equal(f.writes.length, 1);
});

test('foreign, cancelled and concurrent submissions produce no writes', async () => {
  for (const change of [f => { f.target.projectId = 'other'; }, f => { f.target.turnId = 'old'; },
    f => { f.domain.activeAgentExecution.cancellationRequested = true; }, f => { f.domain.advisorySteerInFlight = true; }]) {
    const f = fixture(); change(f);
    await assert.rejects(deliverAuthorQuestion(f.domain, f.target, f.response, f.request));
    assert.equal(f.intents.length, 0); assert.equal(f.writes.length, 0);
  }
});


test('specialists and children cannot solicit author choices outside the primary conversation', () => {
  for (const [operation, threadId] of [['generateChapter', 'thread'], ['reviewQuality', 'thread'], ['advisory', 'child']]) {
    const f = fixture(); f.adapter.userInputs.clear(); f.adapter.activeOperation = operation;
    f.adapter.handleLine(JSON.stringify({ id: 8, method: 'item/tool/requestUserInput', params: { threadId, turnId: 'turn', questions: [{ id: 'choice', question: '选择？' }] } }));
    assert.deepEqual(f.writes, [{ id: 8, result: { answers: {} } }]);
    assert.equal(f.adapter.userInputs.size, 0);
  }
});

test('late Native receipt never clears a later question with a reused protocol number', () => {
  const f = fixture();
  f.adapter.resolveUserInput(1, f.response, { retain: true });
  f.adapter.userInputs.set(1, { method: 'item/tool/requestUserInput', params: { threadId: 'thread', turnId: 'next-turn', questions: [] } });
  f.adapter.acknowledgeUserInput(1, { threadId: 'thread', turnId: 'turn' });
  assert.equal(f.adapter.userInputs.get(1).params.turnId, 'next-turn');
});

test('long multibyte answers reach the durable record and Core unchanged', async () => {
  const f = fixture();
  const answer = '保留人物动机与全书结构。🌙\n'.repeat(150);
  f.response.answers.direction = answer;
  await deliverAuthorQuestion(f.domain, f.target, f.response, f.request);
  const record = JSON.parse(f.intents[0].authorText.split('\n').slice(1).join('\n'));
  assert.deepEqual(record.questions[0].answers, [answer]);
  assert.deepEqual(f.writes[0].result.answers.direction.answers, [answer]);
});

test('question context counts toward Native capacity before any persistence or Core delivery', async () => {
  const f = fixture(); let requests = 0;
  f.adapter.userInputs.get(1).params.questions[0].question = '长问题'.repeat(1800);
  await assert.rejects(deliverAuthorQuestion(f.domain, f.target, f.response, async () => { requests++; }), /超出单次保存容量/);
  assert.equal(requests, 0); assert.equal(f.writes.length, 0); assert.equal(f.intents.length, 0);
  assert.equal(f.adapter.userInputs.size, 1); assert.notEqual(f.domain.advisorySteerInFlight, true);
});

test('aggregate answers cannot bypass capacity with individually short array entries', () => {
  const f = fixture();
  assert.throws(() => f.adapter.prepareUserInput(1, { answers: { direction: Array(6).fill('月'.repeat(1000)) } }), /超出单次保存容量/);
  assert.equal(f.writes.length, 0); assert.equal(f.adapter.userInputs.size, 1);
});

test('Host UTF-8 record budget matches the authoritative Native task-intent contract', async () => {
  const { readFile } = await import('node:fs/promises');
  const { MAX_AUTHOR_ANSWER_RECORD_BYTES, authorAnswerCapacityError } = await import('../../shared/author-answer-budget.mjs');
  const source = await readFile(new URL('../../native/story-core/src/task/task_intents.rs', import.meta.url), 'utf8');
  const nativeBytes = Number(source.match(/const MAX_STEER_BYTES: usize = ([\d_]+);/)[1].replaceAll('_', ''));
  assert.equal(MAX_AUTHOR_ANSWER_RECORD_BYTES, nativeBytes);
  assert.equal(authorAnswerCapacityError('月'.repeat(5333) + 'a'), null);
  assert.match(authorAnswerCapacityError('月'.repeat(5333) + 'ab'), /超出单次保存容量/);
});

test('the durable question and suggested answer retain their complete qualifiers', async () => {
  const f = fixture();
  const question = '比较人物动机与故事结构。'.repeat(30) + '仅讨论，不修改原作';
  const label = '保留既有线索和人物关系。'.repeat(30) + '不代表批准制作';
  const description = '说明适用条件。'.repeat(60);
  f.adapter.userInputs.get(1).params.questions = [{ id: 'direction', question, options: [{ label, description }] }];
  f.response.answers.direction = label;
  await deliverAuthorQuestion(f.domain, f.target, f.response, f.request);
  const record = JSON.parse(f.intents[0].authorText.split('\n').slice(1).join('\n'));
  assert.equal(record.questions[0].question, question);
  assert.deepEqual(record.questions[0].options, [{ label, description }]);
  assert.deepEqual(f.writes[0].result.answers.direction.answers, [label]);
});

test('shared question projection retains identity and safely handles optional protocol fields', async () => {
  const { userInputQuestions } = await import('../../shared/author-questions.mjs');
  assert.deepEqual(userInputQuestions({ questions: [null, { id: 'x'.repeat(241), question: 'invalid' },
    { id: 'valid', question: '完整问题', header: '完整标题'.repeat(40) }] }), [{ id: 'valid', question: '完整问题',
      header: '完整标题'.repeat(40), options: [], isOther: false }]);
  assert.deepEqual(userInputQuestions({questions:[{id:'line\nbreak',question:'invalid'}]}), []);
  const f = fixture(); f.adapter.userInputs.get(1).params.questions = [{ id: 'direction', question: '完整问题' }];
  assert.deepEqual(f.adapter.prepareUserInput(1, f.response).questions, userInputQuestions(f.adapter.userInputs.get(1).params));
});

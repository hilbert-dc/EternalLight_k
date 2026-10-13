import test from 'node:test';
import assert from 'node:assert/strict';
import { runCreationIntake, creationIntakeCases, intakeObservationFailed } from '../fixtures/creation-intake-journey.mjs';

test('synthetic clarification answers use the complete trusted question identity and preserve the authored response', async () => {
  let onQuestion, onEvent, answer, resolveTurn;
  const task = new Promise(resolve => { resolveTurn = resolve; });
  const api = { runtime: {
    createProject: async () => ({ project: { id: 'isolated' } }),
    getCreativeWorkspace: async () => ({ revision: 0 }), listWorkTasks: async () => ({ tasks: [] }),
  }, agent: {
    onEvent: callback => { onEvent = callback; return () => {}; },
    onUserInputRequest: callback => { onQuestion = callback; return () => {}; },
    resolveUserInput: async (target, response) => { answer = { target, response }; resolveTurn({ saved: true }); return { delivered: true }; },
    sendPrompt: () => {
      onEvent({ method: 'agent/execution-request', params: { requestId: 'send' } });
      onQuestion({ executionId: 'execution', requestId: 7, params: { threadId: 'thread', turnId: 'turn', questions: [{ id: 'direction', question: '故事方向？' }] } });
      return task;
    }, interrupt: async () => { throw Error('No interruption expected'); },
  } };
  globalThis.window = { novelCodex: api };
  try {
    const result = await runCreationIntake('intakeClarify', creationIntakeCases.intakeClarify);
    assert.deepEqual(answer.target, { projectId: 'isolated', executionId: 'execution', requestId: 7, threadId: 'thread', turnId: 'turn' });
    assert.equal(answer.response.answers.direction, creationIntakeCases.intakeClarify.answer);
    assert.equal(result.status, 'needsReview');
    assert.equal(result.questions.length, 1);
    assert.equal(result.answers[0].receipt.delivered, true);
  } finally { delete globalThis.window; }
});

test('a question in the fully specified case is retained, never automatically approved', async () => {
  let onQuestion, onEvent, resolveTurn, interrupts = 0;
  const task = new Promise(resolve => { resolveTurn = resolve; });
  globalThis.window = { novelCodex: { runtime: {
    createProject: async () => ({ project: { id: 'isolated' } }), getCreativeWorkspace: async () => ({}), listWorkTasks: async () => ({ tasks: [] }),
  }, agent: {
    onEvent: callback => { onEvent = callback; return () => {}; }, onUserInputRequest: callback => { onQuestion = callback; return () => {}; },
    resolveUserInput: async () => { throw Error('No author response authorized'); },
    sendPrompt: () => { onEvent({ method: 'agent/execution-request', params: { requestId: 'send' } }); onQuestion({ params: { questions: [{ id: 'again' }] } }); return task; },
    interrupt: async target => { assert.equal(target.requestId, 'send'); interrupts++; resolveTurn({ stopped: true }); },
  } } };
  try {
    const result = await runCreationIntake('intakePlan', creationIntakeCases.intakePlan);
    assert.equal(interrupts, 1); assert.equal(result.answers.length, 0); assert.equal(result.questions.length, 1);
    assert.equal(result.status, 'needsReview');
  } finally { delete globalThis.window; }
});


test('bounded intake stops for rejected calls and returned downstream failures, not successful delivery', () => {
  assert.equal(intakeObservationFailed({ ok: false, error: 'budget exhausted' }), true);
  assert.equal(intakeObservationFailed({ ok: true, value: { executionFailure: { code: 'CONVERSATION_INCOMPLETE' } } }), true);
  assert.equal(intakeObservationFailed({ ok: true, value: { executionFailure: null } }), false);
  assert.equal(intakeObservationFailed(undefined), false);
});

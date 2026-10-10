import assert from 'node:assert/strict';
import test from 'node:test';
import { nativeRuntimeError, NativeRuntimeError } from '../runtime-request-error.mjs';
test('Native diagnostic preserves corrective location and source without echoing arbitrary fields', () => {
  const error = nativeRuntimeError({ error: { code: 'invalid_output_payload', message: 'invalid', details: {
    instancePath: '/outcomes/1/evidence/0', code: 'evidence_quote_not_found', sourceRef: { groupId: 'g', draftRevisionId: 'r', privateProse: 'secret' }, privateProse: 'secret' } } });
  assert.ok(error instanceof NativeRuntimeError);
  assert.equal(error.issuer, 'native');
  assert.deepEqual(error.details, { diagnostic: 'invalid', instancePath: '/outcomes/1/evidence/0', code: 'evidence_quote_not_found', sourceRef: { groupId: 'g', draftRevisionId: 'r' } });
  assert.equal(new Error('TaskBindingInvalid') instanceof NativeRuntimeError, false);
});

test('preload retains typed Native correction evidence across IPC failure transport', async () => {
  const { readFile } = await import('node:fs/promises');
  const { runInNewContext } = await import('node:vm');
  const exposed = {};
  const source = await readFile(new URL('../preload.cjs', import.meta.url), 'utf8');
  const failure = { issuer: 'native', phase: 'domainRequest', code: 'invalid_output_payload', message: '请修正引用', details: { instancePath: '/outcomes/1/evidence/0' } };
  runInNewContext(source, { process: { platform: 'darwin' }, require: () => ({ contextBridge: { exposeInMainWorld: (name, value) => { exposed[name] = value; } }, ipcRenderer: { invoke: async () => ({ nativeRequestFailure: failure }) } }) });
  await assert.rejects(exposed.novelCodex.agent.status(), error => {
    assert.equal(error.nativeFailure, failure);
    assert.equal(error.code, 'invalid_output_payload');
    assert.equal(error.details.instancePath, '/outcomes/1/evidence/0');
    return true;
  });
});

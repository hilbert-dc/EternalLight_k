import assert from 'node:assert/strict';
import test from 'node:test';
import { transform } from 'esbuild';
import { readFile } from 'node:fs/promises';
const { code } = await transform(await readFile(new URL('../../src/agent-error-description.ts', import.meta.url), 'utf8'), { loader: 'ts', format: 'esm' });
const { describeAgentError } = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
test('provider prose cannot manufacture local permission or revision facts', () => {
  for (const text of ['Provider mentioned TaskNotWritable', 'task_capability_invalid', 'Provider mentioned TaskBindingInvalid', 'AuthoringRunStale', 'TaskInputStale', 'An example provider_output_item_limit in documentation']) {
    assert.equal(describeAgentError(text), describeAgentError('unclassified failure'));
  }
});
test('known output-limit evidence is localized without interpreting arbitrary prose', () => {
  const expected = describeAgentError('provider_output_item_limit');
  const diagnostic = {type:'message', utf8Bytes:262145, limitKind:'utf8Bytes', limit:262144, conservativeTokens:null};
  const raw = `provider_output_item_limit; outputItem=${JSON.stringify(diagnostic)}`;
  for (const text of [raw, `Error invoking remote method 'resume-work-task': Error: ${raw}`,
    JSON.stringify({error:{code:'provider_output_item_limit',message:'raw remains available'}})]) {
    assert.equal(describeAgentError(text), expected);
  }
  for (const text of [`Example: ${raw}`, 'provider_output_item_limit: arbitrary provider prose',
    'provider_output_item_limit; outputItem={"type":"message"}',
    raw.replace('262145','3'), JSON.stringify({message:raw}), 'provider_output_item_limit_extra']) {
    assert.equal(describeAgentError(text), describeAgentError('unclassified failure'));
  }
});

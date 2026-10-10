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

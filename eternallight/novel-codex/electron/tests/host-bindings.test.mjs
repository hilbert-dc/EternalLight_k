import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { readFileSync } from 'node:fs';
import ts from 'typescript';

const entry = fileURLToPath(new URL('../main.mjs', import.meta.url));
function unresolvedNames(source) {
  const options = { allowJs: true, checkJs: true, noEmit: true, target: ts.ScriptTarget.ESNext,
    module: ts.ModuleKind.NodeNext, moduleResolution: ts.ModuleResolutionKind.NodeNext, skipLibCheck: true };
  const host = ts.createCompilerHost(options);
  const readFile = host.readFile.bind(host);
  host.readFile = path => path === entry ? source : readFile(path);
  const program = ts.createProgram([entry], options, host);
  return program.getSemanticDiagnostics().filter(d => d.file?.fileName === entry && [2304, 2552].includes(d.code))
    .map(d => `${d.file.getLineAndCharacterOfPosition(d.start).line + 1}: ${ts.flattenDiagnosticMessageText(d.messageText, ' ')}`);
}

test('production Host entry has no unresolved variable references', () => {
  assert.deepEqual(unresolvedNames(readFileSync(entry, 'utf8')), []);
});

test('binding check rejects a removed execution-domain storage reference', () => {
  const source = readFileSync(entry, 'utf8');
  const broken = source.replace('...executionContext().agentStorage, credentialRef:', '...missingExecutionStorage, credentialRef:');
  assert.notEqual(broken, source);
  assert.ok(unresolvedNames(broken).some(message => message.includes('missingExecutionStorage')));
});

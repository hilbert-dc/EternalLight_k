import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import ts from 'typescript';

export function verifyPreloadApi(types, preload) {
  const filename = fileURLToPath(new URL('../src/types.ts', import.meta.url));
  const options = { target: ts.ScriptTarget.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler, module: ts.ModuleKind.ESNext };
  const host = ts.createCompilerHost(options);
  const getSource = host.getSourceFile.bind(host);
  host.getSourceFile = (name, languageVersion, ...rest) => name === filename
    ? ts.createSourceFile(name, types, languageVersion, true) : getSource(name, languageVersion, ...rest);
  const program = ts.createProgram([filename], options, host), checker = program.getTypeChecker();
  const source = program.getSourceFile(filename);
  const contract = source.statements.find(node => ts.isInterfaceDeclaration(node) && node.name.text === 'NovelCodexApi');
  if (!contract) throw Error('NovelCodexApi interface not found');
  const expected = [];
  const walk = (type, prefix = '', depth = 0) => {
    if (depth > 8) throw Error('Recursive API contract');
    for (const member of type.getProperties()) {
      const name = `${prefix}${member.name}`;
      const value = checker.getTypeOfSymbolAtLocation(member, member.valueDeclaration ?? contract);
      if (value.getCallSignatures().length) expected.push(name);
      else if (value.flags & (ts.TypeFlags.StringLike | ts.TypeFlags.NumberLike | ts.TypeFlags.BooleanLike | ts.TypeFlags.Union)) continue;
      else if (value.getProperties().length) walk(value, `${name}.`, depth + 1);
      else throw Error(`Unresolved API contract type: ${name}`);
    }
  };
  walk(checker.getTypeAtLocation(contract));
  let exposed;
  const electron = { contextBridge: { exposeInMainWorld(name, api) { if (name === 'novelCodex') exposed = api; } }, ipcRenderer: { on() {}, invoke() {}, removeListener() {} } };
  runInNewContext(preload, { process: { platform: process.platform }, require(name) { if (name !== 'electron') throw Error(`Unexpected preload dependency: ${name}`); return electron; } }, { timeout: 1000 });
  const actual = [];
  const visit = (object, prefix = '') => {
    for (const [key, value] of Object.entries(object ?? {})) {
      if (typeof value === 'function') actual.push(`${prefix}${key}`);
      else if (value && typeof value === 'object') visit(value, `${prefix}${key}.`);
      else if (!['string', 'number', 'boolean'].includes(typeof value)) throw Error(`Unsupported exposed value: ${prefix}${key}`);
    }
  };
  visit(exposed);
  return { missing: expected.filter(name => !actual.includes(name)), undeclared: actual.filter(name => !expected.includes(name)) };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = verifyPreloadApi(readFileSync(new URL('../src/types.ts', import.meta.url), 'utf8'), readFileSync(new URL('../electron/preload.cjs', import.meta.url), 'utf8'));
  console.log(JSON.stringify(result, null, 2));
  if (result.missing.length || result.undeclared.length) process.exitCode = 1;
}

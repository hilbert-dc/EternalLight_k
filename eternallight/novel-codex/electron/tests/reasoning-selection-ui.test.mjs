import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { build } from 'esbuild';
import electron from 'electron';
test('running composer: drag preview/commit/cancel, keyboard change and failure retains choice, no effectiveness labels', async t => {
  const root = await mkdtemp(join(tmpdir(), 'reasoning-ui-')); t.after(() => rm(root, { recursive: true, force: true }));
  await build({ entryPoints: [fileURLToPath(new URL('./fixtures/reasoning-selection-ui.tsx', import.meta.url))], outfile: join(root, 'test.js'), bundle: true, platform: 'browser', jsx: 'automatic' });
  await writeFile(join(root, 'index.html'), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
  const driver = await readFile(new URL('./fixtures/project-information-ui-main.cjs', import.meta.url), 'utf8');
  await writeFile(join(root, 'driver.cjs'), driver.replace('show: false', 'show: true').replace('await window.loadFile', 'window.focus(); await window.loadFile'));
  let stdout;
  try { ({ stdout } = await promisify(execFile)(electron, [join(root, 'driver.cjs'), join(root, 'index.html')], { timeout: 30000, env: { ...process.env, ELECTRON_RUN_AS_NODE: '' } })); }
  catch (error) { throw Error(`${error.stdout ?? ''}\n${error.stderr ?? ''}`); }
  const result = JSON.parse(stdout.trim()); assert.equal(result.passed, true, result.error);
  assert.deepEqual(result.calls, [['fixture', 'Model', 'high'], ['fixture', 'Model', 'max']]);
});

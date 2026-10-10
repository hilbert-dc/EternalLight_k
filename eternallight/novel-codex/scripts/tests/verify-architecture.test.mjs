import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { checkJavaScript, checkRustMetadata, importEdges, verifyArchitecture } from "../verify-architecture.mjs";

function fixture(t, ancestors = []) {
  const base = realpathSync(mkdtempSync(join(tmpdir(), "novel-b01-")));
  const root = join(base, ...ancestors);
  t.after(() => rmSync(base, { recursive: true, force: true }));
  const projectRoot = join(root, "novel-codex");
  const engineRoot = join(root, "agent-engine");
  const write = (path, text = "export const value = 1;\n") => {
    mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, text); return path;
  };
  const project = (path, text) => write(join(projectRoot, path), text);
  const engine = (path, text) => write(join(engineRoot, path), text);
  project("src/types.ts", "export interface Value { text: string }\n");
  project("shared/value.ts"); project("electron/host.mjs");
  engine("src/index.mjs");
  engine("package.json", JSON.stringify({ exports: { ".": "./src/index.mjs", "./qualification/proof": "./qualification/proof.mjs", "./test-support": "./src/testing/index.mjs" } }));
  engine("qualification/proof.mjs"); engine("src/testing/index.mjs");
  return { projectRoot, engineRoot, project, engine, options: { projectRoot, engineRoot } };
}
function rejected(result, file, target) {
  const found = result.diagnostics.find(d => d.file === file && d.target === target);
  assert.ok(found, JSON.stringify(result)); assert.equal(found.rule, "B01"); assert.ok(found.line > 0); assert.ok(found.message);
}
function metadata(packages) { return { packages, workspace_members: packages.map(p => p.id) }; }
function crate(root, name, dependencies = []) {
  return { id: `fixture:${name}`, name, manifest_path: join(root, "Cargo.toml"), dependencies };
}

test("AST collects imports, re-exports, import types, require and dynamic imports without reading prose", () => {
  const result = importEdges("current.ts", `import type { A } from './one';
export { A } from './two'; export * from './three';
type T = import('./four').T; import B = require('./five');
const c = require('./six'); const d = import('./seven');
// import('node:fs');
const text = "require('electron')";`);
  assert.deepEqual(result.edges.map(e => e.target), ["./one", "./two", "./three", "./four", "./five", "./six", "./seven"]);
  assert.deepEqual(result.diagnostics, []);
});

test("current permitted browser/shared/Host/public Engine and Engine internal edges pass", t => {
  const f = fixture(t);
  f.project("src/view.tsx", "import React from 'react'; import type { Value } from './types'; import { value } from '../shared/value'; export { value };\n");
  f.project("shared/compute.mjs", "import { Tiktoken } from 'js-tiktoken/lite'; export { value } from './value';\n");
  f.project("electron/main.cjs", "const electron = require('electron'); const fs = require('node:fs'); import('@eternallight/agent-engine/qualification/proof');\n");
  f.engine("src/worker.mjs", "import { value } from './index.mjs'; import fs from 'node:fs';\n");
  assert.deepEqual(checkJavaScript(f.options).diagnostics, []);
  const projectAlias = join(dirname(f.projectRoot), "project-alias");
  const engineAlias = join(dirname(f.engineRoot), "engine-alias");
  symlinkSync(f.projectRoot, projectAlias, process.platform === "win32" ? "junction" : "dir");
  symlinkSync(f.engineRoot, engineAlias, process.platform === "win32" ? "junction" : "dir");
  assert.deepEqual(checkJavaScript({ projectRoot: projectAlias, engineRoot: engineAlias }).diagnostics, []);
});

test("Renderer rejects Node, Electron and Engine for every relevant import form", t => {
  const f = fixture(t);
  const file = f.project("src/forbidden.ts", "import type { X } from 'node:fs'; export * from 'electron'; type T = import('@eternallight/agent-engine').T; require('fs'); import('node:child_process');\n");
  const result = checkJavaScript(f.options);
  for (const target of ["node:fs", "electron", "@eternallight/agent-engine", "fs", "node:child_process"]) rejected(result, file, target);
});

test("Renderer rejects Host, Native, absolute paths and TypeScript alias bypasses", t => {
  const f = fixture(t);
  f.project("native/state.json", "{}");
  symlinkSync(join(f.projectRoot, "electron"), join(f.projectRoot, "src/linked-host"), process.platform === "win32" ? "junction" : "dir");
  f.project("tsconfig.json", JSON.stringify({ compilerOptions: { baseUrl: ".", paths: { "@private/*": ["electron/*"] } }, include: ["src"] }));
  const file = f.project("src/foreign.ts", `import '../electron/host.mjs'; import '../native/state.json'; import ${JSON.stringify(join(f.projectRoot, "electron/host.mjs"))}; import '@private/host.mjs'; import './linked-host/host.mjs';\n`);
  const result = checkJavaScript(f.options);
  for (const target of ["../electron/host.mjs", "../native/state.json", join(f.projectRoot, "electron/host.mjs"), "@private/host.mjs", "./linked-host/host.mjs"]) rejected(result, file, target);
});

test("shared rejects Renderer import type and Node side-effect dependencies", t => {
  const f = fixture(t);
  const file = f.project("shared/reversed.d.mts", "import type { Value } from '../src/types'; export * from 'node:fs';\n");
  const result = checkJavaScript(f.options);
  rejected(result, file, "../src/types"); rejected(result, file, "node:fs");
});

test("Host rejects Renderer, Native storage, Engine deep imports and local Engine sources", t => {
  const f = fixture(t);
  f.project("native/state.json", "{}");
  const file = f.project("electron/foreign.mjs", "import '../src/types'; import '../native/state.json'; import '@eternallight/agent-engine/src/index.mjs'; import '../../agent-engine/src/index.mjs'; import 'node:sqlite'; import 'better-sqlite3';\n");
  const result = checkJavaScript(f.options);
  for (const target of ["../src/types", "../native/state.json", "@eternallight/agent-engine/src/index.mjs", "../../agent-engine/src/index.mjs", "node:sqlite", "better-sqlite3"]) rejected(result, file, target);
});

test("Engine rejects domain and Codex sources, including re-exports", t => {
  const f = fixture(t);
  const codex = join(dirname(f.engineRoot), "codex-rs/protocol.mjs");
  mkdirSync(dirname(codex), { recursive: true }); writeFileSync(codex, "export const protocol = 1;\n");
  const file = f.engine("src/foreign.mjs", "export * from '../../novel-codex/shared/value'; import '../../codex-rs/protocol.mjs';\n");
  const result = checkJavaScript(f.options);
  rejected(result, file, "../../novel-codex/shared/value"); rejected(result, file, "../../codex-rs/protocol.mjs");
});

test("declaring a domain/Core or aliased external source dependency cannot legitimize Engine imports", t => {
  const f = fixture(t);
  const manifest = f.engine("package.json", JSON.stringify({ exports: { ".": "./src/index.mjs" },
    dependencies: { "novel-codex": "file:../novel-codex", "@openai/codex": "1", innocent: "file:../../codex-rs" } }));
  const file = f.engine("src/declared.mjs", "import 'novel-codex'; import '@openai/codex';\n");
  const result = checkJavaScript(f.options);
  for (const target of ["novel-codex", "@openai/codex", "innocent"]) rejected(result, manifest, target);
  rejected(result, file, "novel-codex"); rejected(result, file, "@openai/codex");
});

test("production cannot borrow tests through local imports, public test exports or support scripts", t => {
  const f = fixture(t);
  f.project("electron/tests/helper.mjs", "import '../../src/types';\n");
  const file = f.project("electron/main.mjs", "import './tests/helper.mjs'; import '@eternallight/agent-engine/test-support'; import '../scripts/production-support.mjs';\n");
  const support = f.project("scripts/production-support.mjs", "export * from './tests/helper.mjs';\n");
  f.project("scripts/tests/helper.mjs");
  const result = checkJavaScript(f.options);
  rejected(result, file, "./tests/helper.mjs"); rejected(result, file, "@eternallight/agent-engine/test-support");
  rejected(result, support, "./tests/helper.mjs");
});

test("support modules obey Host restrictions transitively rather than becoming exemptions", t => {
  const f = fixture(t);
  f.project("electron/main.mjs", "import '../scripts/provenance.mjs';\n");
  f.project("scripts/provenance.mjs", "import 'node:crypto'; export { value } from './nested.mjs';\n");
  const file = f.project("scripts/nested.mjs", "export * from '../src/types';\n");
  rejected(checkJavaScript(f.options), file, "../src/types");
});

test("unreferenced new production files are scanned while standalone tests are excluded", t => {
  const f = fixture(t);
  f.project("src/tests/intentional.mjs", "import '../../electron/host.mjs';\n");
  assert.deepEqual(checkJavaScript(f.options).diagnostics, []);
  const file = f.project("src/new/unreferenced.ts", "import 'electron';\n");
  rejected(checkJavaScript(f.options), file, "electron");
});

test("repository ancestors named tests/testing/fixtures cannot hide production files or diagnostics", t => {
  const f = fixture(t, ["tests", "testing", "fixtures"]);
  const baseline = checkJavaScript(f.options);
  assert.deepEqual(baseline.diagnostics, []);
  assert.ok(baseline.filesScanned >= 5, "must scan actual production under test-named ancestors");
  const renderer = f.project("src/unreferenced.ts", "import 'node:fs';\n");
  const shared = f.project("shared/reversed.d.mts", "import type { Value } from '../src/types';\n");
  const engine = f.engine("src/foreign.mjs", "import '../../novel-codex/shared/value';\n");
  f.project("src/tests/helper.mjs");
  const host = f.project("electron/borrowed.mjs", "import '../src/tests/helper.mjs';\n");
  const result = checkJavaScript(f.options);
  assert.ok(result.filesScanned >= baseline.filesScanned + 4);
  rejected(result, renderer, "node:fs"); rejected(result, shared, "../src/types");
  rejected(result, engine, "../../novel-codex/shared/value"); rejected(result, host, "../src/tests/helper.mjs");
});

test("computed imports, unresolved local edges and parser errors cannot pass silently", t => {
  const f = fixture(t);
  const file = f.project("src/unverifiable.ts", "const x = import(choice); require(target); import './missing';\n");
  const result = checkJavaScript(f.options);
  assert.equal(result.diagnostics.filter(d => d.file === file && d.target === "<computed>").length, 2);
  rejected(result, file, "./missing");
  const malformed = f.project("src/invalid.ts", "import { from '\n");
  rejected(checkJavaScript(f.options), malformed, "<syntax>");
});

test("Rust current production path matrix passes; test-only cross-layer deps do not grant production", t => {
  const f = fixture(t), n = join(f.projectRoot, "native"), e = join(f.engineRoot, "native");
  const core = crate(join(n, "story-core"), "core", [{ name: "fixture", path: join(n, "story-runtime"), kind: "dev" }]);
  const authoring = crate(join(n, "story-authoring"), "authoring", [{ name: "core", path: join(n, "story-core"), kind: null }]);
  const runtime = crate(join(n, "story-runtime"), "runtime", ["story-core", "story-authoring"].map(name => ({ name, path: join(n, name), kind: null }))
    .concat([{ name: "engine", path: join(e, "engine-core"), kind: null }]));
  const mcp = crate(join(n, "story-mcp"), "mcp", [{ name: "serde_json", kind: null }]);
  assert.deepEqual(checkRustMetadata(metadata([core, authoring, runtime, mcp]), f.options).diagnostics, []);
  const projectAlias = join(dirname(f.projectRoot), "project-alias");
  symlinkSync(f.projectRoot, projectAlias, process.platform === "win32" ? "junction" : "dir");
  const aliased = metadata([core, authoring, runtime, mcp].map(p => ({ ...p,
    manifest_path: p.manifest_path.replace(f.projectRoot, projectAlias),
    dependencies: p.dependencies.map(d => ({ ...d, ...(d.path ? { path: d.path.replace(f.projectRoot, projectAlias) } : {}) })),
  })));
  assert.deepEqual(checkRustMetadata(aliased, { projectRoot: projectAlias, engineRoot: f.engineRoot }).diagnostics, []);
  const guard = crate(join(e, "stream-guard"), "guard");
  const engine = crate(join(e, "engine-core"), "engine", [{ name: "guard", path: join(e, "stream-guard"), kind: null }]);
  assert.deepEqual(checkRustMetadata(metadata([guard, engine]), f.options).diagnostics, []);
});

test("Rust rejects Core upward, MCP database, renamed/build/target foreign dependencies and unclassified crates", t => {
  const f = fixture(t), n = join(f.projectRoot, "native"), e = join(f.engineRoot, "native");
  const core = crate(join(n, "story-core"), "renamed-core", [{ name: "innocent", rename: "safe", path: join(n, "story-runtime"), kind: "build", target: "cfg(windows)" }]);
  const mcp = crate(join(n, "story-mcp"), "mcp", [{ name: "rusqlite", kind: null }, { name: "core", path: join(n, "story-core"), kind: null }]);
  const engine = crate(join(e, "engine-core"), "engine", [{ name: "domain", path: join(n, "story-core"), kind: null }]);
  const unknown = crate(join(n, "new-crate"), "new");
  const result = checkRustMetadata(metadata([core, mcp, engine, unknown]), f.options);
  rejected(result, core.manifest_path, join(n, "story-runtime")); rejected(result, mcp.manifest_path, "rusqlite");
  rejected(result, mcp.manifest_path, join(n, "story-core")); rejected(result, engine.manifest_path, join(n, "story-core"));
  rejected(result, unknown.manifest_path, "new");
});

test("unified API accepts injected current metadata and retains precise B01 diagnostics", t => {
  const f = fixture(t);
  const file = f.project("src/new.ts", "import 'node:fs';\n");
  const result = verifyArchitecture({ ...f.options, cargoMetadata: [metadata([crate(join(f.projectRoot, "native/story-core"), "core")]),
    metadata([crate(join(f.engineRoot, "native/engine-core"), "engine")])] });
  assert.equal(result.rule, "B01"); assert.equal(result.cratesScanned, 2); assert.ok(result.filesScanned >= 5);
  rejected(result, file, "node:fs");
});

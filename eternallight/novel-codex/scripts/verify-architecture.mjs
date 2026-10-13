import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, realpathSync, statSync } from "node:fs";
import { builtinModules } from "node:module";
import { basename, dirname, isAbsolute, relative, resolve, sep, win32 } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

export const ARCHITECTURE_RULE = "B01";
const defaultRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const code = /\.(?:[cm]?[jt]sx?)$/;
const testing = /(?:^|\/)(?:tests?|testing|__tests__|fixtures|test-support)(?:\/|$)|\.(?:test|spec)\.[cm]?[jt]sx?$/;
const browserPackages = new Set(["react", "react-dom", "react-markdown", "remark-gfm", "js-tiktoken"]);
const purePackages = new Set(["js-tiktoken"]);
const databases = new Set(["sqlite", "sqlite3", "better-sqlite3", "sql.js", "rusqlite", "libsqlite3-sys", "sqlx", "diesel", "redb", "sled", "rocksdb", "sea-orm"]);
const domainOrCorePackages = new Set(["novel-codex", "@eternallight/novel-codex", "codex-rs", "codex-core", "@openai/codex"]);
const builtins = new Set(builtinModules.map(name => name.replace(/^node:/, "")));
const slash = value => value.split(sep).join("/");
const within = (file, directory) => { const r = relative(directory, file); return r === "" || !r.startsWith(`..${sep}`) && r !== ".." && !isAbsolute(r); };
const packageName = value => value.startsWith("@") ? value.split("/").slice(0, 2).join("/") : value.split("/")[0];
const issue = (file, target, message, line = 1) => ({ rule: ARCHITECTURE_RULE, file, line, target, message });

// Resolve actual aliases/symlinks, retaining the non-existent suffix for useful
// missing-dependency diagnostics and metadata fixtures. Never widen containment.
export function canonicalPath(path) {
  let current = resolve(path);
  const suffix = [];
  while (!existsSync(current)) {
    suffix.unshift(basename(current));
    const parent = dirname(current);
    if (parent === current) throw new Error(`cannot canonicalize path: ${path}`);
    current = parent;
  }
  return resolve(realpathSync(current), ...suffix);
}

// Type-only and re-export edges still form dependencies; comments and prose do not.
export function importEdges(file, text) {
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const edges = [];
  const add = (node, value, kind) => edges.push({ target: value && ts.isStringLiteralLike(value) ? value.text : null,
    kind, line: source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1 });
  function visit(node) {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
      if (node.moduleSpecifier) add(node, node.moduleSpecifier, ts.isImportDeclaration(node) ? "import" : "export");
    } else if (ts.isImportEqualsDeclaration(node) && ts.isExternalModuleReference(node.moduleReference)) {
      add(node, node.moduleReference.expression, "require");
    } else if (ts.isImportTypeNode(node)) {
      add(node, ts.isLiteralTypeNode(node.argument) ? node.argument.literal : node.argument, "import type");
    } else if (ts.isCallExpression(node) && (node.expression.kind === ts.SyntaxKind.ImportKeyword
      || ts.isIdentifier(node.expression) && node.expression.text === "require")) {
      add(node, node.arguments[0], node.expression.kind === ts.SyntaxKind.ImportKeyword ? "dynamic import" : "require");
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  return { edges, diagnostics: source.parseDiagnostics.map(d => issue(file, "<syntax>", ts.flattenDiagnosticMessageText(d.messageText, " "),
    source.getLineAndCharacterOfPosition(d.start ?? 0).line + 1)) };
}

function testPath(path, projectRoot, engineRoot) {
  const owner = within(path, projectRoot) ? projectRoot : within(path, engineRoot) ? engineRoot : null;
  return owner !== null && testing.test(slash(relative(owner, path)));
}

function sourceFiles(directory, ownerRoot, diagnostics) {
  if (!existsSync(directory)) { diagnostics.push(issue(directory, directory, "production source directory is missing")); return []; }
  const files = [];
  function walk(current) {
    for (const entry of readdirSync(current, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const path = resolve(current, entry.name);
      if (testing.test(slash(relative(ownerRoot, path)))) continue;
      if (entry.isSymbolicLink()) { diagnostics.push(issue(path, realpathSync(path), "production source symlinks require an explicit architectural placement")); continue; }
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile() && code.test(entry.name)) files.push(path);
    }
  }
  walk(directory);
  return files;
}

function compilerOptions(projectRoot) {
  const path = resolve(projectRoot, "tsconfig.json");
  if (!existsSync(path)) return { moduleResolution: ts.ModuleResolutionKind.Bundler, allowJs: true };
  const document = ts.readConfigFile(path, ts.sys.readFile);
  if (document.error) throw new Error(ts.flattenDiagnosticMessageText(document.error.messageText, " "));
  const config = ts.parseJsonConfigFileContent(document.config, ts.sys, projectRoot);
  if (config.errors.length) throw new Error(config.errors.map(e => ts.flattenDiagnosticMessageText(e.messageText, " ")).join("\n"));
  return { ...config.options, allowJs: true };
}

function localTarget(target, file, options) {
  const result = ts.resolveModuleName(target, file, options, ts.sys).resolvedModule;
  if (result && !result.isExternalLibraryImport) return result.resolvedFileName;
  if (target.startsWith(".") || isAbsolute(target) || win32.isAbsolute(target)) {
    const path = resolve(dirname(file), target);
    if (existsSync(path) && statSync(path).isFile()) return path;
    return { missing: path };
  }
  return null;
}

export function checkJavaScript({ projectRoot = defaultRoot, engineRoot = resolve(projectRoot, "../agent-engine") } = {}) {
  projectRoot = canonicalPath(projectRoot); engineRoot = canonicalPath(engineRoot);
  const roots = { renderer: resolve(projectRoot, "src"), shared: resolve(projectRoot, "shared"), host: resolve(projectRoot, "electron"),
    support: resolve(projectRoot, "scripts"), engine: engineRoot };
  const diagnostics = [];
  const enginePackage = JSON.parse(readFileSync(resolve(engineRoot, "package.json"), "utf8"));
  const publicExports = enginePackage.exports ?? {};
  const engineDependencies = new Set(Object.keys(enginePackage.dependencies ?? {}));
  for (const [name, location] of Object.entries(enginePackage.dependencies ?? {})) {
    if (domainOrCorePackages.has(name) || typeof location === "string" && /^(?:file|link):/.test(location)
      && !within(canonicalPath(resolve(engineRoot, location.slice(location.indexOf(":") + 1))), engineRoot)) {
      diagnostics.push(issue(resolve(engineRoot, "package.json"), name, "Engine cannot declare domain/Core or external source-tree dependencies"));
    }
  }
  const options = compilerOptions(projectRoot);
  const queue = [];
  for (const layer of ["renderer", "shared", "host"]) for (const file of sourceFiles(roots[layer], projectRoot, diagnostics)) queue.push({ file, layer });
  for (const file of sourceFiles(resolve(engineRoot, "src"), engineRoot, diagnostics)) queue.push({ file, layer: "engine" });
  // Current public qualification modules are executable production support too.
  for (const [name, target] of Object.entries(publicExports)) if (typeof target === "string" && !testing.test(name) && !testing.test(target)) {
    const file = resolve(engineRoot, target);
    if (code.test(file)) queue.push({ file, layer: "engine" });
  }
  const visited = new Set();
  while (queue.length) {
    const { file: lexical, layer } = queue.shift();
    if (!existsSync(lexical)) { diagnostics.push(issue(lexical, lexical, "production dependency is missing")); continue; }
    const file = realpathSync(lexical);
    if (visited.has(`${layer}:${file}`)) continue;
    visited.add(`${layer}:${file}`);
    const parsed = importEdges(file, readFileSync(file, "utf8"));
    diagnostics.push(...parsed.diagnostics);
    for (const edge of parsed.edges) {
      const reject = message => diagnostics.push(issue(file, edge.target ?? "<computed>", message, edge.line));
      if (edge.target === null) { reject("computed import/require has no statically verifiable dependency target"); continue; }
      const local = localTarget(edge.target, file, options);
      if (local?.missing) { reject(`unresolved local dependency: ${local.missing}`); continue; }
      if (local) {
        const path = realpathSync(local);
        if (testPath(path, projectRoot, engineRoot) || testPath(local, projectRoot, engineRoot)) { reject("production code cannot depend on tests, fixtures or test-support"); continue; }
        const allowed = layer === "renderer" ? within(path, roots.renderer) || within(path, roots.shared)
          : layer === "shared" ? within(path, roots.shared)
          : layer === "host" ? within(path, roots.host) || within(path, roots.shared) || within(path, roots.support)
          : within(path, engineRoot);
        if (!allowed) { reject(`${layer} cannot import this layer; Host must use Engine public package exports`); continue; }
        if (code.test(path)) queue.push({ file: path, layer: within(path, roots.shared) ? "shared" : layer });
        continue;
      }
      const name = packageName(edge.target);
      const builtin = builtins.has(edge.target.replace(/^node:/, ""));
      if (edge.target === "@eternallight/agent-engine" || edge.target.startsWith("@eternallight/agent-engine/")) {
        const key = edge.target === "@eternallight/agent-engine" ? "." : `.${edge.target.slice("@eternallight/agent-engine".length)}`;
        const target = publicExports[key];
        if (!["host", "engine"].includes(layer) || typeof target !== "string" || testing.test(key) || testing.test(target)) {
          reject("only Host/Engine may use non-test Engine public exports; deep imports are forbidden"); continue;
        }
        const path = canonicalPath(resolve(engineRoot, target));
        if (!within(path, engineRoot)) { reject("Engine export escapes its package"); continue; }
        queue.push({ file: path, layer: "engine" });
      } else if (layer === "renderer" || layer === "shared") {
        if (builtin || edge.target.startsWith("node:") || !(layer === "renderer" ? browserPackages : purePackages).has(name)) {
          reject(`${layer} may import only its permitted local layer and approved browser/pure packages`);
        }
      } else if (layer === "host") {
        if (databases.has(name) || edge.target === "node:sqlite" || edge.target === "sqlite" || !(builtin || edge.target === "electron" || browserPackages.has(name))) {
          reject("Host may not import database, private Engine, Codex source or unapproved execution dependencies");
        }
      } else if (domainOrCorePackages.has(name) || !(builtin || engineDependencies.has(name))) {
        reject("Engine may import only its own sources and declared generic runtime dependencies");
      }
    }
  }
  return { filesScanned: visited.size, diagnostics };
}

export function readCargoMetadata(manifestPath) {
  return JSON.parse(execFileSync("cargo", ["metadata", "--manifest-path", canonicalPath(manifestPath), "--no-deps", "--offline", "--locked", "--format-version", "1"],
    { encoding: "utf8", maxBuffer: 16 * 1024 * 1024, stdio: ["ignore", "pipe", "pipe"] }));
}

export function checkRustMetadata(metadata, { projectRoot = defaultRoot, engineRoot = resolve(projectRoot, "../agent-engine") } = {}) {
  projectRoot = canonicalPath(projectRoot); engineRoot = canonicalPath(engineRoot);
  const native = resolve(projectRoot, "native");
  const engineCore = resolve(engineRoot, "native/engine-core");
  const matrices = new Map([
    [resolve(native, "story-core"), new Set()],
    [resolve(native, "story-authoring"), new Set([resolve(native, "story-core")])],
    [resolve(native, "story-runtime"), new Set([resolve(native, "story-core"), resolve(native, "story-authoring"), engineCore])],
    [resolve(native, "story-mcp"), new Set()],
  ]);
  const diagnostics = [];
  const members = new Set(metadata.workspace_members);
  const packages = metadata.packages.filter(p => members.has(p.id));
  const engineMembers = new Set(packages.map(p => canonicalPath(dirname(p.manifest_path))).filter(path => within(path, engineRoot)));
  for (const p of packages) {
    const manifest = canonicalPath(p.manifest_path);
    const path = dirname(manifest);
    const engine = within(path, engineRoot);
    const allowed = engine ? engineMembers : matrices.get(path);
    if (!allowed) { diagnostics.push(issue(manifest, p.name, "unclassified production crate requires an explicit dependency boundary")); continue; }
    for (const d of p.dependencies) {
      if (d.kind === "dev") continue;
      if (path === resolve(native, "story-mcp") && databases.has(d.name)) {
        diagnostics.push(issue(manifest, d.name, "MCP cannot own a direct database dependency"));
      }
      if (d.path && !allowed.has(canonicalPath(d.path))) diagnostics.push(issue(manifest, d.path,
        engine ? "Engine production/build dependencies must remain inside its current workspace" : "production/build path dependency violates the Native crate matrix"));
    }
  }
  return { cratesScanned: packages.length, diagnostics };
}

export function verifyArchitecture({ projectRoot = defaultRoot, engineRoot = resolve(projectRoot, "../agent-engine"), cargoMetadata } = {}) {
  projectRoot = canonicalPath(projectRoot); engineRoot = canonicalPath(engineRoot);
  const js = checkJavaScript({ projectRoot, engineRoot });
  const documents = cargoMetadata ?? [readCargoMetadata(resolve(projectRoot, "native/Cargo.toml")), readCargoMetadata(resolve(engineRoot, "Cargo.toml"))];
  const rust = documents.map(document => checkRustMetadata(document, { projectRoot, engineRoot }));
  return { rule: ARCHITECTURE_RULE, filesScanned: js.filesScanned, cratesScanned: rust.reduce((n, r) => n + r.cratesScanned, 0),
    diagnostics: [...js.diagnostics, ...rust.flatMap(r => r.diagnostics)] };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = {};
    for (let i = 2; i < process.argv.length; i += 2) {
      const field = { "--project-root": "projectRoot", "--engine-root": "engineRoot" }[process.argv[i]];
      if (!field || !process.argv[i + 1]) throw new Error("usage: verify-architecture.mjs [--project-root PATH] [--engine-root PATH]");
      options[field] = canonicalPath(process.argv[i + 1]);
    }
    const result = verifyArchitecture(options);
    for (const d of result.diagnostics) process.stderr.write(`${d.rule} ${d.file}:${d.line} → ${d.target}: ${d.message}\n`);
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    if (result.diagnostics.length) process.exitCode = 1;
  } catch (error) {
    process.stderr.write(`${ARCHITECTURE_RULE} architecture check failed: ${error.message}\n`);
    process.exitCode = 1;
  }
}

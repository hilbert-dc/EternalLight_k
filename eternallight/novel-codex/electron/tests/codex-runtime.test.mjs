import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { CAPABILITIES, defineQualificationSuiteResult } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";

import { assertProviderOperation, buildCodexConfig, buildCodexModelCatalog, checkCodexConnectionAccess, composeNovelCodexPrompt, createCodexEnvironment, createQualificationContextDigest, createRuntimeEnvironment, createTaskCapability, credentialScopesFromEnvironment, inspectCodexConnection, installManagedAgentRoles, listCodexConnections, loadCodexEvidence, loadNovelCodexSkills, prepareIsolatedStorage, providerConfigurationError, resolveCodexCli, resolveCodexProvider, verifyCodexConnection } from "../codex-runtime.mjs";
import { initializeFixtureCodexRepository, writeFixtureBuildIdentity } from "./build-identity-fixture.mjs";
import { certificationPath } from "../../scripts/certification-publication.mjs";

function temporaryDirectory() {
  return mkdtempSync(join(tmpdir(), "novel-codex-runtime-"));
}

const runtimeRoot = fileURLToPath(new URL("../../codex-runtime/", import.meta.url));
const modelCatalogSha256 = createHash("sha256").update(readFileSync(join(runtimeRoot, "model-catalog.json"))).digest("hex");
const activeCertificationsPath = certificationPath(runtimeRoot, JSON.parse(readFileSync(join(runtimeRoot, "supported-cli.json"), "utf8")));
const modelCertificationsSha256 = createHash("sha256").update(readFileSync(activeCertificationsPath)).digest("hex");
const TEST_NOW = new Date("2026-09-19T01:00:00Z");
const TEST_BRIDGE = { baseUrl: "http://127.0.0.1:43123", credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" };

function verifiedProvider(capabilities = [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP]) {
  const inspected = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
  const evidence = verifiedBaselineEvidence(inspected.connection, {
    capabilities,
    protocol: "responses",
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  return resolveCodexProvider({ evidence, now: TEST_NOW, provider: inspected.provider, model: inspected.model });
}

function fakeExecutable(path) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, "#!/usr/bin/env node\nprocess.stdout.write('codex-cli 0.0.0\\n');\n");
  chmodSync(path, 0o755);
}

function executableChecksum(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function writeCliPolicy(root, digests = {}, buildIdentities = {}) {
  const path = join(root, "codex-runtime", "supported-cli.json");
  mkdirSync(dirname(path), { recursive: true });
  copyFileSync(join(runtimeRoot, "model-catalog.json"), join(root, "codex-runtime", "model-catalog.json"));
  copyFileSync(activeCertificationsPath, join(root, "codex-runtime", "model-certifications.json"));
  const versions = ["codex-cli 0.0.0"];
  writeFileSync(path, JSON.stringify({ version: 1, modelCertificationsFile: "model-certifications.json", appServerProtocol: "v2", mcpProtocol: "2025-06-18", modelCatalogSha256, modelCertificationsSha256, sources: { bundled: { packaged: true, versions, digests: digests.bundled ?? {}, buildIdentities: buildIdentities.bundled ?? {} }, "project-runtime": { packaged: false, versions, digests: digests["project-runtime"] ?? {}, buildIdentities: buildIdentities["project-runtime"] ?? {} }, "workspace-build": { packaged: false, developmentOnly: true, versions, digests: digests["workspace-build"] ?? {}, buildIdentities: buildIdentities["workspace-build"] ?? {} }, explicit: { packaged: false, developmentOnly: true, versions, digests: digests.explicit ?? {}, buildIdentities: buildIdentities.explicit ?? {} } } }));
  const bundledIdentity = buildIdentities.bundled?.["darwin-arm64"] ?? null;
  writeFileSync(join(root, "release-manifest.json"), JSON.stringify({ version: 1, target: { platform: "darwin", architecture: "arm64" }, codex: { binarySha256: digests.bundled?.["darwin-arm64"] ?? null, unsignedBinarySha256: bundledIdentity?.unsignedBinarySha256 ?? null, buildIdentityId: bundledIdentity?.identityId ?? null, buildIdentitySha256: bundledIdentity?.sha256 ?? null }, resources: {} }));
}

test("resolves the packaged CLI instead of PATH", () => {
  const resourcesPath = temporaryDirectory();
  const cliPath = join(resourcesPath, "codex-runtime", "bin", "codex");
  fakeExecutable(cliPath);
  const identityPath = join(resourcesPath, "codex-runtime", "build-identity.json");
  const { identity, sha256 } = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: cliPath, identityPath, platform: "darwin" });
  writeCliPolicy(resourcesPath, { bundled: { "darwin-arm64": executableChecksum(cliPath) } }, { bundled: { "darwin-arm64": { identityId: identity.identityId, sha256, unsignedBinarySha256: identity.binary.sha256 } } });
  try {
    assert.deepEqual(resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath }), {
      path: realpathSync(cliPath),
      source: "bundled",
      reason: null,
    });
  } finally {
    rmSync(resourcesPath, { recursive: true, force: true });
  }
});

test("requires packaged BuildIdentity provenance while allowing signed executable bytes", () => {
  const resourcesPath = temporaryDirectory();
  const cliPath = join(resourcesPath, "codex-runtime", "bin", "codex");
  fakeExecutable(cliPath);
  writeCliPolicy(resourcesPath);
  try {
    const missing = resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath });
    assert.equal(missing.path, null);
    assert.match(missing.reason, /BuildIdentity/);
    const identityPath = join(resourcesPath, "codex-runtime", "build-identity.json");
    const recorded = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: cliPath, identityPath, platform: "darwin" });
    writeFileSync(cliPath, "#!/usr/bin/env node\nprocess.stdout.write('codex-cli 0.0.0\\n');\n// changed after build\n");
    writeCliPolicy(resourcesPath, { bundled: { "darwin-arm64": executableChecksum(cliPath) } }, { bundled: { "darwin-arm64": { identityId: recorded.identity.identityId, sha256: recorded.sha256, unsignedBinarySha256: recorded.identity.binary.sha256 } } });
    const signed = resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath });
    assert.equal(signed.path, realpathSync(cliPath));

    writeCliPolicy(resourcesPath, { bundled: { "darwin-arm64": executableChecksum(cliPath) } }, { bundled: { "darwin-arm64": { identityId: recorded.identity.identityId, sha256: recorded.sha256, unsignedBinarySha256: "0".repeat(64) } } });
    const stale = resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath });
    assert.equal(stale.path, null);
    assert.match(stale.reason, /平台 SHA-256 或版本/);
  } finally {
    rmSync(resourcesPath, { recursive: true, force: true });
  }
});

test("requires a SHA manifest for a project runtime and permits only the development workspace build fallback", () => {
  const root = temporaryDirectory();
  const projectRoot = join(root, "app", "novel");
  const projectCli = join(projectRoot, "codex-runtime", "bin", "codex");
  const workspaceCli = join(projectRoot, "..", "..", "codex-rs", "target", "debug", "codex");
  const sourceTreeDigest = initializeFixtureCodexRepository(root);
  fakeExecutable(projectCli);
  fakeExecutable(workspaceCli);
  const identityPath = join(projectRoot, "codex-runtime", "build-identities", "darwin-arm64.json");
  const workspaceIdentity = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: workspaceCli, identityPath, platform: "darwin", repositoryRoot: root, sourceTreeDigest });
  writeCliPolicy(projectRoot, { "workspace-build": { "darwin-arm64": executableChecksum(workspaceCli) } }, { "workspace-build": { "darwin-arm64": { identityId: workspaceIdentity.identity.identityId, sha256: workspaceIdentity.sha256 } } });
  try {
    const fallback = resolveCodexCli({ architecture: "arm64", isPackaged: false, platform: "darwin", projectRoot, resourcesPath: "/unused" });
    assert.equal(fallback.source, "workspace-build");
    assert.equal(fallback.path, realpathSync(workspaceCli));
    const projectIdentity = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: projectCli, identityPath, platform: "darwin", repositoryRoot: root, sourceTreeDigest });
    writeCliPolicy(projectRoot, { "project-runtime": { "darwin-arm64": executableChecksum(projectCli) }, "workspace-build": { "darwin-arm64": executableChecksum(workspaceCli) } }, { "project-runtime": { "darwin-arm64": { identityId: projectIdentity.identity.identityId, sha256: projectIdentity.sha256 } } });
    const pinned = resolveCodexCli({ architecture: "arm64", isPackaged: false, platform: "darwin", projectRoot, resourcesPath: "/unused" });
    assert.equal(pinned.source, "project-runtime");
    assert.equal(pinned.path, realpathSync(projectCli));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("rejects an explicit CLI in a packaged build", () => {
  const root = temporaryDirectory();
  const bundledCli = join(root, "codex-runtime", "bin", "codex");
  fakeExecutable(bundledCli);
  const identityPath = join(root, "codex-runtime", "build-identity.json");
  const bundledIdentity = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: bundledCli, identityPath, platform: "darwin" });
  writeCliPolicy(root, { bundled: { "darwin-arm64": executableChecksum(bundledCli) } }, { bundled: { "darwin-arm64": { identityId: bundledIdentity.identity.identityId, sha256: bundledIdentity.sha256, unsignedBinarySha256: bundledIdentity.identity.binary.sha256 } } });
  rmSync(bundledCli);
  const desktopCli = join(root, "Applications", "ChatGPT.app", "Contents", "Resources", "codex");
  fakeExecutable(desktopCli);
  try {
    const result = resolveCodexCli({ architecture: "arm64", explicitPath: desktopCli, isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath: root });
    assert.equal(result.path, null);
    assert.equal(result.source, "missing");
    assert.match(result.reason, /未获.*授权/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("rejects a digest-pinned CLI whose reported version is not supported", () => {
  const resourcesPath = temporaryDirectory();
  const cliPath = join(resourcesPath, "codex-runtime", "bin", "codex");
  mkdirSync(dirname(cliPath), { recursive: true });
  writeFileSync(cliPath, "#!/usr/bin/env node\nprocess.stdout.write('codex-cli 99.0.0\\n');\n");
  chmodSync(cliPath, 0o755);
  const identityPath = join(resourcesPath, "codex-runtime", "build-identity.json");
  const wrongVersionIdentity = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: cliPath, identityPath, platform: "darwin" });
  writeCliPolicy(resourcesPath, { bundled: { "darwin-arm64": executableChecksum(cliPath) } }, { bundled: { "darwin-arm64": { identityId: wrongVersionIdentity.identity.identityId, sha256: wrongVersionIdentity.sha256, unsignedBinarySha256: wrongVersionIdentity.identity.binary.sha256 } } });
  try {
    const result = resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath });
    assert.equal(result.path, null);
    assert.match(result.reason, /平台 SHA-256 或版本/);
  } finally {
    rmSync(resourcesPath, { recursive: true, force: true });
  }
});

test("keeps inherited provider credentials out of the isolated Codex child environment", () => {
  const env = createCodexEnvironment({ CODEX_HOME: "/desktop", CODEX_API_KEY: "implicit-key", CODEX_ACCESS_TOKEN: "secret", OPENAI_API_KEY: "key", PATH: "/usr/bin" }, {
    codexHome: "/novel/codex-home",
    provider: "deepseek",
    sqliteHome: "/novel/codex-home/sqlite",
  });
  assert.equal(env.CODEX_HOME, "/novel/codex-home");
  assert.equal(env.CODEX_SQLITE_HOME, "/novel/codex-home/sqlite");
  assert.equal(env.CODEX_API_KEY, undefined);
  assert.equal(env.CODEX_ACCESS_TOKEN, undefined);
  assert.equal(env.OPENAI_API_KEY, undefined);
  assert.equal(env.PATH, "/usr/bin");
});

test("qualification context binds the current CLI and upstream instructions", () => {
  const root = temporaryDirectory();
  try {
    const cliPath = join(root, "codex");
    fakeExecutable(cliPath);
    const first = createQualificationContextDigest({ baseInstructions: "instruction-v1", cliPath });
    const second = createQualificationContextDigest({ baseInstructions: "instruction-v2", cliPath });
    assert.match(first, /^[a-f0-9]{64}$/);
    assert.notEqual(first, second);
    writeFileSync(cliPath, "#!/usr/bin/env node\nprocess.stdout.write('codex-cli 0.0.1\\n');\n");
    chmodSync(cliPath, 0o755);
    assert.notEqual(second, createQualificationContextDigest({ baseInstructions: "instruction-v2", cliPath }));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("allows only catalogued provider/model operations", () => {
  assert.throws(() => resolveCodexProvider({}), /lacks verified capabilities/);
  const expected = verifiedProvider();
  assert.equal(expected.id, "deepseek-flash");
  assert.equal(expected.provider, "DeepSeek");
  assert.equal(expected.model, "deepseek-flash");
  assert.equal(expected.credentialRef, "DEEPSEEK_API_KEY");
  assert.equal(expected.sessionPlan.routes[0].transport, "native-passthrough");
  for (const operation of ["advisory", "generateBrief", "generateOutline", "generatePlan", "generateChapter", "reviewPlanningSeat"]) {
    assert.equal(assertProviderOperation(expected, operation).model, "deepseek-flash");
  }
  assert.throws(() => resolveCodexProvider({ model: "gpt-5.6-terra", provider: "openai" }), /未知模型连接/);
  assert.throws(() => resolveCodexProvider({ model: "gpt-arbitrary", provider: "openai" }), /未知模型连接/);
  assert.throws(() => resolveCodexProvider({ model: "DeepSeek-V4.1-Flash", provider: "deepseek" }), /未知模型连接/);
});

test("gates each product operation by a verified capability profile without changing the frozen plan", () => {
  const inspected = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
  const baseEvidence = verifiedBaselineEvidence(inspected.connection, {
    capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP],
    protocol: "responses",
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  const baseAdvisory = resolveCodexProvider({ evidence: baseEvidence, now: TEST_NOW, operation: "advisory", provider: inspected.provider, model: inspected.model });
  assert.equal(baseAdvisory.operationProfileId, "base-tools-v1");

  const interactiveEvidence = verifiedBaselineEvidence(inspected.connection, {
    capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING, CAPABILITIES.USAGE],
    protocol: "responses",
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  const configured = resolveCodexProvider({ evidence: interactiveEvidence, now: TEST_NOW, provider: inspected.provider, model: inspected.model });
  const advisory = resolveCodexProvider({ evidence: interactiveEvidence, now: TEST_NOW, operation: "advisory", provider: inspected.provider, model: inspected.model });
  assert.equal(advisory.operationProfileId, "base-tools-v1");
  assert.equal(advisory.sessionPlan.id, configured.sessionPlan.id);
  const baseReview = resolveCodexProvider({ evidence: interactiveEvidence, now: TEST_NOW, operation: "reviewPlanningSeat", provider: inspected.provider, model: inspected.model });
  assert.equal(baseReview.operationProfileId, "base-tools-v1");

  const autonomousEvidence = verifiedBaselineEvidence(inspected.connection, {
    protocol: "responses",
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  const review = resolveCodexProvider({ evidence: autonomousEvidence, now: TEST_NOW, operation: "reviewPlanningSeat", provider: inspected.provider, model: inspected.model });
  assert.equal(review.operationProfileId, "base-tools-v1");
  assert.deepEqual(review.sessionPlan.authorizedCapabilityProfiles, ["base-tools-v1", "directed-delegation-v1", "interactive-v1", "text-v1"]);
});

test("generates model capabilities from the frozen session plan", () => {
  const catalog = buildCodexModelCatalog({ baseInstructions: "Upstream Codex test instructions.", sessionPlan: verifiedProvider().sessionPlan });
  assert.deepEqual(catalog.models.map((model) => ({
    includeApps: model.include_apps_usage_instructions,
    includePlugins: model.include_plugin_usage_instructions,
    multiAgentVersion: model.multi_agent_version,
    nodeReplDisabled: model.node_repl_disabled,
    toolMode: model.tool_mode,
    useResponsesLite: model.use_responses_lite,
  })), [{
    includeApps: false,
    includePlugins: false,
    multiAgentVersion: null,
    nodeReplDisabled: true,
    toolMode: "direct",
    useResponsesLite: false,
  }]);
});

test("creates a short-lived task capability without exposing its hash as the secret", () => {
  const capability = createTaskCapability(1_800_000);
  assert.equal(capability.secret.length >= 24, true);
  assert.equal(capability.hash, createHash("sha256").update(capability.secret).digest("hex"));
  assert.equal(capability.expiresAt, 3_600);
  assert.notEqual(capability.secret, capability.hash);
});

test("starts the current runtime schema in a fresh database", () => {
  const root = temporaryDirectory();
  try {
    const storage = prepareIsolatedStorage(root);
    assert.equal(storage.runtimeDb, join(root, "creative-runtime", "novel-codex-v2.sqlite3"));
    assert.equal(storage.evidencePath, join(root, "agent-engine", "capability-evidence.json"));
    assert.equal(storage.recoveryPath, join(root, "agent-engine", "recovery-index.json"));
    assert.equal(storage.credentialSaltPath, join(root, "agent-engine", "credential-scope.key"));
    assert.match(readFileSync(storage.credentialSaltPath, "utf8").trim(), /^[A-Za-z0-9_-]{43}$/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("invalidates capability evidence after a credential rotation without persisting either credential", () => {
  const root = temporaryDirectory();
  try {
    const storage = prepareIsolatedStorage(root);
    const provider = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
    const firstScopes = credentialScopesFromEnvironment({ DEEPSEEK_API_KEY: "first-credential" }, storage.credentialSaltPath);
    const repeatedScopes = credentialScopesFromEnvironment({ DEEPSEEK_API_KEY: "first-credential" }, storage.credentialSaltPath);
    const rotatedScopes = credentialScopesFromEnvironment({ DEEPSEEK_API_KEY: "rotated-credential" }, storage.credentialSaltPath);
    assert.deepEqual(firstScopes, repeatedScopes);
    assert.notEqual(firstScopes[provider.id], rotatedScopes[provider.id]);
    const evidence = verifiedBaselineEvidence(provider.connection, {
      credentialScope: firstScopes[provider.id],
      protocol: "responses",
      checkedAt: "2026-09-19T00:00:00Z",
      expiresAt: "2026-09-20T00:00:00Z",
    });
    assert.equal(resolveCodexProvider({ credentialScopes: firstScopes, evidence, now: TEST_NOW, provider: provider.provider, model: provider.model }).sessionPlan.routes[0].connectionId, provider.id);
    assert.throws(
      () => resolveCodexProvider({ credentialScopes: rotatedScopes, evidence, now: TEST_NOW, provider: provider.provider, model: provider.model }),
      /lacks verified capabilities/,
    );
    assert.doesNotMatch(JSON.stringify({ firstScopes, rotatedScopes, salt: readFileSync(storage.credentialSaltPath, "utf8") }), /first-credential|rotated-credential/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("persists explicit connection verification evidence without storing the credential", async () => {
  const root = temporaryDirectory();
  try {
    const storage = prepareIsolatedStorage(root);
    const provider = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
    const credentialScopes = credentialScopesFromEnvironment({ DEEPSEEK_API_KEY: "test-secret-never-persist" }, storage.credentialSaltPath);
    const records = verifiedBaselineEvidence(provider.connection, {
      capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.STREAMING, CAPABILITIES.USAGE],
      credentialScope: credentialScopes[provider.id],
      protocol: "responses",
      checkedAt: "2026-09-19T00:00:00Z",
      expiresAt: "2026-09-20T00:00:00Z",
    });
    const result = await verifyCodexConnection({
      baseEnvironment: { DEEPSEEK_API_KEY: "test-secret-never-persist" },
      credentialScopes,
      evidencePath: join(root, "evidence.json"),
      provider: "deepseek",
      model: "deepseek-flash",
      coreRunner: { verify: async ({ protocol, now, credentialScope }) => ({
        kind: "core-integration",
        suiteId: "codex-engine-core-integration-v1",
        suiteVersion: 1,
        status: "passed",
        sampleCount: 1,
        successes: 1,
        blockingFailures: [],
        metrics: {},
        record: {
          capability: CAPABILITIES.TOOL_LOOP,
          status: "verified",
          scope: { protocol, suite: "codex-engine-core-integration-v1", mode: "app-server-mcp", toolTypes: ["function"] },
          source: "conformance-suite",
          credentialRealm: records[0].credentialRealm,
          environmentDigest: records[0].environmentDigest,
          evidenceRef: `core:${now.toISOString()}`,
          checkedAt: now.toISOString(),
          expiresAt: new Date(now.getTime() + 86_400_000).toISOString(),
          sample: { suiteId: "codex-engine-core-integration-v1", suiteVersion: 1, outcome: "passed" },
          assertions: [{ id: "tool-loop-completion", result: "passed", expected: true, observed: true, traceRef: "fixture-core" }],
          dependencies: records[0].dependencies,
          supersedes: [],
        },
      }) },
      conformanceRunner: { verify: async ({ apiKey, credentialScope, now, probeEvidence, protocol }) => {
        assert.equal(apiKey, "test-secret-never-persist");
        assert.equal(credentialScope, credentialScopes[provider.id]);
        assert.equal(protocol, "responses");
        assert.deepEqual(probeEvidence, records);
        return {
          attempt: { protocol, suite: "codex-engine-delegation-execution-v1", status: "verified" },
          records: [CAPABILITIES.CHILD_SPAWN, CAPABILITIES.CHILD_MESSAGE_RUNNING, CAPABILITIES.CHILD_WAIT, CAPABILITIES.CHILD_DIRECTED_ROUTE].map((capability) => ({
            capability,
            status: "verified",
            scope: { protocol, suite: "codex-engine-delegation-execution-v1", mode: "app-server-mcp", toolTypes: ["function"], ...(capability === CAPABILITIES.CHILD_DIRECTED_ROUTE ? { routes: [{ parentConnectionId: provider.id, childConnectionId: provider.id }] } : {}) },
            source: "conformance-suite",
            environmentDigest: records[0].environmentDigest,
            evidenceRef: `conformance:${provider.connection.id}:${now.toISOString()}:${capability}`,
            checkedAt: now.toISOString(),
            expiresAt: new Date(now.getTime() + 86_400_000).toISOString(),
            sample: { suiteId: "codex-engine-delegation-execution-v1", suiteVersion: 1, outcome: "passed", eventDigest: "a".repeat(64) },
            assertions: [{ id: `fixture:${capability}`, result: "passed", expected: true, observed: true, traceRef: "a".repeat(64) }],
            dependencies: records[0].dependencies,
            supersedes: [],
          })),
        };
      } },
      verifier: { verify: async (_connection, { apiKey, credentialScope }) => {
        assert.equal(apiKey, "test-secret-never-persist");
        assert.equal(credentialScope, credentialScopes[provider.id]);
        return { selectedProtocol: "responses", attempts: [{ protocol: "responses", status: "verified", message: "provider debug: test-secret-never-persist" }], records };
      } },
    });
    assert.equal(result.selectedProtocol, "responses");
    assert.match(result.reportId, /^[a-f0-9]{64}$/);
    const storedEvidence = await loadCodexEvidence(join(root, "evidence.json"));
    assert.equal(storedEvidence.length, records.length + 5);
    const childRoute = storedEvidence.find((record) => record.capability === CAPABILITIES.CHILD_DIRECTED_ROUTE);
    assert.equal(childRoute.reportRef, result.reportId);
    assert.equal(childRoute.reportDigest, result.reportId);
    assert.doesNotMatch(readFileSync(join(root, "evidence.json"), "utf8"), /test-secret-never-persist/);
    const reports = readdirSync(join(root, "capability-reports"));
    assert.equal(reports.length, 1);
    const report = JSON.parse(readFileSync(join(root, "capability-reports", reports[0]), "utf8"));
    assert.equal(report.reportId, result.reportId);
    assert.equal(report.status, "verified");
    assert.equal(report.connection.id, "deepseek-flash");
    assert.doesNotMatch(JSON.stringify(report), /test-secret-never-persist/);
    const evidencePath = join(root, "evidence.json");
    const tampered = JSON.parse(readFileSync(evidencePath, "utf8"));
    const tamperedRecord = tampered.records.find((record) => record.capability === CAPABILITIES.CHILD_DIRECTED_ROUTE);
    tamperedRecord.expiresAt = "2026-09-21T00:00:00.000Z";
    writeFileSync(evidencePath, JSON.stringify(tampered));
    await assert.rejects(loadCodexEvidence(evidencePath), /fields? (?:has|have) been tampered|字段已被篡改/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("published connections use a bounded account check without rerunning conformance", async () => {
  const provider = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
  const result = await checkCodexConnectionAccess({
    apiKey: "account-secret",
    connection: provider.connection,
    fetchImpl: async (url, options) => {
      assert.equal(url, "https://api.deepseek.com/v1/responses");
      assert.equal(options.headers.authorization, "Bearer account-secret");
      const body = JSON.parse(options.body);
      assert.equal(body.tools.length, 0);
      assert.equal(body.tool_choice, undefined);
      return new Response(JSON.stringify({ id: "account-check", status: "completed", output: [{ type: "message", role: "assistant", content: [{ type: "output_text", text: "ok" }] }] }), { status: 200 });
    },
  });
  assert.deepEqual(result, { status: "available", protocol: "responses" });
  await assert.rejects(checkCodexConnectionAccess({
    apiKey: "invalid",
    connection: provider.connection,
    fetchImpl: async () => new Response(JSON.stringify({ error: { message: "unauthorized" } }), { status: 401 }),
  }), (error) => error.statusCode === 401);
});

test("failed core tool-loop evidence prevents child conformance from running", async () => {
  const root = temporaryDirectory();
  let childConformanceCalls = 0;
  try {
    const provider = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
    const records = verifiedBaselineEvidence(provider.connection, {
      capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS],
      credentialScope: provider.connection.credentialRef,
      protocol: "responses",
    });
    const result = await verifyCodexConnection({
      baseEnvironment: { DEEPSEEK_API_KEY: "core-failure-secret" },
      evidencePath: join(root, "evidence.json"),
      provider: "deepseek",
      model: "deepseek-flash",
      verifier: { verify: async () => ({ selectedProtocol: "responses", attempts: [], records }) },
      coreRunner: { verify: async () => ({
        ...defineQualificationSuiteResult({ kind: "core-integration", suiteId: "codex-engine-core-integration-v1", suiteVersion: 1, status: "failed", blockingFailures: ["core-tool-loop-error"] }),
        record: {
          capability: CAPABILITIES.TOOL_LOOP,
          status: "failed",
          scope: { protocol: "responses", suite: "codex-engine-core-integration-v1", mode: "app-server-mcp", toolTypes: ["function"] },
          source: "conformance-suite",
          credentialRealm: provider.connection.credentialRef,
          environmentDigest: records[0].environmentDigest,
          evidenceRef: "core-failed",
          checkedAt: TEST_NOW.toISOString(),
          expiresAt: new Date(TEST_NOW.getTime() + 86_400_000).toISOString(),
          sample: { suiteId: "codex-engine-core-integration-v1", suiteVersion: 1, outcome: "failed" },
          assertions: [{ id: "tool-loop-completion", result: "failed", expected: true, observed: false, traceRef: "core-failed" }],
          dependencies: records[0].dependencies,
          supersedes: [],
        },
      }) },
      conformanceRunner: { verify: async () => { childConformanceCalls += 1; throw new Error("must not run"); } },
    });
    assert.equal(childConformanceCalls, 0);
    assert.equal(result.attempts.at(-1).status, "failed");
    const evidence = await loadCodexEvidence(join(root, "evidence.json"));
    assert.equal(evidence.some((record) => record.capability === CAPABILITIES.TOOL_LOOP && record.status === "failed"), true);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("persists a redacted failed capability report", async () => {
  const root = temporaryDirectory();
  try {
    await assert.rejects(
      verifyCodexConnection({
        baseEnvironment: { DEEPSEEK_API_KEY: "failed-secret-never-persist" },
        evidencePath: join(root, "evidence.json"),
        provider: "deepseek",
        model: "deepseek-flash",
        verifier: { verify: async () => {
          const error = new Error("provider echoed failed-secret-never-persist");
          error.attempts = [{ protocol: "responses", status: "failed", category: "authentication", message: "failed-secret-never-persist" }];
          error.records = [];
          throw error;
        } },
      }),
      (error) => {
        assert.match(error.reportId, /^[a-f0-9]{64}$/);
        assert.doesNotMatch(error.message, /failed-secret-never-persist/);
        return true;
      },
    );
    const reports = readdirSync(join(root, "capability-reports"));
    assert.equal(reports.length, 1);
    const report = readFileSync(join(root, "capability-reports", reports[0]), "utf8");
    assert.match(report, /\[REDACTED\]/);
    assert.doesNotMatch(report, /failed-secret-never-persist/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("installs only managed read-only Agent roles into the isolated Codex home", () => {
  const root = temporaryDirectory();
  try {
    const codexHome = join(root, "codex-home");
    mkdirSync(join(codexHome, "agents"), { recursive: true });
    writeFileSync(join(codexHome, "agents", "stale.toml"), "stale = true\n");
    const roles = installManagedAgentRoles(codexHome, join(runtimeRoot, "agents"));
    assert.deepEqual(roles, ["continuity_critic.toml", "default.toml", "explorer.toml", "story_architect.toml", "worker.toml"]);
    assert.equal(existsSync(join(codexHome, "agents", "stale.toml")), false);
    for (const role of roles) {
      const contents = readFileSync(join(codexHome, "agents", role), "utf8");
      assert.match(contents, /sandbox_mode = "read-only"/);
      assert.match(contents, /enabled_tools = \["read_context"\]/);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("builds an isolated Agent Engine provider without persisting the downstream API key", () => {
  const config = buildCodexConfig({ sessionPlan: verifiedProvider().sessionPlan, bridge: TEST_BRIDGE, modelCatalogPath: "/runtime/model-catalog.json", mcpServerCommand: "/runtime/novel-workspace-mcp" });
  assert.match(config, /model_provider = "agent_engine"/);
  assert.match(config, /model_catalog_json = /);
  assert.match(config, /base_url = "http:\/\/127\.0\.0\.1:43123"/);
  assert.match(config, /env_key = "AGENT_ENGINE_BRIDGE_TOKEN"/);
  assert.doesNotMatch(config, /sk-/);
  assert.match(config, /\[shell_environment_policy\]/);
  assert.match(config, /inherit = "core"/);
  assert.match(config, /ignore_default_excludes = false/);
  assert.match(config, /"NOVEL_CODEX_\*"/);
  assert.match(config, /"\*_API_KEY"/);
  assert.match(config, /"CODEX_API_KEY"/);
  assert.match(config, /web_search = "disabled"/);
  assert.match(config, /approval_policy = "never"/);
  assert.match(config, /sandbox_mode = "read-only"/);
  for (const feature of ["multi_agent", "tool_search", "memories", "apps", "computer_use", "in_app_browser", "image_generation", "js_repl"]) {
    assert.match(config, new RegExp(`${feature} = false`));
  }
  assert.match(config, /\[features\.multi_agent_v2\]/);
  assert.match(config, /\[features\.multi_agent_v2\]\nenabled = false/);
  assert.match(config, /non_code_mode_only = false/);
  assert.match(config, /expose_spawn_agent_model_overrides = false/);
  assert.match(config, /\[agents\]/);
  assert.match(config, /max_concurrent_threads_per_session = 3/);
});

test("places the selected provider at the TOML root without a Python parser", () => {
  const config = buildCodexConfig({ sessionPlan: verifiedProvider().sessionPlan, bridge: TEST_BRIDGE, modelCatalogPath: "/runtime/model-catalog.json", mcpServerCommand: "/runtime/novel-workspace-mcp" });
  assert.match(config, /^model_provider = "agent_engine"$/m);
  const shellPolicy = config.slice(config.indexOf("[shell_environment_policy]"), config.indexOf("[features]"));
  assert.doesNotMatch(shellPolicy, /model_provider/);
  assert.match(config, /\[model_providers\.agent_engine\][\s\S]*name = "Eternal Light Agent Engine"/);
  assert.match(config, /\[features\][\s\S]*multi_agent = false/);
  assert.match(config, /\[features\.multi_agent_v2\][\s\S]*enabled = false/);
  assert.match(config, /\[agents\][\s\S]*enabled = false[\s\S]*max_concurrent_threads_per_session = 3/);
});

test("builds a cross-platform candidate-only MCP sidecar config", () => {
  const config = buildCodexConfig({
    mcpServerCommand: "C:\\Novel Codex\\runtime\\novel-workspace-mcp.exe",
    modelCatalogPath: "C:\\Novel Codex\\model-catalog.json",
    bridge: TEST_BRIDGE,
    sessionPlan: verifiedProvider().sessionPlan,
    platform: "win32",
    provider: "deepseek",
  });
  assert.match(config, /\[mcp_servers\.novel_workspace\]/);
  assert.match(config, /command = "C:\\\\Novel Codex\\\\runtime\\\\novel-workspace-mcp\.exe"/);
  assert.match(config, /args = \[\]/);
  assert.match(config, /env_vars = \["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"\]/);
  assert.match(config, /default_tools_approval_mode = "approve"/);
  assert.doesNotMatch(config, /AUTHOR_TOKEN|candidate-secret|accept|undo/);
});

test("passes only the key for the explicitly selected provider", () => {
  const base = { DEEPSEEK_API_KEY: "deepseek-secret", OPENAI_API_KEY: "openai-secret", PATH: "/usr/bin" };
  const deepSeekEnv = createCodexEnvironment(base, { codexHome: "/novel/home", sqliteHome: "/novel/sqlite", credentialRef: "DEEPSEEK_API_KEY" });
  const openAiEnv = createCodexEnvironment(base, { codexHome: "/novel/home", sqliteHome: "/novel/sqlite", credentialRef: "OPENAI_API_KEY" });
  assert.equal(deepSeekEnv.DEEPSEEK_API_KEY, "deepseek-secret");
  assert.equal(openAiEnv.DEEPSEEK_API_KEY, undefined);
  assert.equal(deepSeekEnv.OPENAI_API_KEY, undefined);
  assert.equal(openAiEnv.OPENAI_API_KEY, "openai-secret");
});

test("fresh isolated homes fail closed until DeepSeek has an API key", () => {
  const deepSeek = inspectCodexConnection({ model: "deepseek-flash", provider: "deepseek" });
  assert.equal(providerConfigurationError(deepSeek, {}), "缺少 DEEPSEEK_API_KEY，DeepSeek 请求将无法认证");
  assert.equal(providerConfigurationError(deepSeek, { DEEPSEEK_API_KEY: "deepseek-test" }), null);
});

test("lists declared connections as unverified until endpoint evidence exists", () => {
  const inspected = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
  const [unverified] = listCodexConnections({ now: TEST_NOW });
  assert.equal(unverified.status, "unverified");
  assert.deepEqual(unverified.verifiedCapabilities, []);
  const evidence = verifiedBaselineEvidence(inspected.connection, {
    protocol: "responses",
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  const [verified] = listCodexConnections({ evidence, now: TEST_NOW });
  assert.equal(verified.status, "verified");
  assert.deepEqual(verified.verifiedCapabilities, [CAPABILITIES.CHILD_DIRECTED_ROUTE, CAPABILITIES.CHILD_MESSAGE_RUNNING, CAPABILITIES.CHILD_SPAWN, CAPABILITIES.CHILD_WAIT, CAPABILITIES.STREAMING, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TEXT, CAPABILITIES.TOOL_LOOP, CAPABILITIES.USAGE]);
});

test("isolates Runtime and author credentials from the Codex child", () => {
  const base = {
    NOVEL_CODEX_AUTHOR_TOKEN: "author-secret",
    NOVEL_CODEX_CANDIDATE_TOKEN: "inherited-candidate-secret",
    NOVEL_CODEX_RUNTIME_URL: "http://inherited.invalid",
    CODEX_API_KEY: "implicit-codex-key",
    OPENAI_API_KEY: "desktop-key",
    PATH: "/usr/bin",
  };
  const codexEnv = createCodexEnvironment(base, {
    candidateToken: "candidate-secret",
    codexHome: "/novel/home",
    runtimeUrl: "http://127.0.0.1:43123",
    sqliteHome: "/novel/sqlite",
    provider: "deepseek",
  });
  assert.equal(codexEnv.NOVEL_CODEX_RUNTIME_URL, "http://127.0.0.1:43123");
  assert.equal(codexEnv.NOVEL_CODEX_CANDIDATE_TOKEN, "candidate-secret");
  assert.equal(codexEnv.NOVEL_CODEX_AUTHOR_TOKEN, undefined);
  assert.equal(codexEnv.CODEX_API_KEY, undefined);
  assert.equal(codexEnv.OPENAI_API_KEY, undefined);

  const runtimeEnv = createRuntimeEnvironment(base);
  assert.equal(runtimeEnv.NOVEL_CODEX_RUNTIME_URL, undefined);
  assert.equal(runtimeEnv.NOVEL_CODEX_CANDIDATE_TOKEN, undefined);
  assert.equal(runtimeEnv.NOVEL_CODEX_AUTHOR_TOKEN, undefined);
  assert.equal(runtimeEnv.CODEX_API_KEY, undefined);
  assert.equal(runtimeEnv.OPENAI_API_KEY, undefined);
  assert.equal(runtimeEnv.PYTHONPATH, undefined);
  assert.equal(runtimeEnv.PATH, "/usr/bin");
});

test("MCP sidecar lists only candidate tools and forwards bearer-authenticated calls", async (t) => {
  const requests = [];
  const server = createServer((request, response) => {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      const rawBody = Buffer.concat(chunks).toString("utf8");
      requests.push({
        authorization: request.headers.authorization,
        capability: request.headers["x-novel-task-capability"],
        body: rawBody ? JSON.parse(rawBody) : null,
        method: request.method,
        url: request.url,
      });
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ artifactId: "outline-1", revisionId: "outline-rev-2" }));
    });
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  t.after(() => new Promise((resolve) => server.close(resolve)));

  const address = server.address();
  const suffix = process.platform === "win32" ? ".exe" : "";
  const sidecarPath = realpathSync(join(dirname(fileURLToPath(import.meta.url)), `../../native/target/debug/novel-workspace-mcp${suffix}`));
  const child = spawn(sidecarPath, [], {
    env: {
      ...process.env,
      NOVEL_CODEX_CANDIDATE_TOKEN: "candidate-secret",
      NOVEL_CODEX_TASK_CAPABILITY: "task-capability-secret-1234567890",
      NOVEL_CODEX_OPERATION: "generateOutline",
      NOVEL_CODEX_RUNTIME_URL: `http://127.0.0.1:${address.port}`,
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
  t.after(() => child.kill());
  const lines = createInterface({ input: child.stdout });
  const responses = [];
  lines.on("line", (line) => responses.push(JSON.parse(line)));
  const send = async (message) => {
    child.stdin.write(`${JSON.stringify(message)}\n`);
    const deadline = Date.now() + 5_000;
    while (!responses.some((response) => response.id === message.id)) {
      if (Date.now() > deadline) throw new Error("timed out waiting for MCP response");
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    return responses.find((response) => response.id === message.id);
  };

  const initialized = await send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "test", version: "1" } } });
  assert.equal(initialized.result.serverInfo.name, "novel-codex-workspace");
  child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} })}\n`);
  const listed = await send({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} });
  assert.deepEqual(listed.result.tools.map((tool) => tool.name), [
    "read_context",
    "save_outline",
    "complete_task",
  ]);
  assert.ok(listed.result.tools.every((tool) => ["taskId", "projectId", "threadId", "expectedRevision"].every((key) => tool.inputSchema.required.includes(key))));

  const called = await send({
    jsonrpc: "2.0",
    id: 3,
    method: "tools/call",
    params: {
      name: "save_outline",
      _meta: { threadId: "thread-1", callerThreadId: "thread-1", rootThreadId: "thread-1", sessionId: "thread-1", turnId: "turn-1", toolCallId: "call-save", sessionSource: "mcp" },
      arguments: { taskId: "task 1", projectId: "project/1", threadId: "thread-1", expectedRevision: 4, expectedCandidateRevisionId: null, idempotencyKey: "outline-task-1", payload: { nodes: [{ clientKey: "book", kind: "book", position: 0, title: "白塔未亮", purpose: "追查被改写的记录", summary: "林夕留下证词，港城记住名字", dependencyKeys: [], certainty: "tentative", protection: "free" }] } },
    },
  });
  assert.equal(called.result.isError, undefined);
  assert.deepEqual(JSON.parse(called.result.content.map((item) => item.text).join("")), { artifactId: "outline-1", revisionId: "outline-rev-2" });
  await send({
    jsonrpc: "2.0",
    id: 4,
    method: "tools/call",
    params: {
      name: "complete_task",
      _meta: { threadId: "thread-1", callerThreadId: "thread-1", rootThreadId: "thread-1", sessionId: "thread-1", turnId: "turn-1", toolCallId: "call-complete", sessionSource: "mcp" },
      arguments: { taskId: "task 1", projectId: "project/1", threadId: "thread-1", expectedRevision: 4, summary: "候选已保存" },
    },
  });
  assert.deepEqual(requests, [
    {
      authorization: "Bearer candidate-secret",
      capability: "task-capability-secret-1234567890",
      body: { taskId: "task 1", threadId: "thread-1", expectedRevision: 4, expectedCandidateRevisionId: null, idempotencyKey: "outline-task-1", payload: { nodes: [{ clientKey: "book", kind: "book", position: 0, title: "白塔未亮", purpose: "追查被改写的记录", summary: "林夕留下证词，港城记住名字", dependencyKeys: [], certainty: "tentative", protection: "free" }] } },
      method: "POST",
      url: "/api/projects/project%2F1/artifacts/outline",
    },
    {
      authorization: "Bearer candidate-secret",
      capability: "task-capability-secret-1234567890",
      body: { threadId: "thread-1", expectedRevision: 4, summary: "候选已保存" },
      method: "POST",
      url: "/api/projects/project%2F1/tasks/task%201/complete",
    },
  ]);
});

test("loads only the Novel Codex skill bundle and composes a bounded prompt", () => {
  const projectRoot = realpathSync(join(dirname(fileURLToPath(import.meta.url)), "../.."));
  const skills = loadNovelCodexSkills(projectRoot);
  assert.match(skills, /novel-canon/);
  assert.match(skills, /narrative-ir/);
  assert.match(skills, /story-tests/);
  assert.match(skills, /novel-workflow/);
  const prompt = composeNovelCodexPrompt(skills, "检查这一章的 Scene Anchor").map((item) => item.text).join("");
  assert.match(prompt, /Novel Codex 专用工作约束/);
  assert.match(prompt, /用户当前指令：\n检查这一章的 Scene Anchor/);
  assert.throws(() => composeNovelCodexPrompt(skills, "x".repeat(60_000)), /超过安全上限/);
  assert.throws(() => composeNovelCodexPrompt("", "🌙 ".repeat(5_000)), /超过安全上限/);
  assert.throws(() => composeNovelCodexPrompt("🌙 ".repeat(5_000), "小输入"), /超过安全上限/);
});

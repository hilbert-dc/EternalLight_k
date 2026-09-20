import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import {
  CAPABILITIES,
  CAPABILITY_PROFILES,
  DELEGATION_EXECUTION_CAPABILITIES,
  CapabilityEvidenceStore,
  CapabilityReportStore,
  ENGINE_CONTRACT,
  TrustedEvidenceRepository,
  compileCodexConfig,
  compileCodexModelCatalog,
  connectionDefinitionDigest,
  contractHash,
  defineCapabilityReport,
  defineConnection,
  defineEvidence,
  evidenceAdoptionStatus,
  evaluateCapabilityProof,
  defineModelCertification,
  digest,
  getCodec,
  materializeCertificationEvidence,
  isTrustedEvidenceSnapshot,
  resolveCapabilityProfile,
  resolveProtocolSelections,
  resolveSessionPlan,
  scopeCovers,
  selectQualifiedEvidence,
  summarizeEvidence,
} from "../src/index.mjs";
import { trustedTestEvidence, verifiedBaselineEvidence } from "../src/testing/index.mjs";

function connection(overrides = {}) {
  return defineConnection({
    id: "model-a",
    provider: "Provider A",
    endpoint: "https://models.example.test/api/",
    model: "model-a-2026",
    credentialRef: "MODEL_A_API_KEY",
    protocol: "chat-completions",
    protocolOptions: {},
    capabilitySupport: {},
    declaredLimits: { contextTokens: 128_000, outputTokens: 8_000 },
    ...overrides,
  });
}

function planFor(target = connection(), policy = {}) {
  const evidence = verifiedBaselineEvidence(target, { protocol: target.protocol, now: new Date("2026-09-19T00:00:00Z") });
  return resolveSessionPlan({
    connections: [target],
    evidence,
    now: new Date("2026-09-19T01:00:00Z"),
    policy: { defaultConnectionId: target.id, requiredCapabilities: DELEGATION_EXECUTION_CAPABILITIES, productExtensions: DELEGATION_EXECUTION_CAPABILITIES, allowedMcpServerIds: ["workspace"], ...policy },
  });
}

test("normalizes a provider connection without storing a credential", () => {
  const value = connection();
  assert.deepEqual(value, {
    id: "model-a",
    provider: "Provider A",
    endpoint: "https://models.example.test/api",
    model: "model-a-2026",
    credentialRef: "MODEL_A_API_KEY",
    protocol: "chat-completions",
    protocolOptions: {},
    capabilitySupport: {},
    declaredLimits: { contextTokens: 128_000, outputTokens: 8_000 },
  });
  assert.throws(() => defineConnection({ ...value, endpoint: "file:///tmp/key" }), /HTTP/);
  assert.throws(() => defineConnection({ ...value, credentialRef: "secret-value" }), /environment variable/);
  assert.throws(() => defineConnection({ ...value, capabilitySupport: { text: "invented" } }), /mode is invalid/);
});

test("connection metadata cannot declare engine execution capabilities", () => {
  assert.throws(() => connection({ capabilitySupport: { [CAPABILITIES.CHILD_SPAWN]: "unsupported" } }), /model capability/);
});

test("an unrelated unqualified connection does not block the selected route graph", () => {
  const selected = connection();
  const unrelated = connection({ id: "unrelated", model: "unrelated-model", credentialRef: "UNRELATED_API_KEY" });
  const evidence = verifiedBaselineEvidence(selected, { protocol: selected.protocol });
  const plan = resolveSessionPlan({
    connections: [selected, unrelated],
    evidence,
    policy: { defaultConnectionId: selected.id, allowedChildConnectionIds: [selected.id] },
  });
  assert.deepEqual(plan.routes.map((route) => route.connectionId), [selected.id]);
});

test("evidence uses typed assertions, typed dependencies, and separate adoption state", () => {
  const [record] = verifiedBaselineEvidence(connection(), {
    capabilities: [CAPABILITIES.TEXT],
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  assert.equal(record.measurementStatus, "passed");
  assert.equal(evidenceAdoptionStatus(record, { now: new Date("2026-09-19T01:00:00Z") }), "qualified");
  assert.equal(evidenceAdoptionStatus(record, { now: new Date("2026-09-21T00:00:00Z") }), "expired");
  assert.equal(evidenceAdoptionStatus(record, { now: new Date("2026-09-19T01:00:00Z"), expectedDependencies: { modelProtocol: "0".repeat(64) } }), "dependency-mismatch");
  assert.throws(() => defineEvidence({ ...record, assertions: ["free-text"] }), /assertion must be an object/);
  assert.throws(() => defineEvidence({ ...record, assertions: [{ id: "passed", result: "passed" }, { id: "failed", result: "failed" }] }), /every assertion to pass/);
  assert.throws(() => defineEvidence({ ...record, dependencies: { arbitrary: { digest: "a".repeat(64) } } }), /unknown evidence dependency kind/);
  assert.throws(() => defineEvidence({ ...record, scope: { ...record.scope, invented: true } }), /unknown fields/);
});

test("proof contracts require suite-specific assertions and classified dependencies", () => {
  const target = connection();
  const [record] = verifiedBaselineEvidence(target, { capabilities: [CAPABILITIES.TEXT] });
  assert.equal(evaluateCapabilityProof(record, {
    expectedDependencies: { modelProtocol: record.dependencies.modelProtocol.digest, access: record.dependencies.access.digest },
  }).status, "qualified");

  const missingAssertion = defineEvidence({
    ...record,
    assertions: [{ id: "unrelated-passed-check", result: "passed" }],
  });
  assert.deepEqual(evaluateCapabilityProof(missingAssertion), {
    status: "incomplete-assertions",
    missingAssertions: [`fixture:${CAPABILITIES.TEXT}`],
    mismatchedDependencies: [],
  });

  const mismatchedDependency = evaluateCapabilityProof(record, {
    expectedDependencies: { modelProtocol: "0".repeat(64), access: record.dependencies.access.digest },
  });
  assert.equal(mismatchedDependency.status, "dependency-mismatch");
  assert.deepEqual(mismatchedDependency.mismatchedDependencies, ["modelProtocol"]);
});

test("resolver consumes an immutable trusted evidence snapshot", () => {
  const target = connection();
  const records = verifiedBaselineEvidence(target);
  const snapshot = trustedTestEvidence(records, { now: new Date("2026-09-19T01:00:00Z") });
  assert.equal(isTrustedEvidenceSnapshot(snapshot), true);
  assert.equal(Object.isFrozen(snapshot), true);
  assert.throws(() => selectQualifiedEvidence({ ...snapshot }, { environmentDigest: records[0].environmentDigest }), /trusted snapshot/);
  const plan = resolveSessionPlan({
    connections: [target],
    evidenceSnapshot: snapshot,
    now: new Date("2026-09-19T01:00:00Z"),
    policy: {},
  });
  assert.equal(plan.defaultModel, target.id);
});

test("production resolver rejects copied or fabricated records without provenance", () => {
  const target = connection();
  const records = verifiedBaselineEvidence(target);
  assert.throws(() => resolveSessionPlan({ connections: [target], evidence: structuredClone(records), policy: {} }), /validated provenance/);
  assert.throws(() => { records[0].dependencies.access.digest = "0".repeat(64); }, TypeError);
});

test("trusted evidence repository loads records only from their content-addressed report", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-reports-"));
  try {
    const target = connection();
    const records = verifiedBaselineEvidence(target, { capabilities: [CAPABILITIES.TEXT] });
    const report = defineCapabilityReport({
      connection: target,
      result: { records },
      startedAt: "2026-09-19T00:00:00Z",
      completedAt: "2026-09-19T00:01:00Z",
    });
    const reportStore = new CapabilityReportStore(root);
    await reportStore.write(report);
    const bound = records.map((record) => defineEvidence({ ...record, reportRef: report.reportId, reportDigest: report.reportId }));
    const repository = await TrustedEvidenceRepository.load({ records: bound, reportStore });
    assert.equal(isTrustedEvidenceSnapshot(repository.resolveSnapshot()), true);
    await assert.rejects(TrustedEvidenceRepository.load({
      records: bound.map((record) => ({ ...record, evidenceRef: `${record.evidenceRef}:tampered` })),
      reportStore,
    }), /not bound/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("resolver rejects evidence when a required assertion or dependency is invalid", () => {
  const target = connection();
  const records = verifiedBaselineEvidence(target);
  const missingAssertion = records.map((record) => record.capability === CAPABILITIES.TEXT
    ? defineEvidence({ ...record, assertions: [{ id: "unrelated-passed-check", result: "passed" }] })
    : record);
  assert.throws(() => resolveSessionPlan({ connections: [target], evidence: trustedTestEvidence(missingAssertion), policy: {} }), /lacks verified capabilities: text/);

  const dependencyMismatch = records.map((record) => record.capability === CAPABILITIES.TEXT
    ? defineEvidence({ ...record, dependencies: { ...record.dependencies, modelProtocol: { digest: "0".repeat(64) } } })
    : record);
  assert.throws(() => resolveSessionPlan({ connections: [target], evidence: trustedTestEvidence(dependencyMismatch), policy: {} }), /lacks verified capabilities: text/);
});

test("behavior dimensions are preserved and enforced by scope coverage", () => {
  const [record] = verifiedBaselineEvidence(connection(), { capabilities: [CAPABILITIES.TEXT] });
  const scoped = defineEvidence({ ...record, scope: { ...record.scope, behaviors: ["cancel", "tool-loop"] } });
  assert.deepEqual(scoped.scope.behaviors, ["cancel", "tool-loop"]);
  assert.equal(scopeCovers(scoped.scope, { protocol: scoped.scope.protocol, behaviors: ["cancel"] }), true);
  assert.equal(scopeCovers(scoped.scope, { protocol: scoped.scope.protocol, behaviors: ["recovery"] }), false);
});

test("published model certification materializes into credential-scoped session evidence", () => {
  const target = connection({
    protocol: "responses",
    protocolOptions: { toolChoiceMode: "auto-only", reasoningState: "native" },
  });
  const capabilities = [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP];
  const suiteResults = [
    { kind: "protocol", suiteId: "wire-v1", suiteVersion: 1, status: "passed", metrics: { capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS] } },
    { kind: "core-integration", suiteId: "codex-engine-core-integration-v1", suiteVersion: 1, status: "passed" },
  ];
  const report = { suite: "model-onboarding-v1", outcome: "passed", connectionId: target.id, protocol: target.protocol, codecVersion: getCodec(target.protocol).version, profileId: "base-tools-v1", qualificationContextDigest: digest({ release: "v1" }), capabilities, suiteResults };
  const certification = defineModelCertification({
    connectionId: target.id,
    connectionDigest: connectionDefinitionDigest(target),
    protocol: target.protocol,
    codecVersion: getCodec(target.protocol).version,
    engineContractHash: contractHash(),
    qualificationContextDigest: digest({ release: "v1" }),
    profileId: "base-tools-v1",
    capabilities,
    routes: [{ parentConnectionId: target.id, childConnectionId: target.id }],
    checkedAt: "2026-09-19T00:00:00.000Z",
    expiresAt: "2026-10-19T00:00:00.000Z",
    report,
    reportDigest: digest(report),
    suiteResults,
    outcome: "passed",
  });
  const qualificationContextDigest = certification.qualificationContextDigest;
  const evidence = materializeCertificationEvidence(certification, {
    connection: target,
    credentialScope: "credential-v1:first-key",
    now: new Date("2026-09-20T00:00:00.000Z"),
    qualificationContextDigest,
  });
  const plan = resolveSessionPlan({
    connections: [target],
    evidence,
    now: new Date("2026-09-20T00:00:00.000Z"),
    policy: { credentialScopes: { [target.id]: "credential-v1:first-key" }, qualificationContextDigest },
  });
  assert.equal(plan.routes[0].protocol, "responses");
  assert.deepEqual(plan.allowedChildModels, []);

  const rotated = materializeCertificationEvidence(certification, {
    connection: target,
    credentialScope: "credential-v1:rotated-key",
    now: new Date("2026-09-20T00:00:00.000Z"),
    qualificationContextDigest,
  });
  assert.notEqual(rotated[0].environmentDigest, evidence[0].environmentDigest);
  assert.throws(() => materializeCertificationEvidence(certification, {
    connection: target,
    now: new Date("2026-09-20T00:00:00.000Z"),
    qualificationContextDigest: digest({ release: "v2" }),
  }), /qualification context/);
  assert.throws(() => defineModelCertification({ ...certification, report: { ...certification.report, outcome: "failed" } }), /report digest/);
  const failedReport = { ...certification.report, outcome: "failed" };
  assert.throws(() => defineModelCertification({ ...certification, report: failedReport, reportDigest: digest(failedReport) }), /report did not pass/);
  assert.throws(() => defineModelCertification({ ...certification, capabilities: [...certification.capabilities, CAPABILITIES.CHILD_CANCEL_TREE] }), /lifecycle/);
  assert.throws(() => defineModelCertification({ ...certification, profileId: "directed-delegation-v1", capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS] }), /does not satisfy profile/);
});

test("resolves an immutable bridge session only from active verified evidence", () => {
  const target = connection();
  const plan = planFor(target);
  assert.equal(plan.routes[0].transport, "protocol-conversion");
  assert.equal(plan.routes[0].codecId, "chat-completions");
  assert.equal(plan.routes[0].inputBudget, 103_616);
  assert.deepEqual(plan.allowedChildModels, ["model-a"]);
  assert.equal(Object.isFrozen(plan), true);
  assert.equal(Object.isFrozen(plan.routes), true);
  assert.equal(Object.isFrozen(plan.routes[0].enabledExtensions), true);
  assert.equal(Object.isFrozen(plan.allowedChildModels), true);
  assert.deepEqual(plan.authorizedCapabilityProfiles, ["base-tools-v1", "directed-delegation-v1", "interactive-v1", "text-v1"]);
  assert.match(plan.id, /^[a-f0-9]{64}$/);

  const failed = verifiedBaselineEvidence(target, { protocol: target.protocol, now: new Date("2026-09-19T00:00:00Z") });
  failed.push({ ...failed[1], status: "failed", checkedAt: "2026-09-19T00:30:00Z", evidenceRef: "failure:tool-call" });
  assert.throws(() => resolveSessionPlan({ connections: [target], evidence: trustedTestEvidence(failed), now: new Date("2026-09-19T01:00:00Z"), policy: { requiredCapabilities: [CAPABILITIES.STRUCTURED_TOOL_CALLS] } }), /lacks verified capabilities/);
});

test("child route authorization requires the exact directed evidence edge", () => {
  const parent = connection();
  const child = connection({ id: "model-b", model: "model-b-2026", credentialRef: "MODEL_B_API_KEY" });
  const now = new Date("2026-09-19T01:00:00Z");
  const policy = {
    defaultConnectionId: parent.id,
    allowedChildConnectionIds: [child.id],
    allowedChildEdges: [{ parentConnectionId: parent.id, childConnectionId: child.id }],
    requiredCapabilities: DELEGATION_EXECUTION_CAPABILITIES,
  };
  const selfOnly = [
    ...verifiedBaselineEvidence(parent, { now: new Date("2026-09-19T00:00:00Z"), routeGrants: [{ parentConnectionId: parent.id, childConnectionId: parent.id }] }),
    ...verifiedBaselineEvidence(child, { now: new Date("2026-09-19T00:00:00Z") }),
  ];
  const denied = resolveSessionPlan({ connections: [parent, child], evidence: selfOnly, now, policy });
  assert.deepEqual(denied.allowedChildEdges, []);
  assert.deepEqual(denied.allowedChildModels, []);

  const exact = [
    ...verifiedBaselineEvidence(parent, { now: new Date("2026-09-19T00:00:00Z"), routeGrants: [{ parentConnectionId: parent.id, childConnectionId: child.id }] }),
    ...verifiedBaselineEvidence(child, { now: new Date("2026-09-19T00:00:00Z") }),
  ];
  const allowed = resolveSessionPlan({ connections: [parent, child], evidence: exact, now, policy });
  assert.deepEqual(allowed.allowedChildEdges, [{ parentConnectionId: parent.id, childConnectionId: child.id }]);
  assert.deepEqual(allowed.allowedChildModels, [child.id]);
  const config = compileCodexConfig({
    sessionPlan: allowed,
    modelCatalogPath: "/isolated/model-catalog.json",
    bridge: { baseUrl: "http://127.0.0.1:45678", credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
  });
  assert.match(config, /expose_spawn_agent_model_overrides = true/);
  assert.match(config, /default_subagent_model = "model-b"/);
});

test("fails closed when auto protocol has no verified selection", () => {
  const target = connection({ protocol: "auto" });
  assert.throws(() => resolveSessionPlan({ connections: [target], evidence: [], policy: {} }), /no verified protocol selection/);
});

test("qualification context changes invalidate otherwise identical evidence", () => {
  const target = connection();
  const evidence = verifiedBaselineEvidence(target, { qualificationContextDigest: "instructions-and-build-v1" });
  assert.doesNotThrow(() => resolveSessionPlan({ connections: [target], evidence, policy: { qualificationContextDigest: "instructions-and-build-v1" } }));
  assert.throws(
    () => resolveSessionPlan({ connections: [target], evidence, policy: { qualificationContextDigest: "instructions-and-build-v2" } }),
    /lacks verified capabilities/,
  );
});

test("auto protocol selection comes from active evidence and honors failures", () => {
  const target = connection({ protocol: "auto" });
  const verified = verifiedBaselineEvidence(target, {
    protocol: "messages",
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  assert.deepEqual(resolveProtocolSelections([target], verified, { now: new Date("2026-09-19T01:00:00Z") }), { "model-a": "messages" });
  const failed = [...verified, { ...verified[1], status: "failed", checkedAt: "2026-09-19T00:30:00Z", evidenceRef: "failure:messages" }];
  assert.deepEqual(resolveProtocolSelections([target], trustedTestEvidence(failed), { now: new Date("2026-09-19T01:00:00Z") }), { "model-a": "messages" });
  assert.deepEqual(resolveProtocolSelections([target], trustedTestEvidence(failed), { now: new Date("2026-09-19T01:00:00Z"), requiredCapabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS] }), {});
  assert.deepEqual(resolveProtocolSelections([target], verified, { now: new Date("2026-09-21T00:00:00Z") }), {});
});

test("persists evidence atomically without secrets", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-evidence-"));
  try {
    const target = connection();
    const records = verifiedBaselineEvidence(target, { protocol: target.protocol });
    const store = new CapabilityEvidenceStore(join(root, "evidence.json"));
    const digest = await store.replace(records);
    assert.match(digest, /^[a-f0-9]{64}$/);
    assert.deepEqual(await store.read(), records);
    const raw = await readFile(join(root, "evidence.json"), "utf8");
    assert.doesNotMatch(raw, /api[_-]?key/i);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("serializes concurrent evidence appends without losing records", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-evidence-concurrency-"));
  try {
    const target = connection();
    const records = verifiedBaselineEvidence(target, { protocol: target.protocol });
    const store = new CapabilityEvidenceStore(join(root, "evidence.json"));
    await Promise.all(records.map((record) => store.append([record])));
    assert.deepEqual((await store.read()).map((record) => record.evidenceRef).sort(), records.map((record) => record.evidenceRef).sort());
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("auto protocol selection considers the full requested capability set", () => {
  const target = connection({ protocol: "auto" });
  const messages = verifiedBaselineEvidence(target, { protocol: "messages", capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS] });
  const chat = verifiedBaselineEvidence(target, { protocol: "chat-completions", capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, ...DELEGATION_EXECUTION_CAPABILITIES] });
  assert.deepEqual(resolveProtocolSelections([target], [...messages, ...chat], { requiredCapabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, ...DELEGATION_EXECUTION_CAPABILITIES] }), { "model-a": "chat-completions" });
});

test("evidence aggregation is order-independent and failures require explicit supersession", () => {
  const target = connection();
  const [verified] = verifiedBaselineEvidence(target, {
    capabilities: [CAPABILITIES.TEXT],
    protocol: target.protocol,
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  const failed = { ...verified, status: "failed", evidenceRef: "failure:text", checkedAt: "2026-09-19T00:10:00Z" };
  const laterSuccess = { ...verified, evidenceRef: "success:text", checkedAt: "2026-09-19T00:20:00Z" };
  for (const records of [[verified, failed, laterSuccess], [laterSuccess, verified, failed]]) {
    assert.equal(summarizeEvidence(records).get(CAPABILITIES.TEXT).evidenceRef, "failure:text");
  }
  const supersedingSuccess = { ...laterSuccess, evidenceRef: "superseding:text", supersedes: ["failure:text"] };
  assert.equal(summarizeEvidence([failed, supersedingSuccess]).get(CAPABILITIES.TEXT).evidenceRef, "superseding:text");
  const weakerSupersession = { ...laterSuccess, source: "probe", evidenceRef: "weak:text", supersedes: ["failure:text"] };
  assert.equal(summarizeEvidence([failed, weakerSupersession]).get(CAPABILITIES.TEXT).evidenceRef, "failure:text");
  const crossScopeSupersession = { ...laterSuccess, scope: { ...laterSuccess.scope, mode: "native-sse" }, evidenceRef: "cross-scope:text", supersedes: ["failure:text"] };
  assert.equal(summarizeEvidence([failed, crossScopeSupersession]).get(CAPABILITIES.TEXT).evidenceRef, "failure:text");
});

test("superseding authentication evidence restores the credential realm", () => {
  const target = connection();
  const records = verifiedBaselineEvidence(target, { checkedAt: "2026-09-19T00:00:00Z", expiresAt: "2026-09-20T00:00:00Z" });
  const failure = {
    ...records[0],
    status: "failed",
    evidenceRef: "auth-failure",
    checkedAt: "2026-09-19T00:10:00Z",
    sample: { category: "authentication" },
  };
  assert.throws(() => resolveSessionPlan({ connections: [target], evidence: trustedTestEvidence([...records, failure]), now: new Date("2026-09-19T00:15:00Z"), policy: {} }), /credential realm is revoked/);
  const restored = {
    ...records[0],
    evidenceRef: "auth-restored",
    checkedAt: "2026-09-19T00:20:00Z",
    supersedes: [failure.evidenceRef],
  };
  assert.doesNotThrow(() => resolveSessionPlan({ connections: [target], evidence: trustedTestEvidence([...records, failure, restored]), now: new Date("2026-09-19T00:25:00Z"), policy: {} }));
});

test("session expiry includes optional grants and directed child evidence", () => {
  const target = connection();
  const records = verifiedBaselineEvidence(target, { checkedAt: "2026-09-19T00:00:00Z", expiresAt: "2026-09-20T00:00:00Z" });
  const expiring = records.map((record) => record.capability === CAPABILITIES.CHILD_DIRECTED_ROUTE
    ? { ...record, expiresAt: "2026-09-19T00:05:00Z" }
    : record);
  const plan = resolveSessionPlan({ connections: [target], evidence: trustedTestEvidence(expiring), now: new Date("2026-09-19T00:01:00Z"), policy: { requiredCapabilities: [] } });
  assert.equal(plan.expiresAt, "2026-09-19T00:05:00.000Z");
});

test("capability reports derive their aggregate state from every result", () => {
  const target = connection();
  const records = verifiedBaselineEvidence(target, { capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS] });
  const report = defineCapabilityReport({
    connection: target,
    result: { attempts: [], records: [records[0], { ...records[1], status: "failed" }], selectedProtocol: target.protocol },
    startedAt: "2026-09-19T00:00:00Z",
    completedAt: "2026-09-19T00:01:00Z",
  });
  assert.equal(report.status, "failed");
  assert.deepEqual(report.records.map((record) => record.status), ["verified", "failed"]);
  const fallback = defineCapabilityReport({
    connection: target,
    result: {
      attempts: [{ protocol: "responses", status: "failed" }, { protocol: "chat-completions", status: "verified" }],
      records: [
        { ...records[0], status: "failed", scope: { ...records[0].scope, protocol: "responses" } },
        records[0],
        records[1],
      ],
      selectedProtocol: target.protocol,
    },
    startedAt: "2026-09-19T00:00:00Z",
    completedAt: "2026-09-19T00:01:00Z",
  });
  assert.equal(fallback.status, "verified");

  const diagnostic = defineCapabilityReport({
    connection: target,
    error: Object.assign(new Error("failed with private-token"), {
      attempts: [{
        protocol: "responses",
        suite: "codex-autonomous-delegation-v1",
        status: "failed",
        diagnostics: { category: "transport-result-missing", trace: { events: [{ type: "bridge.response.completed", details: { value: "private-token" } }] } },
      }],
    }),
    redactions: ["private-token"],
    startedAt: "2026-09-19T00:00:00Z",
    completedAt: "2026-09-19T00:01:00Z",
  });
  assert.equal(diagnostic.attempts[0].suite, "codex-autonomous-delegation-v1");
  assert.equal(diagnostic.attempts[0].diagnostics.category, "transport-result-missing");
  assert.doesNotMatch(JSON.stringify(diagnostic), /private-token/);
});

test("compiles Codex config and model catalog from the frozen plan", () => {
  const plan = planFor();
  assert.throws(() => compileCodexModelCatalog(plan), /base instructions/);
  const catalog = compileCodexModelCatalog(plan, { baseInstructions: "Upstream Codex test instructions." });
  assert.deepEqual(catalog.models.map((model) => ({ slug: model.slug, context: model.context_window, multiAgent: model.multi_agent_version, modalities: model.input_modalities })), [{
    slug: "model-a",
    context: 111_616,
    multiAgent: "v2",
    modalities: ["text"],
  }]);
  assert.equal(catalog.models[0].base_instructions, "Upstream Codex test instructions.");
  assert.equal(catalog.models[0].model_messages, null);
  const restrictedPlan = planFor(connection(), { productExtensions: [] });
  const restrictedCatalog = compileCodexModelCatalog(restrictedPlan, { baseInstructions: "Upstream Codex test instructions." });
  assert.equal(restrictedCatalog.models[0].multi_agent_version, null);
  assert.equal(restrictedCatalog.models[0].supports_parallel_tool_calls, false);
  const config = compileCodexConfig({
    sessionPlan: plan,
    modelCatalogPath: "/isolated/model-catalog.json",
    bridge: { baseUrl: "http://127.0.0.1:45678", credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "workspace", command: "python3", args: ["/runtime/mcp.py"], envVars: ["TASK_CAPABILITY"] }],
  });
  assert.match(config, /model_provider = "agent_engine"/);
  assert.match(config, /base_url = "http:\/\/127\.0\.0\.1:45678"/);
  assert.match(config, /env_key = "AGENT_ENGINE_BRIDGE_TOKEN"/);
  assert.match(config, /expose_spawn_agent_model_overrides = false/);
  assert.match(config, /\[mcp_servers\.workspace\]/);
  assert.doesNotMatch(config, /Provider A.*API_KEY/);
  assert.throws(() => compileCodexConfig({
    sessionPlan: plan,
    modelCatalogPath: "/isolated/model-catalog.json",
  }), /controlled bridge/);
  assert.throws(() => compileCodexConfig({
    sessionPlan: plan,
    modelCatalogPath: "/isolated/model-catalog.json",
    bridge: { baseUrl: "http://127.0.0.1:45678", credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "outside-plan", command: "unexpected" }],
  }), /exactly match/);
  assert.throws(() => compileCodexConfig({
    sessionPlan: plan,
    modelCatalogPath: "/isolated/model-catalog.json",
    bridge: { baseUrl: "http://127.0.0.1:45678", credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "workspace", command: "python3" }],
    maxConcurrentChildren: 999,
  }), /cannot override/);
  assert.throws(() => planFor(connection(), { productExtensions: [CAPABILITIES.TOOL_SEARCH], requiredExtensions: [CAPABILITIES.TOOL_SEARCH] }), /not verified/);
});

test("engine contract is explicit and stable", () => {
  assert.equal(ENGINE_CONTRACT.canonicalModelProtocol, "responses");
  assert.deepEqual(ENGINE_CONTRACT.requiredCapabilities, [CAPABILITIES.TEXT]);
  assert.equal(resolveCapabilityProfile("autonomous-delegation-v1"), CAPABILITY_PROFILES["autonomous-delegation-v1"]);
  assert.throws(() => resolveCapabilityProfile("invented"), /unknown capability profile/);
});

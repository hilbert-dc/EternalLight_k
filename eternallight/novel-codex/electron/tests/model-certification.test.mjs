import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { test } from "node:test";

import { CAPABILITIES, DELEGATION_EXECUTION_CAPABILITIES, defineQualificationSuiteResult } from "@eternallight/agent-engine";
import { certifyModel } from "../../scripts/certify-model.mjs";
import { evaluateDelegationCapability } from "@eternallight/agent-engine/qualification/delegation-proof";
import { certificationPath } from "../../scripts/certification-publication.mjs";

test("qualification publishes deterministic protocol, core, and delegation execution evidence", async () => {
  const root = mkdtempSync(join(tmpdir(), "novel-model-certification-"));
  try {
    const catalogPath = join(root, "model-catalog.json");
    const certificationsPath = join(root, "model-certifications.json");
    const policyPath = join(root, "supported-cli.json");
    const reportDirectory = join(root, "certification-reports");
    writeFileSync(catalogPath, JSON.stringify({
      version: 1,
      defaultConnectionId: "fixture-model",
      connections: [{
        id: "fixture-model",
        provider: "Fixture",
        endpoint: "https://fixture.invalid",
        model: "fixture-v1",
        credentialRef: "FIXTURE_API_KEY",
        protocol: "responses",
        protocolOptions: { namespaceToolsMode: "flatten" },
        capabilitySupport: {
          text: "native",
          structuredToolCalls: "lossless-adapted",
          streaming: "native",
          usage: "native",
        },
        declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 },
        declaredCapabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, ...DELEGATION_EXECUTION_CAPABILITIES],
        operations: ["advisory"],
      }],
    }));
    writeFileSync(certificationsPath, JSON.stringify({ version: 2, certifications: [] }));
    writeFileSync(policyPath, JSON.stringify({ version: 1, modelCertificationsFile: "model-certifications.json", modelCatalogSha256: "", modelCertificationsSha256: "" }));
    const records = [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.STREAMING, CAPABILITIES.USAGE]
      .map((capability) => ({ capability, status: "verified" }));
    let executionCalls = 0;
    const result = await certifyModel({
      environment: { FIXTURE_API_KEY: "private-test-key" },
      now: new Date("2026-09-20T00:00:00.000Z"),
      profileId: "directed-delegation-v1",
      cliPath: process.execPath,
      baseInstructions: "Fixed upstream test instructions.",
      paths: { catalogPath, certificationsPath, policyPath, reportDirectory },
      verifier: { verify: async () => ({ selectedProtocol: "responses", attempts: [{ protocol: "responses", status: "verified" }], records }) },
      coreRunner: { verify: async () => defineQualificationSuiteResult({ kind: "core-integration", suiteId: "core-fixture-v1", suiteVersion: 1, status: "passed" }) },
      conformanceRunner: { verify: async () => {
        executionCalls += 1;
        return {
          attempt: { protocol: "responses", suite: "codex-engine-delegation-execution-v1", status: "verified", metrics: { parentSynthesizedReceipt: false, capabilityResults: DELEGATION_EXECUTION_CAPABILITIES.map((capability) => ({ capability, ...evaluateDelegationCapability(capability, { completedOperations: ["spawn_agent", "send_message", "wait_agent"], childIdentityObserved: true, childGateObserved: true, parentFollowupTokenQueued: true, childFollowupVisible: true, childResultObserved: true, childTerminalResultObserved: true, directedRouteObserved: true, terminated: true }) })) } },
          records: DELEGATION_EXECUTION_CAPABILITIES.map((capability) => ({ capability, status: "verified" })),
        };
      } },
    });
    assert.equal(executionCalls, 1);
    assert.equal(result.profileId, "directed-delegation-v1");
    assert.equal(result.published, true);
    assert.match(result.reportPath, /certification-reports/);
    const document = JSON.parse(readFileSync(certificationPath(root, JSON.parse(readFileSync(policyPath, "utf8"))), "utf8"));
    assert.equal(document.version, 2);
    assert.ok(document.certifications[0].capabilities.includes(CAPABILITIES.STREAMING));
    assert.ok(document.certifications[0].capabilities.includes(CAPABILITIES.USAGE));
    assert.deepEqual(document.certifications[0].suiteResults.map(({ kind, status }) => ({ kind, status })), [
      { kind: "protocol", status: "passed" },
      { kind: "core-integration", status: "passed" },
      { kind: "execution-conformance", status: "passed" },
    ]);
    assert.equal(document.certifications[0].suiteResults[2].sampleCount, 1);
    assert.doesNotMatch(JSON.stringify(document), /private-test-key/);
    const policy = JSON.parse(readFileSync(policyPath, "utf8"));
    assert.match(policy.modelCatalogSha256, /^[a-f0-9]{64}$/);
    assert.match(policy.modelCertificationsSha256, /^[a-f0-9]{64}$/);

    await assert.rejects(certifyModel({
      environment: { FIXTURE_API_KEY: "private-test-key" },
      now: new Date("2026-09-21T00:00:00.000Z"),
      profileId: "directed-delegation-v1",
      cliPath: process.execPath,
      baseInstructions: "Fixed upstream test instructions.",
      paths: { catalogPath, certificationsPath, policyPath, reportDirectory },
      verifier: { verify: async () => ({ selectedProtocol: "responses", attempts: [{ protocol: "responses", status: "verified" }], records }) },
      coreRunner: { verify: async () => defineQualificationSuiteResult({ kind: "core-integration", suiteId: "core-fixture-v1", suiteVersion: 1, status: "passed" }) },
      conformanceRunner: { verify: async () => ({
        attempt: { protocol: "responses", suite: "codex-engine-delegation-execution-v1", status: "failed", category: "transport-result-missing", diagnostics: { category: "transport-result-missing" } },
        records: DELEGATION_EXECUTION_CAPABILITIES.map((capability) => ({ capability, status: "verified" })),
      }) },
    }), /delegation execution conformance failed/);
    const failureReports = readdirSync(reportDirectory);
    assert.equal(failureReports.length, 2);
    const failureReport = failureReports.map((name) => readFileSync(join(reportDirectory, name), "utf8")).find((content) => content.includes('"status": "failed"'));
    assert.match(failureReport, /transport-result-missing/);
    assert.doesNotMatch(failureReport, /private-test-key/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

import { createHash, randomUUID } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { certificationPath, publishCertificationSnapshot } from "./certification-publication.mjs";
import { fileURLToPath } from "node:url";

import {
  CAPABILITIES,
  DELEGATION_EXECUTION_CAPABILITIES,
  ENGINE_CAPABILITIES,
  CapabilityVerifier,
  connectionDefinitionDigest,
  contractHash,
  defineConnection,
  defineModelCertification,
  defineQualificationSuiteResult,
  digest,
  getCodec,
  resolveCapabilityProfile,
} from "@eternallight/agent-engine";
import { CodexConformanceRunner } from "../electron/codex-conformance-runner.mjs";
import { CodexCoreIntegrationRunner } from "../electron/codex-core-integration-runner.mjs";
import { createQualificationContextDigest, resolveCodexCli } from "../electron/codex-runtime.mjs";
import { loadProjectDevelopmentEnvironment, resolveDevelopmentEnvironment } from "../electron/development-credentials.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const catalogPath = join(root, "codex-runtime", "model-catalog.json");
const policyPath = join(root, "codex-runtime", "supported-cli.json");
const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

function boundedReportValue(value, secret, state = { nodes: 0 }, depth = 0) {
  state.nodes += 1;
  if (state.nodes > 12_000) return "[NODE_LIMIT]";
  if (value === null || ["boolean", "number"].includes(typeof value)) return value;
  if (typeof value === "string") return value.replaceAll(secret, "[REDACTED]").slice(0, 500);
  if (depth >= 12) return "[DEPTH_LIMIT]";
  if (Array.isArray(value)) return value.slice(0, 320).map((item) => boundedReportValue(item, secret, state, depth + 1));
  if (!value || typeof value !== "object") return String(value).slice(0, 500);
  return Object.fromEntries(Object.entries(value).slice(0, 64)
    .map(([key, child]) => [key.slice(0, 120), boundedReportValue(child, secret, state, depth + 1)]));
}

function atomicWriteJson(path, value, mode = 0o600) {
  const temporaryPath = `${path}.${process.pid}.${randomUUID()}.tmp`;
  try {
    writeFileSync(temporaryPath, `${JSON.stringify(value, null, 2)}\n`, { encoding: "utf8", flag: "wx", mode, flush: true });
    renameSync(temporaryPath, path);
  } catch (error) {
    rmSync(temporaryPath, { force: true });
    throw error;
  }
}

function writeDiagnosticReport({ connection, diagnostics, error = null, now, profile, qualificationContextDigest, reportDirectory, secret, status }) {
  mkdirSync(reportDirectory, { recursive: true, mode: 0o700 });
  const completedAt = new Date();
  const report = boundedReportValue({
    version: 2,
    status,
    connectionId: connection.id,
    profileId: profile.id,
    startedAt: now.toISOString(),
    completedAt: completedAt.toISOString(),
    terminationReason: error ? "failed" : "completed",
    qualificationContextDigest,
    error: error ? error instanceof Error ? error.message : String(error) : null,
    ...diagnostics,
  }, secret);
  const reportPath = join(reportDirectory, `${completedAt.toISOString().replaceAll(":", "-")}-${digest(report)}.json`);
  atomicWriteJson(reportPath, report);
  return reportPath;
}

function capabilityFailure(record) {
  const unresolved = (record.assertions ?? [])
    .filter((assertion) => assertion.result !== "passed")
    .map((assertion) => `${assertion.id}=${assertion.result}`);
  return `${record.capability}:${record.status}${unresolved.length ? `:${unresolved.join("|")}` : ""}`;
}

function selectedEntry(catalog, requestedId) {
  const id = requestedId || catalog.defaultConnectionId;
  const entry = catalog.connections.find((candidate) => candidate.id === id);
  if (!entry) throw new Error(`unknown model connection: ${id}`);
  return entry;
}

export async function certifyModel({
  connectionId = null,
  environment = process.env,
  now = new Date(),
  profileId = null,
  verifier = new CapabilityVerifier({ ttlMs: 30 * 24 * 60 * 60 * 1000 }),
  coreRunner = null,
  conformanceRunner = null,
  cliPath = null,
  baseInstructions: suppliedBaseInstructions = null,
  paths = {},
  publish = true,
} = {}) {
  const activeCatalogPath = paths.catalogPath ?? catalogPath;
  const activePolicyPath = paths.policyPath ?? policyPath;
  const catalog = JSON.parse(readFileSync(activeCatalogPath, "utf8"));
  const entry = selectedEntry(catalog, connectionId);
  const connection = defineConnection(entry);
  const profile = resolveCapabilityProfile(profileId ?? "base-tools-v1");
  const engineCapabilities = new Set(Object.values(ENGINE_CAPABILITIES));
  const unsupportedCapabilities = profile.requiredCapabilities.filter((capability) => !engineCapabilities.has(capability) && !["native", "lossless-adapted"].includes(connection.capabilitySupport?.[capability]));
  if (unsupportedCapabilities.length) throw new Error(`connection adapter does not declare profile support: ${unsupportedCapabilities.join(", ")}`);
  if (connection.protocol === "auto") throw new Error("release certification requires an explicit protocol adapter");
  const apiKey = environment[connection.credentialRef];
  if (typeof apiKey !== "string" || !apiKey.trim()) throw new Error(`missing ${connection.credentialRef}`);
  const cli = cliPath ? { path: cliPath, source: "injected" } : resolveCodexCli({
    explicitPath: environment.NOVEL_CODEX_CLI,
    isPackaged: false,
    projectRoot: root,
    resourcesPath: root,
  });
  if (!cli.path) throw new Error(cli.reason ?? "Codex CLI unavailable");
  const baseInstructions = suppliedBaseInstructions ?? readFileSync(resolve(root, "../../codex-rs/models-manager/prompt.md"), "utf8").trim();
  if (typeof baseInstructions !== "string" || !baseInstructions.trim()) throw new Error("base instructions are required");
  const qualificationContextDigest = createQualificationContextDigest({ baseInstructions: baseInstructions.trim(), cliPath: cli.path });
  const workspace = mkdtempSync(join(tmpdir(), "novel-codex-certification-"));
  const diagnostics = { attempts: [], suiteResults: [] };
  try {
    const probe = await verifier.verify(connection, {
      apiKey: apiKey.trim(),
      credentialScope: `release-certification:${connection.id}`,
      now,
      qualificationContextDigest,
    });
    const verified = new Set(probe.records.filter((record) => record.status === "verified").map((record) => record.capability));
    const protocolCapabilities = profile.requiredCapabilities.filter((capability) => !engineCapabilities.has(capability));
    const missingProtocolCapabilities = protocolCapabilities.filter((capability) => !verified.has(capability));
    if (missingProtocolCapabilities.length) throw new Error(`protocol qualification lacks profile capabilities: ${missingProtocolCapabilities.join(", ")}`);
    const protocolSuite = defineQualificationSuiteResult({ kind: "protocol", suiteId: "model-protocol-conformance-v1", suiteVersion: 1, status: "passed", metrics: { capabilities: [...verified].filter((capability) => !engineCapabilities.has(capability)).sort() } });
    diagnostics.attempts.push(...probe.attempts);
    diagnostics.suiteResults.push(protocolSuite);
    coreRunner ??= new CodexCoreIntegrationRunner({
      baseEnvironment: environment,
      baseInstructions: baseInstructions.trim(),
      command: [cli.path, "app-server"],
      cwd: workspace,
      nodeCommand: process.execPath,
    });
    const coreSuite = await coreRunner.verify({
      apiKey: apiKey.trim(),
      connection,
      credentialScope: `release-certification:${connection.id}`,
      protocol: probe.selectedProtocol,
      probeEvidence: probe.records,
      qualificationContextDigest,
      now,
    });
    diagnostics.suiteResults.push(coreSuite);
    if (coreSuite.status !== "passed") throw new Error(`Codex core integration failed: ${coreSuite.blockingFailures.join(", ")}`);
    if (profile.requiredCapabilities.includes(CAPABILITIES.TOOL_LOOP)) verified.add(CAPABILITIES.TOOL_LOOP);
    const suiteResults = [protocolSuite, coreSuite];
    const conformanceAttempts = [];
    if (profile.requiredCapabilities.some((capability) => DELEGATION_EXECUTION_CAPABILITIES.includes(capability))) {
      conformanceRunner ??= new CodexConformanceRunner({ baseEnvironment: environment, baseInstructions: baseInstructions.trim(), command: [cli.path, "app-server"], cwd: workspace, nodeCommand: process.execPath });
      const conformance = await conformanceRunner.verify({ apiKey: apiKey.trim(), connection, credentialScope: `release-certification:${connection.id}`, protocol: probe.selectedProtocol, probeEvidence: probe.records, qualificationContextDigest, now });
      conformanceAttempts.push(conformance.attempt);
      diagnostics.attempts.push(conformance.attempt);
      const failed = conformance.records.filter((record) => record.status !== "verified");
      const attemptFailed = conformance.attempt.status !== "verified";
      const attemptFailure = attemptFailed
        ? `suite:${conformance.attempt.category ?? "execution-stage-failed"}`
        : null;
      const executionSuite = defineQualificationSuiteResult({
        kind: "execution-conformance",
        suiteId: "codex-engine-delegation-execution-v1",
        suiteVersion: 2,
        status: failed.length || attemptFailed ? "failed" : "passed",
        blockingFailures: [...failed.map(capabilityFailure), ...(attemptFailure ? [attemptFailure] : [])],
        metrics: {
          capabilities: conformance.records.map((record) => record.capability),
          capabilityResults: conformance.attempt.metrics?.capabilityResults ?? [],
          parentSynthesizedReceipt: conformance.attempt.metrics?.parentSynthesizedReceipt ?? null,
          terminationCategory: conformance.attempt.category ?? null,
        },
      });
      suiteResults.push(executionSuite);
      diagnostics.suiteResults.push(executionSuite);
      if (executionSuite.status !== "passed") throw new Error(`delegation execution conformance failed: ${executionSuite.blockingFailures.join(", ")}`);
      for (const record of conformance.records) verified.add(record.capability);
    }
    const required = profile.requiredCapabilities;
    const missing = required.filter((capability) => !verified.has(capability));
    if (missing.length) throw new Error(`certification lacks profile capabilities: ${missing.join(", ")}`);
    const report = {
      suiteId: "codex-engine-model-certification-v1",
      suiteVersion: 1,
      outcome: "passed",
      connectionId: connection.id,
      protocol: probe.selectedProtocol,
      profileId: profile.id,
      codecVersion: getCodec(probe.selectedProtocol).version,
      attempts: [...probe.attempts, ...conformanceAttempts],
      suiteResults,
      capabilities: [...verified].sort(),
      executionConformance: conformanceAttempts.at(-1)?.metrics ?? null,
      qualificationContextDigest,
    };
    const certification = defineModelCertification({
      connectionId: connection.id,
      connectionDigest: connectionDefinitionDigest(connection),
      protocol: probe.selectedProtocol,
      codecVersion: getCodec(probe.selectedProtocol).version,
      engineContractHash: contractHash(),
      qualificationContextDigest,
      profileId: profile.id,
      capabilities: [...verified].sort(),
      routes: profile.requiredCapabilities.includes(CAPABILITIES.CHILD_DIRECTED_ROUTE) ? [{ parentConnectionId: connection.id, childConnectionId: connection.id }] : [],
      checkedAt: now.toISOString(),
      expiresAt: new Date(now.getTime() + 30 * 24 * 60 * 60 * 1000).toISOString(),
      report,
      reportDigest: digest(report),
      suiteResults,
      outcome: "passed",
    });
    if (publish) {
      const policy = JSON.parse(readFileSync(activePolicyPath, "utf8"));
      const existing = JSON.parse(readFileSync(certificationPath(dirname(activePolicyPath), policy), "utf8"));
      const certifications = [...existing.certifications.filter((candidate) => !(candidate.connectionId === connection.id && candidate.profileId === profile.id)), certification]
        .sort((left, right) => `${left.connectionId}:${left.profileId}`.localeCompare(`${right.connectionId}:${right.profileId}`));
      policy.modelCatalogSha256 = sha256(activeCatalogPath);
      publishCertificationSnapshot(activePolicyPath, policy, { version: 2, certifications });
    }
    const reportPath = writeDiagnosticReport({
      connection,
      diagnostics: { ...diagnostics, certificationId: certification.certificationId },
      now,
      profile,
      qualificationContextDigest,
      reportDirectory: paths.reportDirectory ?? join(root, ".certification-reports"),
      secret: apiKey.trim(),
      status: "passed",
    });
    return { certificationId: certification.certificationId, connectionId: connection.id, expiresAt: certification.expiresAt, profileId: certification.profileId, protocol: certification.protocol, published: publish, reportPath };
  } catch (error) {
    const reportPath = writeDiagnosticReport({ connection, diagnostics, error, now, profile, qualificationContextDigest, reportDirectory: paths.reportDirectory ?? join(root, ".certification-reports"), secret: apiKey.trim(), status: "failed" });
    if (error instanceof Error) error.reportPath = reportPath;
    throw error;
  } finally {
    rmSync(workspace, { recursive: true, force: true });
  }
}

import { parseModelCliOptions } from "./model-cli-options.mjs";

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  const { connectionId, profileId, help } = parseModelCliOptions(process.argv.slice(2));
  if (help) {
    process.stdout.write("Usage: npm run certify:model -- [connection-id] [--profile profile-id]\nRuns conformance and publishes a passing certification.\n");
    process.exit(0);
  }
  const environment = resolveDevelopmentEnvironment(loadProjectDevelopmentEnvironment(root, process.env));
  const result = await certifyModel({ connectionId, environment, profileId });
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

import { runNativeQualification } from "./native-run.mjs";
import { nativeBinding, nativeCall } from "../src/native.mjs";
import { fileURLToPath } from "node:url";

// Product-neutral Codex child-lifecycle conformance runner.

import {
  CAPABILITIES,
  defineConnection,
  resolveSessionPlan,
} from "@eternallight/agent-engine";

const DEFAULT_MCP_PATH = fileURLToPath(new URL("./conformance-mcp.mjs", import.meta.url));
const CONFORMANCE_ENVIRONMENT_KEYS = [
  "PATH",
  "PATHEXT",
  "SystemRoot",
  "WINDIR",
  "ComSpec",
  "TEMP",
  "TMP",
  "TMPDIR",
  "LANG",
  "LC_ALL",
  "SSL_CERT_FILE",
  "SSL_CERT_DIR",
  "HTTP_PROXY",
  "HTTPS_PROXY",
  "NO_PROXY",
];

function isolatedEnvironment(baseEnvironment) {
  return Object.fromEntries(CONFORMANCE_ENVIRONMENT_KEYS
    .filter((key) => typeof baseEnvironment[key] === "string" && baseEnvironment[key])
    .map((key) => [key, baseEnvironment[key]]));
}

function boundedFailure(error) {
  return String(error instanceof Error ? error.message : error).slice(0, 500);
}

export class CodexConformanceRunner {
  constructor({ baseEnvironment = process.env, baseInstructions, command, cwd, nodeCommand = process.execPath, mcpPath = DEFAULT_MCP_PATH, timeoutMs = 90_000, runProbe = null }) {
    if (!Array.isArray(command) || command.length === 0) throw new Error("conformance runner requires the current Codex App Server command");
    if (typeof baseInstructions !== "string" || !baseInstructions.trim()) throw new Error("conformance runner requires upstream Codex instructions");
    this.baseInstructions = baseInstructions;
    this.baseEnvironment = isolatedEnvironment(baseEnvironment);
    this.command = command;
    this.cwd = cwd;
    this.nodeCommand = nodeCommand;
    this.mcpPath = mcpPath;
    this.timeoutMs = timeoutMs;
    this.runProbe = runProbe ?? ((input) => this.#run(input));
  }

  async verify({ apiKey, connection, credentialScope = connection.credentialRef, protocol, probeEvidence, qualificationContextDigest = "default", now = new Date() }) {
    const selectedConnection = defineConnection(connection);
    const plan = resolveSessionPlan({
      connections: [selectedConnection], evidence: probeEvidence, now,
      policy: { capabilityProfileId: "text-v1", requiredCapabilities: [CAPABILITIES.STRUCTURED_TOOL_CALLS],
        credentialScopes: { [selectedConnection.id]: credentialScope }, defaultConnectionId: selectedConnection.id,
        allowedChildConnectionIds: [selectedConnection.id], forceBridge: true, qualificationContextDigest,
        allowedMcpServerIds: ["engine_conformance"] },
    });
    const series = new (nativeBinding().NativeQualificationSeries)(JSON.stringify({
      connection: selectedConnection, sessionPlan: plan, credentialScope, protocol, qualificationContextDigest, now: now.toISOString(),
    }));
    for (let step; (step = JSON.parse(series.apply("next", "{}"))) !== null;) {
      let observation;
      try {
        const result = await this.runProbe({ ...step, credentials: { [selectedConnection.credentialRef]: apiKey }, connections: [selectedConnection] });
        observation = { scenario: step.scenario, result };
      } catch (error) {
        observation = { scenario: step.scenario, error: { message: boundedFailure(error),
          conformanceObservations: error?.conformanceObservations, conformanceDiagnostics: error?.conformanceDiagnostics,
          budgetSnapshot: error?.budgetSnapshot } };
      }
      series.apply("record", JSON.stringify(observation));
    }
    return JSON.parse(series.apply("finish", "{}"));
  }

  async verifyDirectedEdge({ parentApiKey, childApiKey, parentConnection, childConnection, parentCredentialScope = parentConnection.credentialRef, childCredentialScope = childConnection.credentialRef, parentProtocol, childProtocol, parentProbeEvidence, childProbeEvidence, qualificationContextDigest = "default", now = new Date() }) {
    if (parentConnection.id === childConnection.id) throw new Error("directed edge conformance requires distinct connections");
    const parent = defineConnection(parentConnection);
    const child = defineConnection(childConnection);
    const resolveRoute = (connection, evidence, credentialScope) => resolveSessionPlan({
      connections: [connection],
      evidence,
      now,
      policy: { capabilityProfileId: "text-v1", requiredCapabilities: [CAPABILITIES.STRUCTURED_TOOL_CALLS], credentialScopes: { [connection.id]: credentialScope }, defaultConnectionId: connection.id, allowedChildConnectionIds: [connection.id], forceBridge: true, qualificationContextDigest, allowedMcpServerIds: ["engine_conformance"] },
    });
    const parentPlan = resolveRoute(parent, parentProbeEvidence, parentCredentialScope);
    const childPlan = resolveRoute(child, childProbeEvidence, childCredentialScope);
    const specification = { parentConnection: parent, childConnection: child, parentCredentialScope, childCredentialScope,
      parentProtocol, childProtocol, parentPlan, childPlan, qualificationContextDigest, now: now.toISOString() };
    const sessionPlan = nativeCall("qualification.edgePlan", specification);
    let observation;
    try {
      const result = await this.runProbe({ credentials: { [parent.credentialRef]: parentApiKey, [child.credentialRef]: childApiKey },
        connections: [parent, child], sessionPlan, childModel: child.id, scenario: "spawn-route" });
      observation = { result };
    } catch (error) {
      observation = { error: { message: boundedFailure(error), conformanceObservations: error?.conformanceObservations } };
    }
    return nativeCall("qualification.edgeResult", { ...specification, ...observation });
  }

  #run(input) { return runNativeQualification(input, this); }
}

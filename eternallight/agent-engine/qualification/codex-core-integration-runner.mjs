import { fileURLToPath } from "node:url";
import { CAPABILITIES, resolveSessionPlan } from "../src/index.mjs";
import { runNativeQualification } from "./native-run.mjs";

const DEFAULT_MCP_PATH = fileURLToPath(new URL("./core-conformance-mcp.mjs", import.meta.url));

export class CodexCoreIntegrationRunner {
  constructor({ baseEnvironment = process.env, baseInstructions, command, cwd, nodeCommand = process.execPath, mcpPath = DEFAULT_MCP_PATH, timeoutMs = 60_000 }) {
    if (!Array.isArray(command) || command.length === 0) throw new Error("core integration runner requires the current Codex App Server command");
    if (typeof baseInstructions !== "string" || !baseInstructions.trim()) throw new Error("core integration runner requires upstream Codex instructions");
    Object.assign(this, { baseEnvironment, baseInstructions, command, cwd, nodeCommand, mcpPath, timeoutMs });
  }
  async verify({ apiKey, connection, credentialScope = connection.credentialRef, protocol, probeEvidence, qualificationContextDigest = "default", now = new Date() }) {
    const sessionPlan = resolveSessionPlan({
      connections: [connection], evidence: probeEvidence, now,
      policy: { capabilityProfileId: "text-v1", requiredCapabilities: [CAPABILITIES.STRUCTURED_TOOL_CALLS],
        credentialScopes: { [connection.id]: credentialScope }, defaultConnectionId: connection.id,
        forceBridge: true, qualificationContextDigest, allowedMcpServerIds: ["engine_core"] },
    });
    return runNativeQualification({
      credentials: { [connection.credentialRef]: apiKey }, connections: [connection], sessionPlan, scenario: "core",
      connection, credentialScope, protocol, qualificationContextDigest, now: now.toISOString(),
    }, this);
  }
}

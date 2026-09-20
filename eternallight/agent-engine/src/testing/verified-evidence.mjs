import { CAPABILITIES, DELEGATION_EXECUTION_CAPABILITIES, ENGINE_CAPABILITIES, ENGINE_CONTRACT, contractHash, digest } from "../contracts/capabilities.mjs";
import { defineEvidence } from "../capabilities/evidence-store.mjs";
import { connectionDefinitionDigest, connectionEnvironmentDigest, credentialRealmId } from "../connections/connection-spec.mjs";
import { getCodec } from "../codecs/index.mjs";
import { createUnverifiedEvidenceSnapshotForTesting } from "../capabilities/evidence-repository.mjs";
import { registerValidatedEvidence } from "../capabilities/evidence-provenance.mjs";

const ENGINE_CAPABILITY_IDS = new Set(Object.values(ENGINE_CAPABILITIES));

function capabilityMode(capability) {
  return capability === CAPABILITIES.STREAMING ? "native-sse" : ENGINE_CAPABILITY_IDS.has(capability) ? "app-server-mcp" : "buffered";
}

function capabilityToolTypes(capability) {
  return capability === CAPABILITIES.STRUCTURED_TOOL_CALLS || ENGINE_CAPABILITY_IDS.has(capability) ? ["function"] : [];
}

/** Test-only evidence fixture. Never use this helper to authorize a production session. */
export function verifiedBaselineEvidence(connection, {
  capabilities = [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.STREAMING, CAPABILITIES.USAGE, CAPABILITIES.TOOL_LOOP, ...DELEGATION_EXECUTION_CAPABILITIES],
  checkedAt = null,
  credentialScope = connection.credentialRef,
  engineContract = ENGINE_CONTRACT,
  evidenceRefPrefix = `baseline:${connection.id}`,
  expiresAt = null,
  now = new Date(),
  protocol = connection.protocol,
  qualificationContextDigest = "default",
  routeGrants = [{ parentConnectionId: connection.id, childConnectionId: connection.id }],
  ttlMs = 24 * 60 * 60 * 1000,
} = {}) {
  if (protocol === "auto") throw new Error("baseline evidence requires a selected protocol");
  const checked = checkedAt ? new Date(checkedAt) : now;
  const expires = expiresAt ? new Date(expiresAt) : new Date(checked.getTime() + ttlMs);
  if (!Number.isFinite(checked.getTime()) || !Number.isFinite(expires.getTime()) || expires <= checked) throw new Error("baseline evidence validity window is invalid");
  const environmentDigest = connectionEnvironmentDigest(connection, { credentialScope, engineContractHash: contractHash(engineContract), protocol, qualificationContextDigest, codecVersion: getCodec(protocol).version });
  const credentialRealm = credentialRealmId(credentialScope);
  const codecVersion = getCodec(protocol).version;
  const reportDigest = digest({ fixture: true, connectionId: connection.id, protocol, checkedAt: checked.toISOString(), capabilities });
  return registerValidatedEvidence(capabilities.flatMap((capability) => {
    const routeScopes = capability === CAPABILITIES.CHILD_DIRECTED_ROUTE ? routeGrants.map((route) => [route]) : [null];
    return routeScopes.map((routes, routeIndex) => defineEvidence({
    capability,
    status: "verified",
    scope: {
      protocol,
      suite: "test-baseline-v1",
      mode: capabilityMode(capability),
      toolTypes: capabilityToolTypes(capability),
      ...(routes ? { routes } : {}),
    },
    source: "conformance-suite",
    credentialRealm,
    reportRef: reportDigest,
    reportDigest,
    environmentDigest,
    evidenceRef: `${evidenceRefPrefix}:${capability}${routes ? `:${routeIndex}` : ""}`,
    checkedAt: checked.toISOString(),
    expiresAt: expires.toISOString(),
    sample: ENGINE_CAPABILITY_IDS.has(capability) ? {
      suiteId: capability === CAPABILITIES.TOOL_LOOP ? "codex-engine-core-integration-v1" : "codex-engine-delegation-execution-v1",
      outcome: "passed",
    } : null,
    assertions: [{ id: `fixture:${capability}`, result: "passed", expected: true, observed: true, traceRef: reportDigest }],
    dependencies: {
      modelProtocol: { digest: digest({ connectionDigest: connectionDefinitionDigest(connection), protocol, codecVersion }), ref: `${connection.id}:${protocol}:${codecVersion}` },
      access: { digest: credentialRealm, ref: `${connection.id}:credential-realm` },
      ...(ENGINE_CAPABILITY_IDS.has(capability) ? { engineBuild: { digest: digest({ engineContractHash: contractHash(engineContract), qualificationContextDigest }), ref: qualificationContextDigest } } : {}),
      ...(routes ? { route: { digest: digest(routes), ref: stableRouteReference(routes) } } : {}),
    },
    }));
  }));
}

export function trustedTestEvidence(records, { now = new Date() } = {}) {
  return createUnverifiedEvidenceSnapshotForTesting(records, { now });
}

function stableRouteReference(routes) {
  return routes.map((route) => `${route.parentConnectionId}->${route.childConnectionId}`).sort().join(",");
}

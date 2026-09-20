import { ENGINE_CONTRACT, PROTOCOLS, contractHash, digest } from "../contracts/capabilities.mjs";
import { ensureTrustedEvidenceSnapshot } from "../capabilities/evidence-repository.mjs";
import { scopeCovers } from "../capabilities/evidence-store.mjs";
import { evaluateCapabilityProof } from "../capabilities/proof-contract.mjs";
import { connectionDefinitionDigest, connectionEnvironmentDigest, credentialRealmId } from "../connections/connection-spec.mjs";
import { getCodec } from "../codecs/index.mjs";
import { directedRouteDependencyDigest } from "./directed-route-proof.mjs";
import { nativeCall } from "../native.mjs";

export function isQualifiedEvidence(record, requiredScope = null, expectedDependencies = {}) {
  return Boolean(record && evaluateCapabilityProof(record, { expectedDependencies }).status === "qualified"
    && (!requiredScope || scopeCovers(record.scope, requiredScope)));
}

function deepFreeze(value) {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) deepFreeze(child);
    Object.freeze(value);
  }
  return value;
}

// Digest serialization remains at the JS boundary; all admission and routing decisions are native.
function contexts(connections, policy, contractDigest) {
  return Object.fromEntries(connections.map(connection => {
    const credentialScope = policy.credentialScopes?.[connection.id] ?? connection.credentialRef;
    const credentialRealm = credentialRealmId(credentialScope);
    return [connection.id, Object.fromEntries(PROTOCOLS.map(protocol => {
      const codecVersion = getCodec(protocol).version ?? policy.codecVersion ?? "1";
      return [protocol, {
        credentialRealm,
        environmentDigest: connectionEnvironmentDigest(connection, {
          codecVersion, credentialScope, engineContractHash: contractDigest, protocol,
          qualificationContextDigest: policy.qualificationContextDigest,
        }),
        expectedDependencies: {
          modelProtocol: digest({ connectionDigest: connectionDefinitionDigest(connection), protocol, codecVersion }),
          engineBuild: digest({ engineContractHash: contractDigest, qualificationContextDigest: policy.qualificationContextDigest ?? "default" }),
          access: credentialRealm,
        },
      }];
    }))];
  }));
}

export function resolveProtocolSelections(connections, evidence, options = {}) {
  const now = options.now ?? new Date();
  const engineContract = options.engineContract ?? ENGINE_CONTRACT;
  const snapshot = ensureTrustedEvidenceSnapshot(evidence, { now });
  return Object.freeze(nativeCall("session.protocols", {
    connections, records: snapshot.records, now: now.getTime(),
    requiredCapabilities: options.requiredCapabilities ?? engineContract.requiredCapabilities,
    contexts: contexts(connections, options, contractHash(engineContract)),
  }));
}

export function resolveSessionPlan({ connections, evidence, evidenceSnapshot, policy, engineContract = ENGINE_CONTRACT, now = new Date() }) {
  if (!Array.isArray(connections) || !connections.length) throw new Error("at least one connection is required");
  const snapshot = ensureTrustedEvidenceSnapshot(evidenceSnapshot ?? evidence, { now });
  const contractDigest = contractHash(engineContract);
  const input = { connections, policy, engineContract, records: snapshot.records, now: now.getTime(), contexts: contexts(connections, policy, contractDigest) };
  const draft = nativeCall("session.compile", input);
  const byId = new Map(connections.map(connection => [connection.id, connection]));
  const routes = new Map(draft.routes.map(route => [route.connectionId, route]));
  const edges = draft.requestedEdges.map(edge => ({
    edge,
    dependencies: [digest([edge]), directedRouteDependencyDigest({
      parentConnection: byId.get(edge.parentConnectionId), childConnection: byId.get(edge.childConnectionId),
      parentProtocol: routes.get(edge.parentConnectionId).protocol, childProtocol: routes.get(edge.childConnectionId).protocol,
      parentCredentialScope: policy.credentialScopes?.[edge.parentConnectionId], childCredentialScope: policy.credentialScopes?.[edge.childConnectionId],
    })],
  }));
  const { plan, adoptedRecords } = nativeCall("session.finalize", { ...input, draft, edges, contractDigest, policyDigest: digest(policy) });
  plan.evidenceDigest = digest(adoptedRecords
    .map(({ capability, status, scope, source, evidenceRef, reportRef, reportDigest, checkedAt, expiresAt, dependencies }) =>
      ({ capability, status, scope, source, evidenceRef, reportRef, reportDigest, checkedAt, expiresAt, dependencies }))
    .sort((left, right) => left.evidenceRef.localeCompare(right.evidenceRef)));
  return deepFreeze({ id: digest(plan), ...plan });
}

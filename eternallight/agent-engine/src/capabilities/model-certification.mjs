import { ENGINE_CONTRACT, contractHash, digest } from "../contracts/capabilities.mjs";
import { connectionDefinitionDigest, connectionEnvironmentDigest, credentialRealmId } from "../connections/connection-spec.mjs";
import { getCodec } from "../codecs/index.mjs";
import { registerValidatedEvidence } from "./evidence-provenance.mjs";
import { nativeCall } from "../native.mjs";

function deepFreeze(value) {
  if (value && typeof value === "object") { for (const child of Object.values(value)) deepFreeze(child); Object.freeze(value); }
  return value;
}
export function defineModelCertification(input) {
  const body = nativeCall("certification.define", { value: input, computedReportDigest: digest(input?.report ?? null) });
  const certificationId = digest(body);
  if (input.certificationId !== undefined && input.certificationId !== certificationId) throw new Error("model certification digest does not match its contents");
  return deepFreeze({ certificationId, ...body });
}
export function parseModelCertificationDocument(document) {
  if (document?.version !== 2 || !Array.isArray(document.certifications)) throw new Error("unsupported model certification document");
  const certifications = document.certifications.map(defineModelCertification);
  if (new Set(certifications.map(record => record.certificationId)).size !== certifications.length) throw new Error("model certification ids must be unique");
  return deepFreeze({ version: 2, certifications });
}
export function materializeCertificationEvidence(certificationInput, {
  connection, credentialScope = connection.credentialRef, engineContract = ENGINE_CONTRACT, now = new Date(), qualificationContextDigest,
} = {}) {
  const certification = defineModelCertification(certificationInput);
  const records = nativeCall("certification.materialize", {
    certification, connection, now: now.getTime(), qualificationContextDigest,
    connectionDigest: connectionDefinitionDigest(connection), codecVersion: getCodec(connection.protocol).version,
    contractDigest: contractHash(engineContract), credentialRealm: credentialRealmId(credentialScope),
    environmentDigest: connectionEnvironmentDigest(connection, {
      codecVersion: certification.codecVersion, credentialScope, engineContractHash: certification.engineContractHash,
      protocol: certification.protocol, qualificationContextDigest,
    }),
    modelProtocolDigest: digest({ connectionDigest: certification.connectionDigest, protocol: certification.protocol, codecVersion: certification.codecVersion }),
    engineBuildDigest: digest({ engineContractHash: certification.engineContractHash, qualificationContextDigest }),
    routeDigest: digest(certification.routes),
  });
  return registerValidatedEvidence(records);
}

import { ENGINE_CONTRACT } from "../contracts/capabilities.mjs";
import { registerValidatedEvidence } from "./evidence-provenance.mjs";
import { nativeCall } from "../native.mjs";

function deepFreeze(value) {
  if (value && typeof value === "object") { for (const child of Object.values(value)) deepFreeze(child); Object.freeze(value); }
  return value;
}
export const defineModelCertification = input => deepFreeze(nativeCall("certification.complete", input));
export const parseModelCertificationDocument = input => deepFreeze(nativeCall("certification.document", input));
export function materializeCertificationEvidence(certification, {
  connection, credentialScope = connection.credentialRef, engineContract = ENGINE_CONTRACT, now = new Date(), qualificationContextDigest,
}) {
  return registerValidatedEvidence(nativeCall("certification.evidence", {
    certification, connection, credentialScope, engineContract, now: now.getTime(), qualificationContextDigest,
  }));
}

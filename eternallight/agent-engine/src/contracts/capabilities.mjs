import { nativeCall } from "../native.mjs";

function freezeDeep(value) {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) freezeDeep(child);
    Object.freeze(value);
  }
  return value;
}

export const {
  PROTOCOLS, MODEL_CAPABILITIES, ENGINE_CAPABILITIES, CAPABILITIES,
  CAPABILITY_DOMAINS, DELEGATION_EXECUTION_CAPABILITIES, RESILIENT_DELEGATION_CAPABILITIES,
  CAPABILITY_DEPENDENCIES, CAPABILITY_SUPPORT_MODES, CAPABILITY_PROFILES,
  ENGINE_CAPABILITY_MANIFEST, ENGINE_CONTRACT,
} = freezeDeep(nativeCall("contracts.definitions"));

export const capabilityDomain = (capability) => nativeCall("contracts.domain", capability);
export const resolveCapabilityProfile = (profileId = "base-tools-v1") => CAPABILITY_PROFILES[nativeCall("contracts.profile", profileId).id];

export function stableJson(value) {
  return nativeCall("identity.stableJson", value);
}

export function digest(value) {
  return nativeCall("identity.digest", value);
}

export function contractHash(contract = ENGINE_CONTRACT) {
  return digest(contract);
}

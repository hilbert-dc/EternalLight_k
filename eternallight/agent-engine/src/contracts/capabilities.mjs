import { createHash } from "node:crypto";
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
  if (Array.isArray(value)) return `[${value.map(stableJson).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${stableJson(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

export function digest(value) {
  return createHash("sha256").update(stableJson(value), "utf8").digest("hex");
}

export function contractHash(contract = ENGINE_CONTRACT) {
  return digest(contract);
}

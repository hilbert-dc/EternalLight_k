import { CAPABILITIES, resolveCapabilityProfile } from "@eternallight/agent-engine";

const BASE_OPERATION = Object.freeze({
  capabilityProfileId: "base-tools-v1",
  requiredCapabilities: Object.freeze([CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP]),
  preferredCapabilities: Object.freeze([CAPABILITIES.STREAMING, CAPABILITIES.USAGE]),
});

export const OPERATION_REQUIREMENTS = Object.freeze({
  advisory: BASE_OPERATION,
  reviewQuality: BASE_OPERATION,
  generateBrief: BASE_OPERATION,
  generatePropertyRules: BASE_OPERATION,
  generateOutline: BASE_OPERATION,
  generatePlan: BASE_OPERATION,
  generateChapter: BASE_OPERATION,
  modelKnowledge: BASE_OPERATION,
  assessSceneOutcome: BASE_OPERATION,
  assessRevisionOutcome: BASE_OPERATION,
  assessChapterStructure: BASE_OPERATION,
  extractCommitments: BASE_OPERATION,
  reviseCommitment: BASE_OPERATION,
  // Planning review is an application-orchestrated fixed panel. Each seat is
  // an independent root task, so it must not be gated on model-led delegation.
  reviewPlanningSeat: BASE_OPERATION,
  autonomousDelegation: Object.freeze({
    capabilityProfileId: "autonomous-delegation-v1",
    requiredCapabilities: Object.freeze([...resolveCapabilityProfile("autonomous-delegation-v1").requiredCapabilities]),
    preferredCapabilities: Object.freeze([CAPABILITIES.STREAMING, CAPABILITIES.USAGE]),
  }),
});

export const PRODUCT_OPERATION_IDS = Object.freeze([
  "advisory",
  "reviewQuality",
  "generateBrief",
  "generatePropertyRules",
  "generateOutline",
  "generatePlan",
  "generateChapter",
  "modelKnowledge",
  "assessSceneOutcome",
  "assessRevisionOutcome",
  "assessChapterStructure",
  "extractCommitments",
  "reviseCommitment",
  "reviewPlanningSeat",
]);

export function operationRequirement(operation) {
  if (operation === null || operation === undefined) return BASE_OPERATION;
  const requirement = OPERATION_REQUIREMENTS[operation];
  if (!requirement) throw new Error(`unknown product operation: ${operation}`);
  return requirement;
}

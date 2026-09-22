import { nativeCall } from "../src/native.mjs";

function freeze(value) {
  if (value && typeof value === "object") { Object.values(value).forEach(freeze); Object.freeze(value); }
  return value;
}
export const evaluateDelegationCapability = (capability, observations = {}, traceRef = null) =>
  freeze(nativeCall("delegation.evaluate", { capability, observations, traceRef }));
export const observedTerminalChildWait = (events, childRequests, expectedText, responses = [], requests = []) =>
  nativeCall("delegation.terminalWait", { events, childRequests, expectedText, responses, requests });
export const summarizeCollaborationEvents = (events = [], responses = []) =>
  freeze(nativeCall("delegation.summary", { events, responses }));

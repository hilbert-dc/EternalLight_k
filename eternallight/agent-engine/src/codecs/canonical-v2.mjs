import { nativeBinding, nativeCall, UnsupportedProtocolStateError } from "../native.mjs";
import { ToolRegistry } from "./tool-registry.mjs";
export { UnsupportedProtocolStateError };
export const SSE_DONE = Symbol("agent-engine-sse-done");
export const conservativeTokenUpperBound = (value) => typeof value === "string" ? Buffer.byteLength(value, "utf8") : 0;
export const modelVisibleTokenBudget = (value) => nativeCall("tokens.count", typeof value === "string" ? value : "");
export const assertBoundedModelItem = (value, label = "model-visible item") => { nativeCall("tokens.bound", { value, label }); };
export const extractInitialInstructions = (messages, protocol) => nativeCall("canonical.initial", { messages, protocol });
export const parseJsonArguments = (value) => nativeCall("canonical.arguments", value);
function hydrate(context) {
  const registry = new ToolRegistry({ maxTools: context.registry.maxTools });
  registry.entries = context.registry.entries;
  return Object.freeze({ ...context, registry });
}
export const prepareCanonicalRequest = (request, { protocol, connection } = {}) => hydrate(nativeCall("canonical.prepare", { request, protocol, connection }));
export const finalizeProviderRequest = (context, body, items = []) => hydrate(nativeCall("canonical.finalize", { context, body, items }));
export function createResponseValidation(context) {
  if (!context?.registry) throw new Error("prepared request context is required to decode provider tool calls");
  return { context, callIds: [] };
}
export function canonicalToolCall(call, validation) {
  const result = nativeCall("canonical.call", { call, validation });
  validation.callIds = result.validation.callIds;
  return result.call;
}
export const assertToolChoiceSatisfied = (context, items) => nativeCall("canonical.choice", { context, items });
export const responseItemFromText = (text, id) => nativeCall("canonical.text", { text, id });
export function responseItemFromRefusal(refusal, id) {
  if (typeof refusal !== "string" || !refusal) throw new Error("provider refusal must be non-empty text");
  return responseItemFromText(refusal, id);
}
export const responseItemFromToolCall = (call, id) => nativeCall("canonical.tool", { call, id });
export const responseEnvelope = (items, usage = null, id, options = {}) => nativeCall("canonical.envelope", { items, usage, id, options });
export const responseEvents = (response) => nativeCall("canonical.events", response);
export const endpointWithPath = (endpoint, path) => nativeCall("canonical.endpoint", { endpoint, path });

export async function* jsonSseEvents(body, { maxBytes = 8 * 1024 * 1024 } = {}) {
  if (!body) throw new Error("streaming response has no body");
  if (!Number.isInteger(maxBytes) || maxBytes <= 0 || maxBytes > 40 * 1024 * 1024) throw new Error("invalid streaming response limit");
  const decoder = new (nativeBinding().NativeSseDecoder)(maxBytes);
  try {
    for await (const chunk of body) {
      for (const event of JSON.parse(decoder.push(Buffer.from(chunk)))) yield event.done ? SSE_DONE : event.data;
    }
    for (const event of JSON.parse(decoder.finish())) yield event.done ? SSE_DONE : event.data;
  } finally { decoder.close(); }
}

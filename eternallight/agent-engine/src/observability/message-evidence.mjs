import { nativeCall } from "../native.mjs";

export const textDigest = (text) => nativeCall("message.digest", text);
export const visibleTextDigests = (body) => nativeCall("message.visible", body);
export const acknowledgedMessage = (events, responses, expectedDigest) => nativeCall("message.acknowledged", { events, responses, expectedDigest });
export const waitResultFacts = (input) => nativeCall("message.wait", input);
export const finalAgentMessageFacts = (input) => nativeCall("message.final", input);

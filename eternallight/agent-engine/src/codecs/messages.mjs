import { nativeCall } from "../native.mjs";
export const messagesCodec = Object.freeze({
  id: "messages",
  version: "3",
  createRequest(request, connection, apiKey) { return nativeCall("codec.encode", { protocol: this.id, request, connection, apiKey }); },
  parseResponse(payload, context) { return nativeCall("codec.decode", { protocol: this.id, payload, context }); },
});

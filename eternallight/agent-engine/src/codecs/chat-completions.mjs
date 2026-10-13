import { nativeBinding, nativeCall, normalizeNativeError } from "../native.mjs";
import { jsonSseEvents, SSE_DONE } from "./canonical-v2.mjs";

export const chatCompletionsCodec = Object.freeze({
  id: "chat-completions",
  version: "3",
  createRequest(request, connection, apiKey) { return nativeCall("codec.encode", { protocol: this.id, request, connection, apiKey }); },
  parseResponse(payload, context) { return nativeCall("codec.decode", { protocol: this.id, payload, context }); },
  async *parseStream(body, context) {
    const stream = new (nativeBinding().NativeChatStream)(JSON.stringify(context ?? null));
    try {
      for await (const data of jsonSseEvents(body)) {
        for (const event of JSON.parse(stream.push(JSON.stringify(data === SSE_DONE ? { done: true } : { data })))) yield event;
      }
      for (const event of JSON.parse(stream.finish())) yield event;
    } catch (error) { throw normalizeNativeError(error); }
    finally { stream.close(); }
  },
});

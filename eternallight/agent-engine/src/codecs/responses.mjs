import { nativeBinding, nativeCall } from "../native.mjs";
import { jsonSseEvents, SSE_DONE } from "./canonical-v2.mjs";

export const responsesCodec = Object.freeze({
  id: "responses",
  version: "3",
  createRequest(request, connection, apiKey) { return nativeCall("responses.encode", { request, connection, apiKey }); },
  parseResponse(payload, context) { return nativeCall("responses.decode", { payload, context }); },
  async *parseStream(body, context) {
    const stream = new (nativeBinding().NativeResponsesStream)(JSON.stringify(context ?? null));
    try {
      for await (const event of jsonSseEvents(body)) {
        if (event === SSE_DONE) continue;
        for (const ready of JSON.parse(stream.push(JSON.stringify(event)))) yield ready;
      }
      for (const ready of JSON.parse(stream.finish())) yield ready;
    } finally { stream.close(); }
  },
});

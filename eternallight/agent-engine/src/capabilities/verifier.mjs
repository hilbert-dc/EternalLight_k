import { ENGINE_CONTRACT } from "../contracts/capabilities.mjs";
import { getCodec } from "../codecs/index.mjs";
import { registerValidatedEvidence } from "./evidence-provenance.mjs";
import { nativeFetch } from "../http-transport.mjs";
import { nativeBinding } from "../native.mjs";

async function* boundedBody(response, maximum) {
  const declared = Number(response.headers.get("content-length"));
  if (Number.isFinite(declared) && declared > maximum) {
    await response.body?.cancel();
    throw Object.assign(new Error("probe response exceeds byte limit"), { status: response.status, category: "transport" });
  }
  let bytes = 0;
  for await (const chunk of response.body ?? []) {
    bytes += chunk.byteLength;
    if (bytes > maximum) throw Object.assign(new Error("probe response exceeds byte limit"), { status: response.status, category: "transport" });
    yield chunk;
  }
}

/** Transport adapter; protocol selection, retries and evidence belong to Rust. */
export class CapabilityVerifier {
  constructor({ fetchImpl = nativeFetch, maxAttempts = 4, timeoutMs = 20_000, maxResponseBytes = 1024 * 1024, ttlMs = 86_400_000, engineContract = ENGINE_CONTRACT } = {}) {
    Object.assign(this, { fetchImpl, maxAttempts, timeoutMs, maxResponseBytes, ttlMs, engineContract });
  }
  async verify(connection, { apiKey, protocols = null, now = new Date(), credentialScope = connection.credentialRef, qualificationContextDigest = "default" }) {
    const native = new (nativeBinding().NativeCapabilityVerifier)(JSON.stringify({
      connection, apiKey, protocols, now: now.toISOString(), credentialScope, qualificationContextDigest,
      maxAttempts: this.maxAttempts, timeoutMs: this.timeoutMs, maxResponseBytes: this.maxResponseBytes, ttlMs: this.ttlMs, engineContract: this.engineContract,
    }));
    const apply = (method, input = {}) => JSON.parse(native.apply(method, JSON.stringify(input)));
    for (let request; (request = apply("next")) !== null;) {
      let observed;
      try { observed = await this.#transport(request); }
      catch (error) { observed = { error: { name: error?.name, status: error?.status, category: error?.category, message: String(error?.message ?? error) } }; }
      apply("response", observed);
    }
    const result = apply("result");
    if (result.error) throw Object.assign(new Error(result.error), { records: result.records, attempts: result.attempts });
    return { ...result, records: registerValidatedEvidence(result.records) };
  }
  async #transport(request) {
    const response = await this.fetchImpl(request.url, {
      method: "POST", headers: request.headers, body: JSON.stringify(request.body),
      signal: AbortSignal.timeout(request.timeoutMs), timeoutMs: request.timeoutMs, maxBytes: request.maxResponseBytes,
    });
    if (request.stream && response.ok) {
      if (!(response.headers.get("content-type") ?? "").includes("text/event-stream")) {
        await response.body?.cancel();
        throw Object.assign(new Error("streaming probe response is not SSE"), { status: 422 });
      }
      let canonical = null;
      for await (const event of getCodec(request.protocol).parseStream(boundedBody(response, request.maxResponseBytes), request.context)) {
        if (event?.type === "response.completed") canonical = event.response;
      }
      return { status: response.status, canonical };
    }
    const chunks = [];
    for await (const chunk of boundedBody(response, request.maxResponseBytes)) chunks.push(Buffer.from(chunk));
    return { status: response.status, text: Buffer.concat(chunks).toString("utf8") };
  }
}

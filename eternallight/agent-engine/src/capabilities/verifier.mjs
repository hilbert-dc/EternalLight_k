import { CAPABILITIES, ENGINE_CONTRACT, contractHash, digest } from "../contracts/capabilities.mjs";
import { connectionDefinitionDigest, connectionEnvironmentDigest, credentialRealmId } from "../connections/connection-spec.mjs";
import { getCodec } from "../codecs/index.mjs";
import { registerValidatedEvidence } from "./evidence-provenance.mjs";

const DEFAULT_PROTOCOL_ORDER = ["responses", "chat-completions", "messages", "generate-content"];

function classify(error) {
  if (typeof error?.category === "string") return error.category;
  if (error?.name === "AbortError" || error?.name === "TimeoutError") return "timeout";
  if (error?.status === 401) return "authentication";
  if (error?.status === 404 || error?.status === 405 || error?.status === 415) return "unsupportedProtocol";
  if (Number.isInteger(error?.status) && error.status === 429) return "rateLimited";
  if (Number.isInteger(error?.status) && error.status >= 400 && error.status < 500) return "invalidRequest";
  return "transport";
}

function errorCategory(status, payload) {
  if (status === 401) return "authentication";
  if (status !== 403) return null;
  const marker = `${payload?.error?.type ?? ""} ${payload?.error?.code ?? ""} ${payload?.code ?? ""}`.toLowerCase();
  return /(auth|api.?key|credential|unauthorized)/.test(marker) ? "authentication" : "invalidRequest";
}

async function boundedResponseText(response, maximumBytes) {
  const declared = Number(response.headers.get("content-length"));
  if (Number.isFinite(declared) && declared > maximumBytes) throw Object.assign(new Error("probe response exceeds byte limit"), { status: response.status, category: "transport" });
  const chunks = [];
  let bytes = 0;
  for await (const chunk of response.body ?? []) {
    bytes += chunk.byteLength;
    if (bytes > maximumBytes) throw Object.assign(new Error("probe response exceeds byte limit"), { status: response.status, category: "transport" });
    chunks.push(Buffer.from(chunk));
  }
  return Buffer.concat(chunks).toString("utf8");
}

function evidence({ capability, connection, credentialRealm, environmentDigest, protocol, status, reference, now, ttlMs, sample = null, source = "probe", mode = "buffered" }) {
  const codecVersion = getCodec(protocol).version;
  return {
    capability,
    status,
    scope: { protocol, suite: "synthetic-tool-probe-v1", mode, toolTypes: ["function"] },
    source,
    credentialRealm,
    environmentDigest,
    evidenceRef: `${reference}:${capability}`,
    checkedAt: now.toISOString(),
    expiresAt: new Date(now.getTime() + ttlMs).toISOString(),
    sample,
    assertions: [{
      id: `model:${capability}:observed`,
      result: status === "verified" ? "passed" : status === "failed" ? "failed" : "unknown",
      expected: "capability observed through the selected wire protocol",
      observed: sample ?? status,
      traceRef: reference,
    }],
    dependencies: {
      modelProtocol: { digest: digest({ connectionDigest: connectionDefinitionDigest(connection), protocol, codecVersion }), ref: `${connection.id}:${protocol}:${codecVersion}` },
      access: { digest: credentialRealm, ref: `${connection.id}:credential-realm` },
    },
  };
}

function failedProtocolEvidence(connection, protocol, category, message, now, ttlMs, engineContract, credentialScope, qualificationContextDigest) {
  const codec = getCodec(protocol);
  const environmentDigest = connectionEnvironmentDigest(connection, {
    codecVersion: codec.version,
    credentialScope,
    engineContractHash: contractHash(engineContract),
    protocol,
    qualificationContextDigest,
  });
  const reference = `probe:${connection.id}:${protocol}:${now.toISOString()}:failed`;
  const sample = { category, message: String(message ?? "probe failed").slice(0, 500) };
  return [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS].map((capability) => evidence({
    capability,
    connection,
    credentialRealm: credentialRealmId(credentialScope),
    environmentDigest,
    protocol,
    status: "failed",
    reference,
    now,
    ttlMs,
    sample,
  }));
}

function probeRequest(connection, { forceToolChoice = true } = {}) {
  return {
    model: connection.id,
    instructions: "This is a protocol conformance probe. Call the engine_probe tool exactly once with {\"nonce\":\"agent-engine\"}. Do not include user data.",
    input: [{ type: "message", role: "user", content: [{ type: "input_text", text: "Run the synthetic protocol probe." }] }],
    tools: [{
      type: "function",
      name: "engine_probe",
      description: "Synthetic side-effect-free protocol probe.",
      parameters: { type: "object", properties: { nonce: { type: "string", const: "agent-engine" } }, required: ["nonce"], additionalProperties: false },
      strict: false,
    }],
    ...(forceToolChoice ? { tool_choice: "required" } : {}),
    stream: false,
    max_output_tokens: Math.min(512, connection.declaredLimits.outputTokens ?? 512),
  };
}

function rejectsForcedToolChoice(payload) {
  const message = `${payload?.error?.message ?? payload?.message ?? ""}`.toLowerCase();
  return /tool[_ -]?choice/.test(message) && /(not support|unsupported|does not support|invalid)/.test(message);
}

function assertToolRoundTrip(canonical, name, expectedArguments) {
  const call = canonical?.output?.find((item) => item?.type === "function_call" && item.name === name);
  if (!call) throw Object.assign(new Error(`probe did not return the ${name} structured tool call`), { status: 422 });
  let argumentsValue;
  try {
    argumentsValue = JSON.parse(call.arguments);
  } catch {
    throw Object.assign(new Error(`probe ${name} arguments are not valid JSON`), { status: 422 });
  }
  for (const [key, value] of Object.entries(expectedArguments)) {
    if (argumentsValue[key] !== value) throw Object.assign(new Error(`probe ${name} arguments failed round-trip validation`), { status: 422 });
  }
  return call;
}

export class CapabilityVerifier {
  constructor({ fetchImpl = nativeFetch, maxAttempts = 4, timeoutMs = 20_000, maxResponseBytes = 1024 * 1024, ttlMs = 24 * 60 * 60 * 1000, engineContract = ENGINE_CONTRACT } = {}) {
    this.fetchImpl = fetchImpl;
    this.maxAttempts = maxAttempts;
    this.timeoutMs = timeoutMs;
    this.maxResponseBytes = maxResponseBytes;
    this.ttlMs = ttlMs;
    this.engineContract = engineContract;
  }

  async verify(connection, { apiKey, protocols = null, now = new Date(), credentialScope = connection.credentialRef, qualificationContextDigest = "default" }) {
    if (typeof apiKey !== "string" || !apiKey) throw new Error("verification requires a credential value");
    const candidates = protocols ?? (connection.protocol === "auto" ? DEFAULT_PROTOCOL_ORDER : [connection.protocol]);
    if (candidates.length > this.maxAttempts) throw new Error("verification protocol budget exceeded");
    const attempts = [];
    const records = [];
    for (const protocol of candidates) {
      try {
        const result = await this.#probe(connection, protocol, apiKey, now, credentialScope, qualificationContextDigest);
        return { selectedProtocol: protocol, records: registerValidatedEvidence([...records, ...result]), attempts: [...attempts, { protocol, status: "verified" }] };
      } catch (error) {
        const category = classify(error);
        attempts.push({ protocol, status: "failed", category, message: error.message });
        if (["unsupportedProtocol", "invalidRequest", "authentication"].includes(category)) {
          records.push(...failedProtocolEvidence(connection, protocol, category, error.message, now, this.ttlMs, this.engineContract, credentialScope, qualificationContextDigest));
        }
        if (["authentication", "rateLimited"].includes(category)) break;
      }
    }
    const failure = new Error(`no protocol passed capability verification: ${attempts.map((attempt) => `${attempt.protocol}:${attempt.category}`).join(", ")}`);
    failure.attempts = attempts;
    failure.records = records;
    throw failure;
  }

  async #probe(connection, protocol, apiKey, now, credentialScope, qualificationContextDigest) {
    const codec = getCodec(protocol);
    const environmentDigest = connectionEnvironmentDigest(connection, {
      codecVersion: codec.version,
      credentialScope,
      engineContractHash: contractHash(this.engineContract),
      protocol,
      qualificationContextDigest,
    });
    let forceToolChoice = true;
    let outbound = codec.createRequest(probeRequest(connection), connection, apiKey);
    let response = await this.fetchImpl(outbound.url, {
      method: "POST",
      headers: outbound.headers,
      body: JSON.stringify(outbound.body),
      signal: AbortSignal.timeout(this.timeoutMs),
      timeoutMs: this.timeoutMs,
      maxBytes: this.maxResponseBytes,
    });
    let text = await boundedResponseText(response, this.maxResponseBytes);
    let payload;
    try {
      payload = text ? JSON.parse(text) : {};
    } catch {
      throw Object.assign(new Error("probe response is not JSON"), { status: response.status });
    }
    if (!response.ok && rejectsForcedToolChoice(payload)) {
      forceToolChoice = false;
      outbound = codec.createRequest(probeRequest(connection, { forceToolChoice }), connection, apiKey);
      response = await this.fetchImpl(outbound.url, {
        method: "POST",
        headers: outbound.headers,
        body: JSON.stringify(outbound.body),
        signal: AbortSignal.timeout(this.timeoutMs),
        timeoutMs: this.timeoutMs,
        maxBytes: this.maxResponseBytes,
      });
      text = await boundedResponseText(response, this.maxResponseBytes);
      try {
        payload = text ? JSON.parse(text) : {};
      } catch {
        throw Object.assign(new Error("probe response is not JSON"), { status: response.status });
      }
    }
    if (!response.ok) throw Object.assign(new Error(payload?.error?.message ?? payload?.message ?? `probe failed (${response.status})`), {
      status: response.status,
      category: errorCategory(response.status, payload),
    });
    const canonical = codec.parseResponse(payload, outbound.context);
    assertToolRoundTrip(canonical, "engine_probe", { nonce: "agent-engine" });
    let streamingStatus = "unknown";
    let streamingSample = { reason: "codec-does-not-support-native-streaming" };
    if (typeof codec.parseStream === "function") {
      try {
        await this.#probeStreaming(codec, connection, apiKey, forceToolChoice);
        streamingStatus = "verified";
        streamingSample = { basis: "native-sse-tool-round-trip" };
      } catch (error) {
        const category = classify(error);
        if (["authentication", "rateLimited"].includes(category)) throw error;
        if (["unsupportedProtocol", "invalidRequest"].includes(category)) streamingStatus = "failed";
        streamingSample = { reason: category, message: String(error.message ?? "streaming probe failed").slice(0, 500) };
      }
    }
    const reference = `probe:${connection.id}:${protocol}:${now.toISOString()}`;
    const records = [
      evidence({ capability: CAPABILITIES.TEXT, connection, credentialRealm: credentialRealmId(credentialScope), environmentDigest, protocol, status: "verified", reference, now, ttlMs: this.ttlMs }),
      evidence({ capability: CAPABILITIES.STRUCTURED_TOOL_CALLS, connection, credentialRealm: credentialRealmId(credentialScope), environmentDigest, protocol, status: "verified", reference, now, ttlMs: this.ttlMs }),
      evidence({ capability: CAPABILITIES.STREAMING, connection, credentialRealm: credentialRealmId(credentialScope), environmentDigest, protocol, status: streamingStatus, reference, now, ttlMs: this.ttlMs, sample: streamingSample, mode: "native-sse" }),
      evidence({ capability: CAPABILITIES.USAGE, connection, credentialRealm: credentialRealmId(credentialScope), environmentDigest, protocol, status: canonical.usage ? "verified" : "unknown", reference, now, ttlMs: this.ttlMs }),
    ];
    return records;
  }

  async #probeStreaming(codec, connection, apiKey, forceToolChoice) {
    const outbound = codec.createRequest({ ...probeRequest(connection, { forceToolChoice }), stream: true }, connection, apiKey);
    const response = await this.fetchImpl(outbound.url, {
      method: "POST",
      headers: outbound.headers,
      body: JSON.stringify(outbound.body),
      signal: AbortSignal.timeout(this.timeoutMs),
      timeoutMs: this.timeoutMs,
      maxBytes: this.maxResponseBytes,
    });
    if (!response.ok) throw Object.assign(new Error(`streaming probe failed (${response.status})`), { status: response.status, category: response.status === 401 ? "authentication" : null });
    if (!(response.headers.get("content-type") ?? "").includes("text/event-stream")) throw Object.assign(new Error("streaming probe response is not SSE"), { status: 422 });
    let completed = null;
    for await (const event of codec.parseStream(response.body, outbound.context)) {
      if (event?.type === "response.completed") completed = event.response;
    }
    const call = completed?.output?.find((item) => item?.type === "function_call" && item.name === "engine_probe");
    if (!call) throw Object.assign(new Error("streaming probe did not return a structured tool call"), { status: 422 });
    let argumentsValue;
    try {
      argumentsValue = JSON.parse(call.arguments);
    } catch {
      throw Object.assign(new Error("streaming probe tool arguments are not valid JSON"), { status: 422 });
    }
    if (argumentsValue.nonce !== "agent-engine") throw Object.assign(new Error("streaming probe tool arguments failed round-trip validation"), { status: 422 });
  }
}
import { nativeFetch } from "../http-transport.mjs";

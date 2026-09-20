import { createHash, randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import { createServer } from "node:http";
import { nativeFetch } from "../http-transport.mjs";
import { getCodec } from "../codecs/index.mjs";
import { responseEvents } from "../codecs/canonical-v2.mjs";
import { CAPABILITIES } from "../contracts/capabilities.mjs";
import { RouteLineage, assertExecutionLease } from "../state/execution-lease.mjs";
import { finalAgentMessageFacts, textDigest, visibleTextDigests, waitResultFacts } from "../observability/message-evidence.mjs";

const DEFAULT_MAX_BODY_BYTES = 2 * 1024 * 1024;
const DEFAULT_MAX_DOWNSTREAM_BYTES = 32 * 1024 * 1024;
const DEFAULT_MAX_CONCURRENT_REQUESTS = 8;
const DEFAULT_REQUEST_TIMEOUT_MS = 120_000;

function assertActiveExecution(lease, sessionPlan, epoch) {
  try {
    return assertExecutionLease(lease, sessionPlan, { epoch });
  } catch (error) {
    throw Object.assign(error, { statusCode: 403 });
  }
}

function readBody(request, maxBytes, signal) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let bytes = 0;
    let settled = false;
    const finish = (callback, value) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", onAbort);
      callback(value);
    };
    const onAbort = () => {
      request.pause();
      finish(reject, Object.assign(new Error("request body read cancelled"), { name: "AbortError" }));
    };
    signal?.addEventListener("abort", onAbort, { once: true });
    request.on("data", (chunk) => {
      bytes += chunk.length;
      if (bytes > maxBytes) {
        finish(reject, Object.assign(new Error("request body exceeds bridge limit"), { statusCode: 413 }));
        request.destroy();
        return;
      }
      chunks.push(chunk);
    });
    request.on("end", () => {
      try {
        finish(resolve, JSON.parse(Buffer.concat(chunks).toString("utf8")));
      } catch {
        finish(reject, Object.assign(new Error("request body is not valid JSON"), { statusCode: 400 }));
      }
    });
    request.on("error", (error) => finish(reject, error));
  });
}

function writeJson(response, status, payload) {
  response.writeHead(status, { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" });
  response.end(JSON.stringify(payload));
}

async function writeFrame(response, event, signal) {
  signal.throwIfAborted();
  if (response.destroyed) throw new Error("bridge response closed");
  if (response.write(`event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`)) return;
  await new Promise((resolve, reject) => {
    const finish = error => {
      response.removeListener("drain", drained);
      response.removeListener("close", closed);
      response.removeListener("error", finish);
      signal.removeEventListener("abort", aborted);
      if (error) reject(error); else resolve();
    };
    const drained = () => finish();
    const closed = () => finish(new Error("bridge response closed"));
    const aborted = () => finish(signal.reason ?? new Error("bridge response cancelled"));
    response.once("drain", drained);
    response.once("close", closed);
    response.once("error", finish);
    signal.addEventListener("abort", aborted, { once: true });
    if (signal.aborted) aborted();
    else if (response.destroyed) closed();
  });
}

async function observe(observer, payload) {
  try {
    await observer?.(payload);
  } catch {
    // Diagnostics are deliberately non-authoritative and cannot change execution.
  }
}

function responseObservation(payload) {
  const output = Array.isArray(payload?.output) ? payload.output : [];
  const text = output.flatMap((item) => Array.isArray(item?.content) ? item.content : [])
    .filter((part) => ["output_text", "reasoning_text"].includes(part?.type) && typeof part.text === "string")
    .map((part) => part.text)
    .join("");
  return {
    responseId: typeof payload?.id === "string" ? payload.id : null,
    status: payload?.status ?? null,
    outputTypes: output.map((item) => item?.type ?? "unknown"),
    toolNames: output.filter((item) => ["function_call", "custom_tool_call"].includes(item?.type)).map((item) => item?.name ?? null),
    toolCalls: output.filter((item) => ["function_call", "custom_tool_call"].includes(item?.type)).map((item) => ({
      callId: item?.call_id ?? item?.id ?? null,
      name: item?.name ?? null,
      namespace: item?.namespace ?? null,
      argumentValueDigests: (() => {
        if (item?.type !== "function_call" || typeof item.arguments !== "string") return {};
        try {
          const values = JSON.parse(item.arguments);
          if (!values || typeof values !== "object" || Array.isArray(values)) return {};
          return Object.fromEntries(Object.entries(values).map(([key, value]) => [key, createHash("sha256").update(JSON.stringify(value)).digest("hex")]));
        } catch {
          return {};
        }
      })(),
    })),
    textCharacters: text.length,
    finalTextDigest: output.some((item) => ["function_call", "custom_tool_call"].includes(item.type)) ? null : textDigest(text.trim()),
    textLooksLikeToolCall: /(?:DSML|<[^>]*calls|invoke\s+name=|tool[_ ]?call)/i.test(text),
  };
}

async function awaitResponseBarrier(policy, observation, routeIdentity) {
  if (!policy || routeIdentity.parentThreadId || !(observation.toolNames ?? []).includes(policy.rootTool)) return;
  const deadline = Date.now() + Math.max(1, policy.timeoutMs ?? 10_000);
  while (Date.now() < deadline) {
    if (existsSync(policy.releaseFile)) return;
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  throw Object.assign(new Error(`tool response barrier timed out: ${policy.rootTool}`), { statusCode: 504 });
}

function observedTools(tools) {
  if (!Array.isArray(tools)) return [];
  return tools.flatMap((tool) => {
    if (tool?.type === "namespace" && typeof tool.name === "string" && Array.isArray(tool.tools)) {
      return tool.tools.map((nested) => ({ type: nested?.type ?? null, name: nested?.name ?? null, namespace: tool.name }));
    }
    return [{ type: tool?.type ?? null, name: tool?.name ?? tool?.function?.name ?? null, namespace: tool?.namespace ?? null }];
  });
}

async function downstreamResponse(outbound, signal, fetchImpl, maxBytes, timeoutMs) {
  const response = await fetchImpl(outbound.url, {
    method: "POST",
    headers: outbound.headers,
    body: JSON.stringify(outbound.body),
    signal,
    maxBytes,
    timeoutMs,
  });
  if (response.ok) return { response, context: outbound.context ?? null };
  const text = await readDownstreamText(response.body, response.headers, response.status, maxBytes);
  let payload;
  try {
    payload = text ? JSON.parse(text) : {};
  } catch {
    throw Object.assign(new Error(`downstream returned non-JSON (${response.status})`), { statusCode: 502 });
  }
  const message = payload?.error?.message ?? payload?.message ?? `downstream request failed (${response.status})`;
  const marker = `${payload?.error?.type ?? ""} ${payload?.error?.code ?? ""} ${payload?.code ?? ""}`.toLowerCase();
  const authenticationFailure = response.status === 401 || (response.status === 403 && /(auth|api.?key|credential|unauthorized)/.test(marker));
  throw Object.assign(new Error(message), { statusCode: response.status >= 400 && response.status < 500 ? response.status : 502, downstream: payload, authenticationFailure });
}

function trustedRouteIdentity(request) {
  const value = request.headers["x-codex-turn-metadata"];
  if (typeof value !== "string" || value.length > 16_384) throw Object.assign(new Error("trusted Codex turn metadata is required"), { statusCode: 403 });
  let metadata;
  try {
    metadata = JSON.parse(value);
  } catch {
    throw Object.assign(new Error("trusted Codex turn metadata is invalid"), { statusCode: 403 });
  }
  if (typeof metadata.thread_id !== "string" || !metadata.thread_id) throw Object.assign(new Error("trusted Codex thread identity is required"), { statusCode: 403 });
  if (metadata.parent_thread_id != null && (typeof metadata.parent_thread_id !== "string" || !metadata.parent_thread_id)) throw Object.assign(new Error("trusted Codex parent identity is invalid"), { statusCode: 403 });
  for (const field of ["turn_id", "root_turn_id", "session_id"]) {
    if (metadata[field] != null && (typeof metadata[field] !== "string" || !metadata[field])) throw Object.assign(new Error(`trusted Codex ${field} is invalid`), { statusCode: 403 });
  }
  return {
    threadId: metadata.thread_id,
    parentThreadId: metadata.parent_thread_id ?? null,
    turnId: metadata.turn_id ?? null,
    rootTurnId: metadata.root_turn_id ?? null,
    sessionId: metadata.session_id ?? null,
  };
}

async function downstreamJson(codec, response, maxBytes, context) {
  const text = await readDownstreamText(response.body, response.headers, response.status, maxBytes);
  let payload;
  try {
    payload = text ? JSON.parse(text) : {};
  } catch {
    throw Object.assign(new Error(`downstream returned non-JSON (${response.status})`), { statusCode: 502 });
  }
  return codec.parseResponse(payload, context);
}

async function readDownstreamText(body, headers, status, maxBytes = DEFAULT_MAX_DOWNSTREAM_BYTES) {
  const declaredLength = Number(headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > maxBytes) {
    throw Object.assign(new Error(`downstream response exceeds bridge limit (${status})`), { statusCode: 502 });
  }
  const chunks = [];
  let bytes = 0;
  for await (const chunk of body ?? []) {
    bytes += chunk.byteLength;
    if (bytes > maxBytes) throw Object.assign(new Error(`downstream response exceeds bridge limit (${status})`), { statusCode: 502 });
    chunks.push(Buffer.from(chunk));
  }
  return Buffer.concat(chunks).toString("utf8");
}

async function* limitedDownstreamBody(body, headers, maxBytes) {
  const declaredLength = Number(headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > maxBytes) {
    throw Object.assign(new Error("downstream response exceeds bridge limit"), { statusCode: 502 });
  }
  let bytes = 0;
  for await (const chunk of body ?? []) {
    bytes += chunk.byteLength;
    if (bytes > maxBytes) throw Object.assign(new Error("downstream response exceeds bridge limit"), { statusCode: 502 });
    yield chunk;
  }
}

export class ModelBridge {
  constructor({ sessionPlan, connections, credentialResolver, fetchImpl = nativeFetch, maxBodyBytes = DEFAULT_MAX_BODY_BYTES, maxDownstreamBytes = DEFAULT_MAX_DOWNSTREAM_BYTES, maxConcurrentRequests = DEFAULT_MAX_CONCURRENT_REQUESTS, requestTimeoutMs = DEFAULT_REQUEST_TIMEOUT_MS, token = randomBytes(32).toString("base64url"), port = 0, budget = null, requestObserver = null, responseObserver = null, qualificationObserver = null, toolPolicy = null, responseBarrier = null }) {
    this.sessionPlan = sessionPlan;
    this.connections = new Map(connections.map((connection) => [connection.id, connection]));
    this.credentialResolver = credentialResolver;
    this.fetchImpl = fetchImpl;
    this.maxBodyBytes = maxBodyBytes;
    this.maxDownstreamBytes = maxDownstreamBytes;
    this.maxConcurrentRequests = maxConcurrentRequests;
    this.requestTimeoutMs = requestTimeoutMs;
    this.token = token;
    this.port = port;
    this.budget = budget;
    this.budgetTimer = null;
    this.lease = null;
    this.leaseEpoch = 0;
    this.routeLineage = null;
    this.requestObserver = requestObserver;
    this.responseObserver = responseObserver;
    this.qualificationObserver = qualificationObserver;
    this.toolPolicy = toolPolicy;
    this.responseBarrier = responseBarrier;
    this.toolPolicyChildRequests = new Map();
    this.server = null;
    this.baseUrl = null;
    this.controllers = new Set();
    this.requestSettlements = new Map();
  }

  async start() {
    if (this.server) return { baseUrl: this.baseUrl, token: this.token };
    const server = createServer((request, response) => void this.#handle(request, response));
    try {
      await new Promise((resolve, reject) => {
        server.once("error", reject);
        server.listen(this.port, "127.0.0.1", resolve);
      });
    } catch (error) {
      server.close();
      throw error;
    }
    this.server = server;
    const address = server.address();
    this.baseUrl = `http://127.0.0.1:${address.port}`;
    return { baseUrl: this.baseUrl, token: this.token };
  }

  async close() {
    await this.cancelActiveRequests();
    if (this.budgetTimer) clearTimeout(this.budgetTimer);
    this.budgetTimer = null;
    this.budget = null;
    this.lease = null;
    this.routeLineage = null;
    const server = this.server;
    this.server = null;
    this.baseUrl = null;
    if (server) await new Promise((resolve) => server.close(resolve));
  }

  async cancelActiveRequests({ revokeLease = true } = {}) {
    if (revokeLease && this.lease) this.leaseEpoch += 1;
    const count = this.controllers.size;
    for (const controller of this.controllers) controller.abort();
    await Promise.all([...this.requestSettlements.values()]);
    return count;
  }

  revokeExecution(exceptController = null) {
    if (this.lease) this.leaseEpoch += 1;
    for (const controller of this.controllers) if (controller !== exceptController) controller.abort();
  }

  beginTaskBudget(budget) {
    if (!budget || typeof budget.reserveRequest !== "function" || typeof budget.observeUsage !== "function") throw new Error("invalid task budget");
    if (this.budget) throw new Error("a task budget is already active");
    this.budget = budget;
    this.budgetTimer = setTimeout(() => this.cancelActiveRequests(), budget.remainingDurationMs());
    this.budgetTimer.unref?.();
    return budget;
  }

  endTaskBudget(budget) {
    if (this.budget !== budget) return false;
    if (this.budgetTimer) clearTimeout(this.budgetTimer);
    this.budgetTimer = null;
    this.budget = null;
    return true;
  }

  beginExecutionLease(lease) {
    if (this.lease) throw new Error("an execution lease is already active");
    assertExecutionLease(lease, this.sessionPlan, { epoch: lease.epoch });
    this.lease = lease;
    this.leaseEpoch = lease.epoch;
    this.routeLineage = new RouteLineage(lease);
    return lease;
  }

  async endExecutionLease(lease) {
    if (this.lease?.leaseId !== lease?.leaseId) return false;
    await this.cancelActiveRequests();
    this.lease = null;
    this.routeLineage = null;
    return true;
  }

  async #handle(request, response) {
    if (request.method !== "POST" || !["/responses", "/v1/responses"].includes(new URL(request.url, "http://localhost").pathname)) {
      writeJson(response, 404, { error: { message: "route not found", type: "invalid_request_error" } });
      return;
    }
    if (request.headers.authorization !== `Bearer ${this.token}`) {
      writeJson(response, 401, { error: { message: "invalid bridge capability", type: "authentication_error" } });
      return;
    }
    if (this.controllers.size >= this.maxConcurrentRequests) {
      writeJson(response, 429, { error: { message: "bridge concurrency limit reached", type: "rate_limit_error" } });
      return;
    }
    const controller = new AbortController();
    let timedOut = false;
    const timeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, this.requestTimeoutMs);
    this.controllers.add(controller);
    let settleRequest;
    this.requestSettlements.set(controller, new Promise((resolve) => { settleRequest = resolve; }));
    request.once("aborted", () => controller.abort());
    response.once("close", () => {
      if (!response.writableEnded) controller.abort();
    });
    const taskBudget = this.budget;
    const executionLease = this.lease;
    let reservationId = null;
    let downstreamStarted = false;
    let reservationSettled = false;
    let activeRoute = null;
    let observationContext = null;
    try {
      const body = await readBody(request, this.maxBodyBytes, controller.signal);
      if (!taskBudget || !executionLease || taskBudget.id !== executionLease.budgetId) throw Object.assign(new Error("an active execution lease and matching task budget are required"), { statusCode: 403 });
      assertActiveExecution(executionLease, this.sessionPlan, this.leaseEpoch);
      const logicalModel = typeof body.model === "string" && body.model ? body.model : this.sessionPlan.defaultModel;
      const route = this.sessionPlan.routes.find((candidate) => candidate.logicalModel === logicalModel);
      if (!route) throw Object.assign(new Error(`model route is not authorized: ${logicalModel}`), { statusCode: 403 });
      activeRoute = route;
      const routeIdentity = trustedRouteIdentity(request);
      try {
        this.routeLineage.authorize({ routeId: route.connectionId, threadId: routeIdentity.threadId, parentThreadId: routeIdentity.parentThreadId });
      } catch (error) {
        throw Object.assign(error, { statusCode: 403 });
      }
      const connection = this.connections.get(route.connectionId);
      if (!connection) throw Object.assign(new Error(`connection is unavailable: ${route.connectionId}`), { statusCode: 503 });
      const apiKey = await this.credentialResolver(connection.credentialRef);
      if (typeof apiKey !== "string" || !apiKey) throw Object.assign(new Error(`credential is unavailable: ${connection.credentialRef}`), { statusCode: 503 });
      const maximumOutput = Math.min(
        Number.isInteger(body.max_output_tokens) && body.max_output_tokens > 0 ? body.max_output_tokens : route.outputBudget,
        route.outputBudget,
      );
      if (this.toolPolicy) {
        let allowedNames = this.toolPolicy.rootAllowed ?? [];
        let childComplete = false;
        if (routeIdentity.parentThreadId) {
          childComplete = Boolean(this.toolPolicy.childCompletionFile && existsSync(this.toolPolicy.childCompletionFile));
          if (childComplete) allowedNames = [];
          else if (this.toolPolicy.childPhaseFile) {
            allowedNames = existsSync(this.toolPolicy.childPhaseFile)
              ? this.toolPolicy.childAfterPhaseAllowed ?? this.toolPolicy.childAllowed ?? []
              : this.toolPolicy.childBeforePhaseAllowed ?? this.toolPolicy.childAllowed ?? [];
          } else {
            const requestCount = this.toolPolicyChildRequests.get(routeIdentity.threadId) ?? 0;
            allowedNames = requestCount === 0
              ? this.toolPolicy.childFirstAllowed ?? this.toolPolicy.childAllowed ?? []
              : this.toolPolicy.childSubsequentAllowed ?? this.toolPolicy.childAllowed ?? [];
            this.toolPolicyChildRequests.set(routeIdentity.threadId, requestCount + 1);
          }
        }
        const allowed = new Set(allowedNames);
        body.tools = (body.tools ?? []).flatMap((tool) => {
          if (tool?.type !== "namespace") return allowed.has(tool?.name) ? [tool] : [];
          const nested = (tool.tools ?? []).filter((entry) => allowed.has(`${tool.name}.${entry?.name}`) || allowed.has(entry?.name));
          return nested.length ? [{ ...tool, tools: nested }] : [];
        });
        if (routeIdentity.parentThreadId && !childComplete && this.toolPolicy.childToolChoiceRequired && body.tools.length > 0) body.tool_choice = "required";
        if (childComplete) body.tool_choice = "none";
        if (body.tool_choice && typeof body.tool_choice === "object") {
          const selected = body.tool_choice.namespace ? `${body.tool_choice.namespace}.${body.tool_choice.name}` : body.tool_choice.name;
          if (!allowed.has(selected) && !allowed.has(body.tool_choice.name)) throw Object.assign(new Error(`required tool is outside the execution tool policy: ${selected}`), { statusCode: 403 });
        }
      }
      body.max_output_tokens = maximumOutput;
      const codec = getCodec(route.protocol);
      const prepared = codec.createRequest(body, connection, apiKey);
      observationContext = {
        runId: executionLease.runId,
        requestId: prepared.context?.requestId ?? null,
        connectionId: route.connectionId,
        logicalModel,
        ...routeIdentity,
      };
      await observe(this.requestObserver, {
        ...observationContext,
        model: typeof body.model === "string" ? body.model : null,
        stream: body.stream !== false,
        toolChoice: body.tool_choice ?? null,
        tools: observedTools(body.tools),
        encodedTools: observedTools(prepared.body?.tools),
        encodedToolChoice: prepared.body?.tool_choice ?? null,
        visibleTextDigests: visibleTextDigests(prepared.body),
        waitResults: waitResultFacts(body.input),
        finalAgentMessages: finalAgentMessageFacts(body.input),
        inputItemTypes: Array.isArray(body.input) ? body.input.map((item) => item?.type ?? "unknown") : [],
        toolResultCallIds: Array.isArray(body.input) ? body.input.filter((item) => item?.type === "function_call_output").map((item) => item?.call_id ?? null).filter(Boolean) : [],
      });
      const providerStreaming = prepared.body?.stream !== false && typeof codec.parseStream === "function";
      if (providerStreaming && !route.enabledExtensions.includes(CAPABILITIES.STREAMING)) {
        throw Object.assign(new Error(`streaming is not qualified for model route: ${logicalModel}`), { statusCode: 403 });
      }
      const estimatedInputTokens = prepared.context?.estimatedInputTokens;
      if (!Number.isInteger(estimatedInputTokens) || estimatedInputTokens < 0) throw Object.assign(new Error("codec did not produce a bounded input estimate"), { statusCode: 500 });
      if (estimatedInputTokens > route.inputBudget) throw Object.assign(new Error("encoded request exceeds the route input budget"), { statusCode: 413 });
      reservationId = await taskBudget.reserveRequest({ estimatedInputTokens, maxOutputTokens: maximumOutput });
      assertActiveExecution(executionLease, this.sessionPlan, this.leaseEpoch);
      downstreamStarted = true;
      const outbound = await downstreamResponse(prepared, controller.signal, this.fetchImpl, this.maxDownstreamBytes, this.requestTimeoutMs);
      const downstream = outbound.response;
      const contentType = downstream.headers.get("content-type") ?? "";
      if (body.stream !== false && typeof codec.parseStream === "function" && contentType.includes("text/event-stream")) {
        const bufferedEvents = this.responseBarrier ? [] : null;
        if (!bufferedEvents) response.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-store", connection: "keep-alive" });
        let terminalObservation = null;
        for await (const event of codec.parseStream(limitedDownstreamBody(downstream.body, downstream.headers, this.maxDownstreamBytes), outbound.context)) {
          if (["response.completed", "response.incomplete", "response.failed"].includes(event.type) && reservationId && !reservationSettled) {
            await taskBudget.settleReservation(reservationId, event.response?.usage);
            reservationSettled = true;
            terminalObservation = { ...observationContext, ...responseObservation(event.response) };
            if (!bufferedEvents) await observe(this.responseObserver, terminalObservation);
          }
          if (bufferedEvents) bufferedEvents.push(event);
          else await writeFrame(response, event, controller.signal);
        }
        if (bufferedEvents) {
          if (terminalObservation) {
            await awaitResponseBarrier(this.responseBarrier, terminalObservation, routeIdentity);
            await observe(this.responseObserver, terminalObservation);
          }
          response.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-store", connection: "keep-alive" });
          for (const event of bufferedEvents) await writeFrame(response, event, controller.signal);
        }
        response.end("data: [DONE]\n\n");
        return;
      }
      const canonical = await downstreamJson(codec, downstream, this.maxDownstreamBytes, outbound.context);
      const canonicalObservation = { ...observationContext, ...responseObservation(canonical) };
      await awaitResponseBarrier(this.responseBarrier, canonicalObservation, routeIdentity);
      await observe(this.responseObserver, canonicalObservation);
      if (reservationId) {
        await taskBudget.settleReservation(reservationId, canonical.usage);
        reservationSettled = true;
      }
      if (body.stream === false) {
        writeJson(response, 200, canonical);
      } else {
        response.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-store", connection: "keep-alive" });
        for (const event of responseEvents(canonical)) await writeFrame(response, event, controller.signal);
        response.end("data: [DONE]\n\n");
      }
    } catch (error) {
      if (observationContext) {
        await observe(this.responseObserver, {
          ...observationContext,
          status: "failed",
          errorCategory: error.authenticationFailure ? "authentication" : controller.signal.aborted ? timedOut ? "timeout" : "cancelled" : "bridge",
          errorMessage: String(error?.message ?? "bridge request failed").slice(0, 500),
          errorStatusCode: Number.isInteger(error?.statusCode) ? error.statusCode : null,
          outputTypes: [],
          toolNames: [],
          toolCalls: [],
          textCharacters: 0,
          textLooksLikeToolCall: false,
        });
      }
      if (error.authenticationFailure && activeRoute) {
        this.revokeExecution(controller);
        await this.qualificationObserver?.({
          connectionId: activeRoute.connectionId,
          credentialRef: activeRoute.credentialRef,
          environmentDigest: activeRoute.environmentDigest,
          protocol: activeRoute.protocol,
          statusCode: error.statusCode,
        });
      }
      if (controller.signal.aborted) {
        if (!response.headersSent) {
          response.once("finish", () => request.destroy());
          writeJson(response, timedOut ? 504 : 499, { error: { message: timedOut ? "downstream request timed out" : "request cancelled", type: timedOut ? "timeout" : "cancelled" } });
        }
        else response.destroy();
      } else if (!response.headersSent) {
        writeJson(response, error.statusCode ?? 502, { error: { message: error.message, type: "bridge_error", downstream: error.downstream ?? undefined } });
      } else {
        response.destroy(error);
      }
    } finally {
      try {
        if (reservationId && !reservationSettled) await taskBudget?.settleReservation(reservationId, null, { outcome: downstreamStarted ? "unknown" : "not-sent" });
      } finally {
        clearTimeout(timeout);
        this.controllers.delete(controller);
        this.requestSettlements.delete(controller);
        settleRequest();
      }
    }
  }
}

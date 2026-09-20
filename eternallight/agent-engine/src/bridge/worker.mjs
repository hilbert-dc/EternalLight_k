import { ModelBridge } from "./server.mjs";

let bridge = null;
let budget = null;
let lease = null;
let nextHostRequestId = 1;
const hostRequests = new Map();

function requestHost(method, params = {}) {
  const requestId = nextHostRequestId++;
  return new Promise((resolve, rejectPromise) => {
    hostRequests.set(requestId, { resolve, reject: rejectPromise });
    process.send({ kind: "host-request", requestId, method, params }, (error) => {
      if (!error) return;
      hostRequests.delete(requestId);
      rejectPromise(error);
    });
  });
}

class HostBudgetProxy {
  constructor(snapshot) {
    this.id = snapshot.id;
    this.startedAt = snapshot.startedAt;
    this.maxDurationMs = snapshot.maxDurationMs;
  }

  #request(method, params = {}) {
    return requestHost(method, params);
  }

  reserveRequest(params) { return this.#request("budget:reserve", params); }
  settleReservation(id, usage, options) { return this.#request("budget:settle", { id, usage, options }); }
  observeUsage(usage) { return this.#request("budget:observe", { usage }); }
  remainingDurationMs(now = Date.now()) { return Math.max(0, this.startedAt + this.maxDurationMs - now); }
}

function reply(id, result) {
  if (process.connected) process.send({ id, result });
}

function reject(id, error) {
  if (process.connected) process.send({ id, error: { message: error instanceof Error ? error.message : String(error), code: error?.code ?? null } });
}

async function handle(method, params) {
  if (method === "start") {
    if (bridge) throw new Error("bridge is already started");
    const credentials = params.credentials ?? {};
    bridge = new ModelBridge({
      sessionPlan: params.sessionPlan,
      connections: params.connections,
      credentialResolver: async (reference) => credentials[reference] ?? null,
      qualificationObserver: (failure) => requestHost("qualification:failure", failure),
      requestObserver: (request) => requestHost("request:observed", request),
      responseObserver: (response) => requestHost("response:observed", response),
      toolPolicy: params.toolPolicy ?? null,
      responseBarrier: params.responseBarrier ?? null,
      ...(params.token ? { token: params.token } : {}),
      ...(Number.isInteger(params.port) ? { port: params.port } : {}),
    });
    return bridge.start();
  }
  if (!bridge) throw new Error("bridge is not started");
  if (method === "ping") return { alive: true };
  if (method === "cancel") return bridge.cancelActiveRequests();
  if (method === "budget:start") {
    budget = new HostBudgetProxy(params);
    bridge.beginTaskBudget(budget);
    return { id: budget.id };
  }
  if (method === "lease:start") {
    lease = Object.freeze({ ...params, routeGrants: Object.freeze((params.routeGrants ?? []).map((edge) => Object.freeze({ ...edge }))) });
    bridge.beginExecutionLease(lease);
    return lease;
  }
  if (method === "lease:end") {
    const ended = await bridge.endExecutionLease(lease);
    lease = null;
    return { ended };
  }
  if (method === "budget:end") {
    const ended = bridge.endTaskBudget(budget);
    budget = null;
    lease = null;
    return { ended };
  }
  if (method === "close") {
    await bridge.close();
    bridge = null;
    budget = null;
    return { closed: true };
  }
  throw new Error(`unknown bridge control method: ${method}`);
}

process.on("message", (message) => {
  if (message?.kind === "host-response") {
    const pending = hostRequests.get(message.requestId);
    if (!pending) return;
    hostRequests.delete(message.requestId);
    if (message.error) pending.reject(new Error(message.error.message));
    else pending.resolve(message.result);
    return;
  }
  void handle(message?.method, message?.params ?? {}).then((result) => {
    reply(message?.id, result);
    if (message?.method === "close") setImmediate(() => process.exit(0));
  }, (error) => reject(message?.id, error));
});

process.on("disconnect", () => {
  void Promise.resolve(bridge?.close()).finally(() => process.exit(0));
});

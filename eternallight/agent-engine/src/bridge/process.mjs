import { fork } from "node:child_process";
import { existsSync } from "node:fs";
import { sep } from "node:path";
import { fileURLToPath } from "node:url";

function workerEnvironment(source = process.env) {
  const allowed = ["PATH", "SystemRoot", "WINDIR", "TMP", "TEMP", "TMPDIR", "SSL_CERT_FILE", "NODE_EXTRA_CA_CERTS"];
  return Object.fromEntries(allowed.flatMap((name) => typeof source[name] === "string" ? [[name, source[name]]] : []));
}

function bridgeWorkerPath() {
  const source = fileURLToPath(new URL("./worker.mjs", import.meta.url));
  const unpacked = source.replace(`${sep}app.asar${sep}`, `${sep}app.asar.unpacked${sep}`);
  return unpacked !== source && existsSync(unpacked) ? unpacked : source;
}

export class ModelBridgeProcess {
  constructor({ sessionPlan, connections, credentialResolver, qualificationObserver = null, requestObserver = null, responseObserver = null, token = null, toolPolicy = null, responseBarrier = null, heartbeatIntervalMs = 5_000, heartbeatTimeoutMs = 2_000, lifecycleTimeoutMs = 10_000, workerPath = bridgeWorkerPath(), forkImpl = fork }) {
    this.sessionPlan = sessionPlan;
    this.connections = connections;
    this.credentialResolver = credentialResolver;
    this.qualificationObserver = qualificationObserver;
    this.requestObserver = requestObserver;
    this.responseObserver = responseObserver;
    this.token = token;
    this.toolPolicy = toolPolicy;
    this.responseBarrier = responseBarrier;
    this.port = null;
    this.child = null;
    this.baseUrl = null;
    this.nextId = 1;
    this.pending = new Map();
    this.budget = null;
    this.lease = null;
    this.budgetInvalidated = false;
    this.heartbeatIntervalMs = heartbeatIntervalMs;
    this.heartbeatTimeoutMs = heartbeatTimeoutMs;
    this.lifecycleTimeoutMs = lifecycleTimeoutMs;
    this.workerPath = workerPath;
    this.forkImpl = forkImpl;
    this.heartbeatTimer = null;
    this.heartbeatInFlight = false;
  }

  get processId() {
    return this.child?.pid ?? null;
  }

  async start() {
    if (this.child) return { baseUrl: this.baseUrl, token: this.token };
    if (this.budgetInvalidated) throw new Error("bridge task budget outcome is unknown; terminate the task before restarting");
    const credentials = {};
    for (const connection of this.connections) {
      if (!Object.hasOwn(credentials, connection.credentialRef)) credentials[connection.credentialRef] = await this.credentialResolver(connection.credentialRef);
    }
    const child = this.forkImpl(this.workerPath, [], {
      env: { ...workerEnvironment(), ELECTRON_RUN_AS_NODE: "1" },
      stdio: ["ignore", "ignore", "ignore", "ipc"],
      serialization: "json",
    });
    this.child = child;
    child.on("message", (message) => this.#onMessage(message));
    child.once("exit", (code, signal) => this.#onExit(child, code, signal));
    child.once("error", (error) => {
      if (this.child === child) this.#rejectAll(error);
    });
    let started;
    try {
      started = await this.#requestWithTimeout("start", { sessionPlan: this.sessionPlan, connections: this.connections, credentials, token: this.token, toolPolicy: this.toolPolicy, responseBarrier: this.responseBarrier, port: this.port });
    } catch (error) {
      if (child.exitCode === null) child.kill();
      if (this.child === child) this.child = null;
      this.#rejectAll(error);
      throw error;
    }
    this.baseUrl = started.baseUrl;
    this.token = started.token;
    this.port = Number(new URL(started.baseUrl).port);
    this.#startHeartbeat();
    return started;
  }

  async close() {
    const child = this.child;
    if (!child) return;
    this.#stopHeartbeat();
    try {
      await this.#requestWithTimeout("close", {});
    } catch (error) {
      if (child.exitCode === null) child.kill();
      throw error;
    } finally {
      this.child = null;
      this.baseUrl = null;
      this.budget = null;
      this.lease = null;
      this.budgetInvalidated = false;
      if (child.connected) child.disconnect();
    }
  }

  cancelActiveRequests() {
    if (!this.child) return Promise.resolve(0);
    return this.#request("cancel", {});
  }

  async beginTaskBudget(budget) {
    if (!budget || typeof budget.snapshot !== "function") throw new Error("invalid task budget");
    if (this.budget) throw new Error("a task budget is already active");
    this.budget = budget;
    this.budgetInvalidated = false;
    try {
      await this.#request("budget:start", { ...budget.snapshot(), startedAt: budget.startedAt });
      return budget;
    } catch (error) {
      this.budget = null;
      throw error;
    }
  }

  async beginExecutionLease(lease) {
    if (this.lease) throw new Error("an execution lease is already active");
    const started = await this.#request("lease:start", lease);
    this.lease = started;
    return started;
  }

  async endExecutionLease(lease) {
    if (this.lease?.leaseId !== lease?.leaseId) return false;
    try {
      const result = await this.#request("lease:end", {});
      return result.ended;
    } finally {
      this.lease = null;
    }
  }

  async endTaskBudget(budget) {
    if (this.budget !== budget) return false;
    if (this.budgetInvalidated) {
      await budget.markOutstandingUnknown();
      await budget.complete?.("recovered_unknown");
      this.budget = null;
      this.budgetInvalidated = false;
      return false;
    }
    const result = await this.#request("budget:end", {});
    await budget.complete?.("terminal");
    this.budget = null;
    return result.ended;
  }

  #request(method, params) {
    if (!this.child?.connected) return Promise.reject(new Error("bridge process is not running"));
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.child.send({ id, method, params }, (error) => {
        if (!error) return;
        this.pending.delete(id);
        reject(error);
      });
    });
  }

  #requestWithTimeout(method, params) {
    let timeout;
    return Promise.race([
      this.#request(method, params),
      new Promise((_, reject) => {
        timeout = setTimeout(() => reject(new Error(`bridge ${method} timed out`)), this.lifecycleTimeoutMs);
      }),
    ]).finally(() => clearTimeout(timeout));
  }

  #onMessage(message) {
    if (message?.kind === "host-request") {
      void this.#handleHostRequest(message);
      return;
    }
    const waiter = this.pending.get(message?.id);
    if (!waiter) return;
    this.pending.delete(message.id);
    if (message.error) waiter.reject(Object.assign(new Error(message.error.message), { code: message.error.code }));
    else waiter.resolve(message.result);
  }

  async #handleHostRequest(message) {
    const child = this.child;
    const reply = (result = null, error = null) => {
      if (this.child === child && child?.connected) child.send({
        kind: "host-response",
        requestId: message.requestId,
        ...(error ? { error: { message: error instanceof Error ? error.message : String(error) } } : { result }),
      });
    };
    try {
      if (message.method === "qualification:failure") {
        this.lease = null;
        reply(await this.qualificationObserver?.(message.params) ?? { recorded: false });
        return;
      }
      if (message.method === "request:observed") {
        reply(await this.requestObserver?.(message.params) ?? { recorded: false });
        return;
      }
      if (message.method === "response:observed") {
        reply(await this.responseObserver?.(message.params) ?? { recorded: false });
        return;
      }
      if (!this.budget) throw new Error("host task budget is not active");
      if (message.method === "budget:reserve") reply(await this.budget.reserveRequest(message.params));
      else if (message.method === "budget:settle") reply(await this.budget.settleReservation(message.params.id, message.params.usage, message.params.options));
      else if (message.method === "budget:observe") reply(await this.budget.observeUsage(message.params.usage));
      else throw new Error(`unknown host budget method: ${message.method}`);
    } catch (error) {
      reply(null, error);
    }
  }

  #onExit(child, code, signal) {
    if (this.child !== child) return;
    const error = new Error(`bridge process exited (${code ?? signal ?? "unknown"})`);
    this.#stopHeartbeat();
    this.child = null;
    this.baseUrl = null;
    if (this.budget) {
      this.budgetInvalidated = true;
      this.budget.markOutstandingUnknown();
    }
    this.#rejectAll(error);
  }

  #rejectAll(error) {
    for (const waiter of this.pending.values()) waiter.reject(error);
    this.pending.clear();
  }

  #startHeartbeat() {
    this.#stopHeartbeat();
    if (!Number.isFinite(this.heartbeatIntervalMs) || this.heartbeatIntervalMs <= 0) return;
    this.heartbeatTimer = setInterval(() => void this.#heartbeat(), this.heartbeatIntervalMs);
    this.heartbeatTimer.unref();
  }

  #stopHeartbeat() {
    if (this.heartbeatTimer) clearInterval(this.heartbeatTimer);
    this.heartbeatTimer = null;
  }

  async #heartbeat() {
    const child = this.child;
    if (!child || this.heartbeatInFlight) return;
    this.heartbeatInFlight = true;
    let timeout;
    try {
      await Promise.race([
        this.#request("ping", {}),
        new Promise((_, reject) => {
          timeout = setTimeout(() => reject(new Error("bridge heartbeat timed out")), this.heartbeatTimeoutMs);
        }),
      ]);
    } catch {
      if (this.child === child && child.exitCode === null) child.kill();
    } finally {
      clearTimeout(timeout);
      this.heartbeatInFlight = false;
    }
  }
}

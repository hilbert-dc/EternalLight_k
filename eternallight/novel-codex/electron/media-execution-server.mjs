// @ts-check
import { createServer } from "node:http";
import { randomBytes, timingSafeEqual } from "node:crypto";
import { MediaServiceHost } from "./media-service-host.mjs";
import { importMediaBytes } from "./media-files.mjs";

const validId = value => typeof value === "string" && value.length > 0 && value.length <= 160 && !/[\p{Cc}/\\]/u.test(value);
const terminal = new Set(["ready", "failed", "cancelled", "outcome_unknown"]);

/** Internal Runtime-to-Host boundary. No renderer API or polling scheduler. */
export async function createMediaExecutionServer({ runtimeRequest, multimediaHost, databasePath, getExecution }) {
  const secret = randomBytes(32).toString("base64url"), operationCalls = new Map(), pendingCancel = new Set(), shutdown = new AbortController();
  const operationPath = record => {
    if (!validId(record.projectId) || !validId(record.id)) throw Error("媒体操作编号无效");
    return `/api/projects/${encodeURIComponent(record.projectId)}/media-operations/${encodeURIComponent(record.id)}`;
  };
  const read = record => runtimeRequest(operationPath(record));
  const service = new MediaServiceHost({ databasePath, connections: multimediaHost.connections,
    registerIntent: async ({ operation, grant }) => {
      const intent = await read(operation);
      if (intent.operation.id !== operation.id || intent.operation.requestDigest !== operation.requestDigest
        || intent.grant.id !== grant.id || intent.operation.attemptId !== operation.attemptId) throw Error("领域和引擎媒体操作绑定不一致");
    },
    authorizeOperation: async (record, { phase }) => {
      const intent = await read(record);
      if (intent.operation.attemptId !== record.attemptId || intent.operation.scopeId !== record.scopeId
        || intent.operation.branchId !== record.branchId || intent.operation.workTaskId !== record.workTaskId) throw Error("媒体操作已失去原任务绑定");
      if (["prepare", "dispatch"].includes(phase) && !intent.authorization.canDispatch) throw Error("媒体任务已停止或授权已变化，尚未提交新的生成请求");
      if (!["prepare", "dispatch"].includes(phase) && !intent.authorization.canReconcile) throw Error("媒体结果当前不可核对");
    },
    loadAsset: async (record, assetRevisionId) => {
      const intent = await read(record);
      if (intent.operation.requestDigest !== record.requestDigest
        || !Array.isArray(intent.operation.source?.assetRevisionIds)
        || !intent.operation.source.assetRevisionIds.includes(assetRevisionId)) throw Error("素材不在原媒体操作授权的输入中");
      // Domain pins may include a referenced asset from another Edition in the
      // same project. The immutable operation pin authorizes that exact ID;
      // requiring its import scope to equal the target scope rejects valid reuse.
      const asset = await multimediaHost.assetBytes(assetRevisionId, record.projectId);
      return { name: asset.name, mimeType: asset.mimeType, bytes: asset.bytes };
    },
    importOutput: async (record, output) => {
      await runtimeRequest(`${operationPath(record)}/outputs`, { method: "POST", body: JSON.stringify({ outputCount: output.outputCount }) });
      if (output.kind === "transcript") return runtimeRequest(`${operationPath(record)}/transcript`, { method: "POST",
        body: JSON.stringify({ text: output.text, sourceAssetRevisionId: record.request.audioAssetRevisionId }) });
      const operationId = `${record.id}-output-${output.outputIndex}`, idempotencyKey = operationId;
      const route = multimediaHost.route(record.projectId, record.editionId);
      // /outputs already registered the entire batch's import pins. Registering
      // this pin again as a manual import would collide with the domain intent.
      const metadata = await importMediaBytes({ bytes: output.bytes, name: output.name, rootDirectory: multimediaHost.directory,
        decodeImage: multimediaHost.decodeImage, ffprobePath: multimediaHost.executables.ffprobePath });
      const { relativePath: _relativePath, ...asset } = metadata;
      const result = await runtimeRequest(`${route}/assets`, { method: "POST", body: JSON.stringify({ operationId, idempotencyKey, asset, sources: [] }) });
      return multimediaHost.rememberAsset(result);
    },
    sealOutputs: async (record, outputs) => {
      const result = await runtimeRequest(`${operationPath(record)}/seal`, { method: "POST", body: JSON.stringify({ state: "ready",
        outputs: { ...outputs, artifactRevisionId: outputs.artifactRevisionId ?? null }, providerReceipt: record.receipt ?? null }) });
      return result;
    },
    lookupOutputs: async record => {
      const intent = await read(record);
      if (!intent.seal || intent.state !== "ready") return null;
      return { assetRevisionIds: intent.seal.outputs.assetRevisionIds, receipt: intent.seal };
    },
    admitResource: async record => {
      const admission = service.ledger.admit(record, { globalLimit: 2, poolLimit: 1 });
      if (!admission.admitted) return admission;
      if (record.state === "prepared") {
        const execution = getExecution(record);
        if (!execution?.taskBudget || execution.task.id !== record.attemptId) {
          service.ledger.releaseAdmission(record);
          throw Error("媒体提交缺少原任务执行额度");
        }
        // Runtime media grant meters units; the original task also meters this
        // provider request. Reconciliation never grants another request.
        try {
          const reservation = await execution.taskBudget.reserveRequest({ reservationId: `media:${record.id}`, estimatedInputTokens: 0, maxOutputTokens: 0 });
          await execution.taskBudget.settleReservation(reservation, { total_tokens: 0 }, { outcome: "completed" });
        } catch (error) { service.ledger.releaseAdmission(record); throw error; }
      }
      return () => service.ledger.releaseAdmission(record);
    },
  });
  const execute = async body => {
    if (!validId(body.projectId) || !validId(body.operationId) || !["prepare", "read", "wait", "cancel"].includes(body.action)) throw Error("媒体执行请求无效");
    const identity = { projectId: body.projectId, id: body.operationId };
    const intent = await read(identity);
    let record = service.ledger.find(body.operationId);
    if (!record) {
      if (body.action === "cancel") {
        await runtimeRequest(`${operationPath(identity)}/seal`, { method: "POST", body: JSON.stringify({ state: "cancelled", outputs: null }) });
        return { state: "cancelled" };
      }
      record = await service.prepare({ task: intent.task, grant: intent.grant,
        request: { providerId: intent.operation.providerId, kind: intent.operation.kind,
          parameters: intent.operation.request, idempotencyKey: intent.operation.idempotencyKey },
        reservation: intent.operation.reservation, source: intent.operation.source, target: intent.operation.target,
        editionId: intent.operation.editionId, operationId: intent.operation.id });
    }
    if (body.action === "cancel" || pendingCancel.has(`${body.projectId}:${body.operationId}`)) await service.cancel(record.id);
    if (!terminal.has(record.state) || body.action === "wait" || body.action === "cancel") record = await service.step(record.id, { signal: shutdown.signal });
    if (["failed", "cancelled"].includes(record.state)) await runtimeRequest(`${operationPath(record)}/seal`, { method: "POST", body: JSON.stringify({ state: record.state,
      outputs: null, providerReceipt: record.receipt, error: record.errorCode }) });
    return { operationId: record.id, state: record.state, nextPollAt: record.nextPollAt,
      outputAssetRevisionIds: record.outputAssetRevisionIds, receipt: record.resultReceipt, error: record.errorCode };
  };
  const server = createServer(async (request, response) => {
    response.setHeader("Content-Type", "application/json; charset=utf-8");
    const supplied = request.headers.authorization ?? "", expected = `Bearer ${secret}`;
    if (request.method !== "POST" || request.url !== "/media/execute" || supplied.length !== expected.length
      || !timingSafeEqual(Buffer.from(supplied), Buffer.from(expected))) { response.writeHead(403); response.end(JSON.stringify({ error: "forbidden" })); return; }
    try {
      shutdown.signal.throwIfAborted();
      let bytes = 0; const chunks = [];
      for await (const chunk of request) { bytes += chunk.length; if (bytes > 4096) throw Error("媒体控制请求过大"); chunks.push(chunk); }
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      const key = `${body.projectId}:${body.operationId}`;
      if (body.action === "cancel" && validId(body.projectId) && validId(body.operationId)) pendingCancel.add(key);
      let call = operationCalls.get(key);
      if (!call) {
        call = execute(body); operationCalls.set(key, call);
        void call.finally(() => { if (operationCalls.get(key) === call) operationCalls.delete(key); pendingCancel.delete(key); }).catch(() => {});
      } else if (body.action === "cancel") {
        const record = service.ledger.find(body.operationId);
        if (record) await service.cancel(record.id);
      }
      const result = JSON.stringify(await call);
      response.writeHead(200, { "Content-Length": Buffer.byteLength(result) }); response.end(result);
    } catch (error) {
      const result = JSON.stringify({ error: error instanceof Error ? error.message : "媒体操作未完成" });
      response.writeHead(409, { "Content-Length": Buffer.byteLength(result) }); response.end(result);
    }
  });
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(0, "127.0.0.1", () => resolve()); });
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("媒体服务未绑定 TCP 地址");
  return { url: `http://127.0.0.1:${address.port}/media/execute`, token: secret, service,
    close: () => new Promise((resolve, reject) => { shutdown.abort(); server.close(error => error ? reject(error) : resolve()); server.closeIdleConnections(); }) };
}

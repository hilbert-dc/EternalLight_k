// @ts-check
import { randomUUID } from "node:crypto";

// Cross-object proposals, hypotheses and knowledge reads/saves forwarded to Native Runtime.
export function registerKnowledgeProposalHandlers(ipcMain, runtimeRequest) {
  ipcMain.handle("runtime:list-proposal-sets", (_event, projectId, cursor = null) => {
    if (typeof projectId !== "string" || !projectId.trim() || (cursor !== null && (typeof cursor !== "string" || !/^[1-9][0-9]{0,18}$/.test(cursor)))) throw new Error("提案列表范围无效");
    const suffix = cursor === null ? "" : `?cursor=${encodeURIComponent(cursor)}`;
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-sets${suffix}`);
  });
  ipcMain.handle("runtime:create-proposal-set", (_event, projectId, request) => {
    if (typeof projectId !== "string" || !projectId.trim() || !request || typeof request !== "object"
      || typeof request.baseSnapshotId !== "string" || !request.baseSnapshotId.trim()
      || typeof request.intent !== "string" || !request.intent.trim() || request.intent.length > 1000
      || !Array.isArray(request.operations) || request.operations.length < 2 || request.operations.length > 20
      || typeof request.idempotencyKey !== "string" || !request.idempotencyKey.trim()) throw new Error("跨对象提案参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-sets`, { method: "POST", body: JSON.stringify(request) });
  });
  ipcMain.handle("runtime:get-proposal-set", (_event, projectId, proposalSetId) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof proposalSetId !== "string" || !/^proposal-set-[a-f0-9]{32}$/.test(proposalSetId)) throw new Error("提案编号无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-sets/${proposalSetId}`);
  });
  ipcMain.handle("runtime:preview-proposal-set", (_event, projectId, proposalSetId, selectedOperationIds, selectionsByOperationId) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof proposalSetId !== "string" || !/^proposal-set-[a-f0-9]{32}$/.test(proposalSetId)
      || !Array.isArray(selectedOperationIds) || selectedOperationIds.length > 20 || selectedOperationIds.some((id) => typeof id !== "string" || !id.trim())
      || !selectionsByOperationId || typeof selectionsByOperationId !== "object" || Array.isArray(selectionsByOperationId)
      || JSON.stringify(selectionsByOperationId).length > 32_000) throw new Error("提案选择无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-sets/${proposalSetId}/preview`, { method: "POST", body: JSON.stringify({ selectedOperationIds, selectionsByOperationId }) });
  });
  ipcMain.handle("runtime:apply-proposal-set", (_event, projectId, proposalSetId, request) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof proposalSetId !== "string" || !/^proposal-set-[a-f0-9]{32}$/.test(proposalSetId)
      || !request || typeof request !== "object" || !Array.isArray(request.selectedOperationIds) || request.selectedOperationIds.length < 1 || request.selectedOperationIds.length > 20
      || !request.selectionsByOperationId || typeof request.selectionsByOperationId !== "object" || Array.isArray(request.selectionsByOperationId)
      || typeof request.idempotencyKey !== "string" || !request.idempotencyKey.trim() || JSON.stringify(request).length > 32_000) throw new Error("整组采用参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-sets/${proposalSetId}/apply`, { method: "POST", body: JSON.stringify(request) });
  });
  ipcMain.handle("runtime:undo-proposal-set", (_event, projectId, applicationId, expectedProjectRevision, idempotencyKey) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof applicationId !== "string" || !/^proposal-application-[a-f0-9]{32}$/.test(applicationId)
      || !Number.isInteger(expectedProjectRevision) || expectedProjectRevision < 0 || typeof idempotencyKey !== "string" || !idempotencyKey.trim()) throw new Error("整组撤销参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-applications/${applicationId}/undo`, { method: "POST", body: JSON.stringify({ expectedProjectRevision, idempotencyKey }) });
  });
  ipcMain.handle("runtime:get-impact-analysis", (_event, projectId, options) => {
    if (typeof projectId !== "string" || !projectId.trim() || !options || typeof options !== "object") throw new Error("影响分析目标无效");
    const { entityId, fromRevisionId = null, toRevisionId = null, maxHops = 3, limit = 25 } = options;
    if (typeof entityId !== "string" || !entityId.trim() || entityId.length > 160) throw new Error("影响分析实体无效");
    if ((fromRevisionId === null) !== (toRevisionId === null)
      || [fromRevisionId, toRevisionId].some((id) => id !== null && (typeof id !== "string" || !id.trim() || id.length > 160))) throw new Error("修订版本必须成对提供");
    if (!Number.isInteger(maxHops) || maxHops < 1 || maxHops > 3 || !Number.isInteger(limit) || limit < 1 || limit > 50) throw new Error("影响分析范围无效");
    const params = new URLSearchParams({ entityId: entityId.trim(), maxHops: String(maxHops), limit: String(limit) });
    if (fromRevisionId !== null) {
      params.set("fromRevisionId", fromRevisionId);
      params.set("toRevisionId", toRevisionId);
    }
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/impact-analysis?${params}`);
  });
  ipcMain.handle("runtime:get-historical-knowledge-state", (_event, projectId, snapshotId) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof snapshotId !== "string" || !snapshotId.trim() || snapshotId.length > 160) throw new Error("历史知识范围无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/knowledge/state?perspective=author&snapshotId=${encodeURIComponent(snapshotId)}`);
  });
  ipcMain.handle("runtime:create-hypothesis", (_event, projectId, request) => {
    if (typeof projectId !== "string" || !projectId.trim() || !request || typeof request !== "object"
      || typeof request.baseSnapshotId !== "string" || !request.baseSnapshotId.trim()
      || typeof request.intent !== "string" || !request.intent.trim() || request.intent.length > 1000
      || !Array.isArray(request.assumptions) || request.assumptions.length < 1 || request.assumptions.length > 25
      || typeof request.idempotencyKey !== "string" || !request.idempotencyKey.trim()) throw new Error("假设条件无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/hypotheses`, { method: "POST", body: JSON.stringify(request) });
  });
  ipcMain.handle("runtime:get-hypothesis", (_event, projectId, hypothesisId) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof hypothesisId !== "string" || !/^hypothesis-[a-f0-9]{32}$/.test(hypothesisId)) throw new Error("假设编号无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/hypotheses/${encodeURIComponent(hypothesisId)}`);
  });
  ipcMain.handle("runtime:get-identity-catalog", (_event, projectId) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/knowledge/entities`));
  ipcMain.handle("runtime:get-character-knowledge", (_event, projectId, entityId) => {
    const validId = value => typeof value === "string" && value.trim() === value && value.length > 0
      && Buffer.byteLength(value, "utf8") <= 160 && !/[\p{Cc}]/u.test(value);
    if (!validId(projectId) || !validId(entityId)) throw new Error("人物资料读取范围无效");
    const query = new URLSearchParams({ perspective: "author", entityId });
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/knowledge/state?${query}`);
  });
  ipcMain.handle("runtime:save-identity-decisions", (_event, projectId, decisions, expectedRevision, expectedCandidateRevisionId = null) => {
    if (!Array.isArray(decisions) || decisions.length > 256 || !Number.isInteger(expectedRevision) || expectedRevision < 0) throw new Error("身份决议参数无效");
    if (expectedCandidateRevisionId !== null && (typeof expectedCandidateRevisionId !== "string" || !expectedCandidateRevisionId.trim())) throw new Error("身份候选版本无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/artifacts/identity`, { method: "POST", body: JSON.stringify({ payload: { decisions }, expectedRevision, expectedCandidateRevisionId, idempotencyKey: `desktop-identity-${randomUUID()}` }) });
  });
  ipcMain.handle("runtime:save-property-rules", (_event, projectId, request) => {
    if (typeof projectId !== "string" || !projectId.trim() || !request || !Array.isArray(request.payload?.definitions)
      || !Array.isArray(request.payload.eventRules) || request.payload.eventRules.length > 16 || Object.keys(request.payload).length !== 2
      || request.payload.definitions.length > 64 || Buffer.byteLength(JSON.stringify(request.payload)) > 16 * 1024
      || !Number.isSafeInteger(request.expectedRevision) || request.expectedRevision < 0
      || typeof request.idempotencyKey !== "string" || !request.idempotencyKey.trim() || request.idempotencyKey.length > 160
      || (request.expectedCandidateRevisionId !== null && (typeof request.expectedCandidateRevisionId !== "string" || !request.expectedCandidateRevisionId.trim()))) throw new Error("属性规则参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/artifacts/propertyRules`, { method: "POST", body: JSON.stringify(request) });
  });
}

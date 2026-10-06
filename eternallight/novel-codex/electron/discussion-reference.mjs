import { modelInputTokenBudget } from "../shared/author-input-budget.mjs";

const fields = Object.freeze({
  scene: ["kind", "projectId", "snapshotId", "objectId", "revisionId", "sceneGroupId"],
  claim: ["kind", "projectId", "snapshotId", "objectId", "revisionId", "modelingBatchId", "assertionIndex"],
  commitment: ["kind", "projectId", "snapshotId", "commitmentId", "revisionId"],
});
const identifier = value => typeof value === "string" && value.trim() === value && value.length > 0
  && Buffer.byteLength(value, "utf8") <= 160 && !/[\p{Cc}]/u.test(value);
const stale = () => new Error("讨论来源已变化或不属于当前作品，请重新读取并选择；未使用其他版本代替。");

export function validateDiscussionReference(value) {
  if (!value || typeof value !== "object" || Array.isArray(value) || !Object.hasOwn(fields, value.kind)) {
    throw new Error("讨论来源引用无效");
  }
  const keys = fields[value.kind];
  if (Reflect.ownKeys(value).length !== keys.length || keys.some(key => !Object.hasOwn(value, key)
    || (key === "assertionIndex" ? !Number.isSafeInteger(value[key]) || value[key] < 0
      : key !== "kind" && !identifier(value[key])))) throw new Error("讨论来源引用字段无效");
  const reference = Object.freeze(Object.fromEntries(keys.map(key => [key, value[key]])));
  if (modelInputTokenBudget(JSON.stringify(reference)) > 1000) throw new Error("讨论来源引用超过安全文本预算");
  return reference;
}

/** Verify author-selected identity against existing Author reads; no text or authority is accepted from the renderer. */
export async function verifyDiscussionReference({ projectId, workspace, reference, runtimeRequest }) {
  if (!reference) return;
  reference = validateDiscussionReference(reference);
  if (reference.projectId !== projectId || workspace.project?.id !== projectId
    || reference.snapshotId !== workspace.headSnapshotId || !Number.isSafeInteger(workspace.revision)
    || workspace.revision < 0 || !identifier(workspace.sourceDigest)) throw stale();
  const base = `/api/projects/${encodeURIComponent(projectId)}`;
  if (reference.kind === "scene") {
    const matches = workspace.chapters?.filter(chapter => reference.objectId === `draft-${chapter.id}`) ?? [];
    if (matches.length !== 1) throw stale();
    const revisions = [matches[0].candidate, matches[0].accepted].filter(revision => revision?.id === reference.revisionId);
    // Saved and published may be the same revision. Its group identity must still be unique.
    if (!revisions.length || revisions.some(revision => revision.sceneGroups?.filter(group => group.id === reference.sceneGroupId).length !== 1)) throw stale();
  } else if (reference.kind === "claim") {
    const knowledge = await runtimeRequest(`${base}/knowledge/state?perspective=author&snapshotId=${encodeURIComponent(reference.snapshotId)}`);
    const matches = knowledge.entries?.filter(entry => entry.modelingBatchId === reference.modelingBatchId
      && entry.assertionIndex === reference.assertionIndex) ?? [];
    if (knowledge.projectId !== projectId || knowledge.snapshotId !== reference.snapshotId || matches.length !== 1
      || matches[0].sourceObjectId !== reference.objectId || matches[0].sourceRevisionId !== reference.revisionId) throw stale();
  } else {
    const detail = await runtimeRequest(`${base}/commitments/${encodeURIComponent(reference.commitmentId)}?snapshotId=${encodeURIComponent(reference.snapshotId)}`);
    if (detail.projectId !== projectId || detail.snapshotId !== reference.snapshotId || detail.payload?.id !== reference.commitmentId
      || detail.revisionId !== reference.revisionId || detail.sourceCurrent !== true) throw stale();
  }
  const fresh = await runtimeRequest(`${base}/creative-workspace`);
  if (fresh.project?.id !== projectId || fresh.revision !== workspace.revision
    || fresh.headSnapshotId !== workspace.headSnapshotId || fresh.sourceDigest !== workspace.sourceDigest) throw stale();
}

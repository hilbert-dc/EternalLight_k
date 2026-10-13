import type { CommitmentDetail, CommitmentNavigationRequest, NovelCodexApi } from "./types";
export type { CommitmentNavigationRequest } from "./types";

/** Opens a displayed directory revision for an author decision; never invents task delivery evidence. */
export async function resolveCommitmentNavigation(api: NovelCodexApi, request: CommitmentNavigationRequest): Promise<{ chapterId: string; detail: CommitmentDetail }> {
  const keys = ["projectId", "commitmentId", "revisionId", "snapshotId"] as const;
  if (Object.keys(request).length !== keys.length || keys.some(key => typeof request[key] !== "string"
    || !request[key].trim() || request[key] !== request[key].trim() || /[\p{Cc}]/u.test(request[key]) || new TextEncoder().encode(request[key]).length > 160)) throw new Error("伏笔资料引用无效，请从目录重新选择。");
  const workspace = await api.runtime.getCreativeWorkspace(request.projectId);
  if (workspace.project.id !== request.projectId || workspace.headSnapshotId !== request.snapshotId) {
    throw new Error("伏笔目录快照已变化，请重新读取；不会打开最新版本代替。");
  }
  const detail = await api.runtime.getCommitment(request.projectId, request.commitmentId, request.snapshotId);
  if (detail.projectId !== request.projectId || detail.payload.id !== request.commitmentId
    || detail.revisionId !== request.revisionId || detail.snapshotId !== request.snapshotId || !detail.sourceCurrent) {
    throw new Error("伏笔版本或原文来源已变化，请重新读取；未打开其他版本。");
  }
  const origin = detail.extractionOrigin?.chapterId;
  const chapters = workspace.chapters.filter(chapter => origin ? chapter.id === origin
    : detail.payload.setupEvidenceRefs.some(anchor => anchor.objectId === `draft-${chapter.id}`));
  if (chapters.length !== 1) throw new Error("无法唯一定位这条伏笔的章节，请先在目录核对原文。");
  const current = await api.runtime.getCreativeWorkspace(request.projectId);
  if (current.project.id !== request.projectId || current.revision !== workspace.revision
    || current.headSnapshotId !== workspace.headSnapshotId || current.sourceDigest !== workspace.sourceDigest
    || !current.chapters.some(chapter => chapter.id === chapters[0].id)) {
    throw new Error("读取期间作品已变化，请重新选择伏笔；不会沿用旧检查。");
  }
  return { chapterId: chapters[0].id, detail };
}

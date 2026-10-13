import type { MultimediaWorkspace } from "../../shared/multimedia-types";

/** Validate a receipt before it replaces the current editor's authoritative projection. */
export function assertWorkspaceReceipt(previous: MultimediaWorkspace, receipt: MultimediaWorkspace): void {
  if (!receipt?.project || !receipt.edition || !receipt.document
    || receipt.project.id !== previous.project.id
    || receipt.edition.projectId !== previous.edition.projectId
    || receipt.edition.id !== previous.edition.id
    || receipt.edition.editionId !== previous.edition.editionId
    || receipt.edition.scopeId !== previous.edition.scopeId
    || receipt.edition.branchId !== previous.edition.branchId
    || receipt.edition.medium !== previous.edition.medium
    || receipt.document.kind !== previous.document.kind
    || typeof receipt.revisionId !== "string" || !receipt.revisionId.trim()) {
    throw Error("保存回执与当前作品范围不一致，当前内容仍保留，请核对后重试。");
  }
}

import { useEffect, useRef, useState } from "react";
import type { CreativeWorkspace, NovelCodexApi } from "./types";

export type ChapterPublicationTarget = { scope: string; revisionId: string; reviewId: string | null; knowledge: "modeled" | "manual"; sceneIds: string[]; factIds: string[]; bodyOnlyConfirmed: boolean };
type Options = {
  scope: string; api?: NovelCodexApi; projectId: string; chapterId?: string;
  target: ChapterPublicationTarget | null; bodyOnlyConfirmed: boolean; dirty: boolean;
  isBlocked: () => boolean; save: () => Promise<boolean>; savedRevision: () => string | null;
  manualProposals: (workspace: CreativeWorkspace) => Record<string, unknown>[];
  onWorkspace: (workspace: CreativeWorkspace) => void;
  onTarget: (target: ChapterPublicationTarget) => void; onError: (message: string) => void;
};

/** One owner spans saving, source verification, check creation and read recovery. */
export function useChapterPublication(options: Options) {
  const current = useRef<Options | null>(options);
  current.current = options;
  const incarnation = useRef({ scope: options.scope, api: options.api });
  const owner = useRef<{ scope: Options; incarnation: object } | null>(null);
  const receipt = useRef<{ incarnation: object; revisionId: string; reviewId: string; batchId: string | null; factIds: string[] } | null>(null);
  if (incarnation.current.scope !== options.scope || incarnation.current.api !== options.api) {
    incarnation.current = { scope: options.scope, api: options.api }; owner.current = null; receipt.current = null;
  }
  const [activity, setActivity] = useState<typeof owner.current>(null);
  const preparing = activity?.incarnation === incarnation.current;
  useEffect(() => () => { current.current = null; }, []);

  async function prepare({ bodyOnlyConfirmed = false, expectedSource }: { bodyOnlyConfirmed?: boolean; expectedSource?: { revisionId: string; batchId: string } } = {}) {
    const scope = current.current;
    if (!scope?.api || !scope.chapterId || owner.current || scope.isBlocked()) return;
    const operation = { scope, incarnation: incarnation.current };
    const isCurrent = () => current.current !== null && incarnation.current === operation.incarnation;
    owner.current = operation;
    setActivity(operation);
    try {
      if (expectedSource && scope.dirty) throw new Error("当前正文有未保存修改，请先核对修改；不能用旧分析结果发布新内容。");
      if (scope.dirty && !(await scope.save())) return;
      if (!isCurrent()) return;
      let next = await scope.api.runtime.getCreativeWorkspace(scope.projectId);
      if (!isCurrent()) return;
      if (expectedSource && (current.current?.dirty || current.current?.isBlocked())) throw new Error("当前编辑或任务状态已变化，请先核对后再检查这次分析。");
      const chapter = next.chapters.find(item => item.id === scope.chapterId);
      const revision = chapter?.candidate;
      if (next.project.id !== scope.projectId || !revision) throw new Error("章节内容来源未能确认，请重新读取。");
      if (scope.dirty && revision.id !== scope.savedRevision()) throw new Error("保存后的章节版本已变化，请重新检查当前内容。");
      if (next.tasks.some(task => task.target?.chapterId === scope.chapterId && ["queued", "running", "waiting"].includes(task.status))) {
        throw new Error("本章仍有创作任务进行中，请等待任务交付后再发布。");
      }
      const modeling = next.modeling.find(item => item.objectId === `draft-${scope.chapterId}`);
      if (!modeling || modeling.sourceRevisionId !== revision.id) throw new Error("故事知识来源未能确认，请重新读取当前章节。");
      const batchId = modeling.status === "ready" ? modeling.batchId : null;
      if (expectedSource && (revision.id !== expectedSource.revisionId || batchId !== expectedSource.batchId)) throw new Error("这次分析的正文或批次已被更新，请查看当前章节；不会改用其他分析结果。");
      if (modeling.status === "ready" && !batchId) throw new Error("当前分析批次未能确认，请重新读取。");
      const knowledge = batchId ? "modeled" : "manual";
      const savedCheck = receipt.current?.incarnation === operation.incarnation && receipt.current.revisionId === revision.id && receipt.current.batchId === batchId ? receipt.current : null;
      const sourceReviews = next.reviews.filter(review => review.artifactKind === "draft" && review.artifactRevisionId === revision.id && review.status === "ready");
      let existing = sourceReviews.find(review => review.artifactKind === "draft" && review.artifactRevisionId === revision.id
        && review.projectRevision === next.revision && review.status === "ready"
        && (batchId ? review.selections.modelingBatchId === batchId : !review.selections.modelingBatchId));
      const knownReview = scope.target?.scope === scope.scope && scope.target.reviewId === existing?.id && scope.target.revisionId === revision.id;
      if (savedCheck) existing = sourceReviews.find(review => review.id === savedCheck.reviewId && review.projectRevision === next.revision);
      else if (sourceReviews.length !== 1 && !knownReview) existing = undefined;
      let factIds = savedCheck ? savedCheck.factIds : existing ? knownReview ? scope.target!.factIds : [...new Set(next.factProposals.filter(fact => fact.draft_revision_id === revision.id).map(fact => fact.id))] : [];
      const proposals = batchId ? [] : scope.manualProposals(next);
      const hasFacts = Boolean(existing && factIds.length);
      const consent = bodyOnlyConfirmed || (scope.target?.scope === scope.scope && scope.target.revisionId === revision.id && scope.bodyOnlyConfirmed);
      scope.onWorkspace(next);
      if (!batchId && !proposals.length && !hasFacts && !consent) {
        scope.onTarget({ scope: scope.scope, revisionId: revision.id, reviewId: null, knowledge, sceneIds: revision.sceneGroups.map(group => group.id), factIds: [], bodyOnlyConfirmed: false });
        return;
      }
      if (savedCheck && !existing) throw new Error("已创建的发布检查已变化，请重新读取确认。");
      let reviewId = existing?.id;
      if (!reviewId) {
        const created = await scope.api.runtime.createReview(scope.projectId, "draft", revision.id, proposals, batchId ? { modelingBatchId: batchId } : {}, batchId);
        reviewId = created.id;
        factIds = [...new Set(created.factProposals.map(fact => fact.id))];
      }
      if (!isCurrent()) return;
      if (expectedSource && (current.current?.dirty || current.current?.isBlocked())) throw new Error("当前编辑或任务状态已变化，请先核对已创建的检查。");
      receipt.current = { incarnation: operation.incarnation, revisionId: revision.id, reviewId, batchId, factIds };
      next = await scope.api.runtime.getCreativeWorkspace(scope.projectId);
      if (!isCurrent()) return;
      const latestModeling = next.modeling.find(item => item.objectId === `draft-${scope.chapterId}`);
      const review = next.reviews.find(item => item.id === reviewId);
      if (expectedSource && (next.chapters.find(item => item.id === scope.chapterId)?.candidate?.id !== expectedSource.revisionId || latestModeling?.batchId !== expectedSource.batchId || latestModeling?.sourceRevisionId !== expectedSource.revisionId)) throw new Error("分析来源在检查期间已变化，请重新读取；不会确认其他版本。");
      if (next.project.id !== scope.projectId) throw new Error("发布检查返回了不同作品，请重新读取当前作品。");
      for (const id of factIds) {
        const copies = next.factProposals.filter(fact => fact.id === id);
        if (copies.some(fact => JSON.stringify(fact) !== JSON.stringify(copies[0]))) throw new Error("同一故事设定来源存在不同内容，请重新读取确认。");
      }
      scope.onWorkspace(next);
      if (next.project.id !== scope.projectId || next.chapters.find(item => item.id === scope.chapterId)?.candidate?.id !== revision.id
        || !review || review.artifactRevisionId !== revision.id || review.projectRevision !== next.revision || review.status !== "ready"
        || latestModeling?.sourceRevisionId !== revision.id || (batchId ? latestModeling.status !== "ready" || latestModeling.batchId !== batchId : latestModeling.status !== "model_pending")
        || next.tasks.some(task => task.target?.chapterId === scope.chapterId && ["queued", "running", "waiting"].includes(task.status))
        || factIds.some(id => !next.factProposals.some(fact => fact.id === id && fact.draft_revision_id === revision.id))) {
        receipt.current = null;
        throw new Error("发布检查来源已变化，请重新准备检查。");
      }
      scope.onTarget({ scope: scope.scope, revisionId: revision.id, reviewId, knowledge, sceneIds: revision.sceneGroups.map(group => group.id), factIds, bodyOnlyConfirmed: knowledge === "manual" && consent });
    } catch (reason) {
      if (isCurrent()) scope.onError(reason instanceof Error ? reason.message : "发布检查准备失败，请重新读取。");
    } finally {
      if (owner.current === operation) owner.current = null;
      if (isCurrent()) setActivity(null);
    }
  }
  return { preparing, prepare };
}

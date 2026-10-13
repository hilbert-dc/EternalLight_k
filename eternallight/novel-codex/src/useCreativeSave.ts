import type { Dispatch, RefObject, SetStateAction } from "react";
import { useEffect, useRef } from "react";
import { firstOutlineEditorError } from "./creative-product-logic";
import type { EditableChapterPlan, EditorSession, ReviewUiSnapshot, WorkspaceView } from './creative-workspace-types';
import { StoppedExecutionError } from "./execution-stop";
import { writeReviewUi } from "./review-ui-storage";
import { useSupplementalEditor } from "./supplemental-editor";
import type { Chapter, CreativeArtifactKind, CreativeArtifactPayload, CreativeWorkspace, NovelCodexApi, OutlineNode, RollingPlanPayload } from "./types";
import type { WorkTaskControls } from "./useWorkTaskControls";

type Input = {
  beginWrite: (label: string) => (() => void) | null;
  writingRef: RefObject<boolean>;
  taskControls: WorkTaskControls;
  supplementalEditor: ReturnType<typeof useSupplementalEditor>;
  setBusy: Dispatch<SetStateAction<boolean>>;
  setBusyAction: Dispatch<SetStateAction<string>>;
  setError: Dispatch<SetStateAction<string | null>>;
  setNotice: Dispatch<SetStateAction<string | null>>;
  activeIdentityRef: RefObject<{ scope: string; api: NovelCodexApi | undefined; }>;
  editorOwner: { scope: string; api: NovelCodexApi | undefined; };
  reload: (validate?: (next: CreativeWorkspace) => void) => Promise<void>;
  editorSessionRef: RefObject<EditorSession | null>;
  api: NovelCodexApi | undefined;
  projectId: string;
  setWorkspace: Dispatch<SetStateAction<CreativeWorkspace | null>>;
  setImpactCursor: Dispatch<SetStateAction<string | null>>;
  setImpactCounts: Dispatch<SetStateAction<{ total: number; open: number; retained: number; lockedConflict: number; }>>;
  setConflict: Dispatch<SetStateAction<{ baseCandidateRevisionId: string | null; serverCandidateRevisionId: string | null; baseContent: string; } | null>>;
  candidateRevisionId: (next: CreativeWorkspace) => string | null;
  workspace: CreativeWorkspace | null;
  pendingPlanningSave: RefObject<{ owner: object; signature: string; revisionId: string; } | null>;
  lastSavedRevisionIdRef: RefObject<string | null>;
  onFormalChange: () => Promise<void>;
  editorIdentity: string;
  view: WorkspaceView;
  brief: { premise: string; promise: string; stakes: string; endingDirection: string; };
  outlineNodes: OutlineNode[];
  setOutlineError: (error: { nodeId: string; message: string } | null) => void;
  feedbackTarget: { impactId: string; targetKind: "outline" | "plan"; targetNodeId: string; targetLabel: string; } | null;
  plans: EditableChapterPlan[];
  draftTitle: string;
  draftSceneRows: NonNullable<CreativeWorkspace["chapters"][number]["candidate"]>["sceneGroups"];
  lockedSceneGroups: string[];
  selectedChapter: Chapter;
  pendingChapterSave: RefObject<{ scope: string; signature: string; revisionId: string; } | null>;
  chapterStorageKey: string | null;
  unreadableDrafts: RefObject<Set<string>>;
  busy: boolean;
  dirty: boolean;
  reviewDirty: boolean;
  reviewSessionKey: () => string;
  reviewUiSnapshot: ReviewUiSnapshot;
  reviewDirtyRef: RefObject<boolean>;
  setReviewDirty: Dispatch<SetStateAction<boolean>>;
  onSaveHandlerChange: ((handler: (() => Promise<boolean>) | null) => void) | undefined;
  loading: boolean;
  agentRunning: boolean;
  artifactLocked: boolean;
  conflict: { baseCandidateRevisionId: string | null; serverCandidateRevisionId: string | null; baseContent: string; } | null;
  onPlanningSaveHandlerChange: ((handler: (() => Promise<boolean>) | null) => void) | undefined;
  dirtyRef: RefObject<boolean>;
  runningNow: RefObject<boolean>;
  onDirtyChange: (dirty: boolean, chapterScope?: { projectId: string; chapterId: string; } | null) => void;
  onDiscardHandlerChange: ((handler: (() => boolean) | null) => void) | undefined;
  discardCurrentReviewUi: () => boolean;
};

export function useCreativeSave({ beginWrite, writingRef, taskControls, supplementalEditor, setBusy, setBusyAction, setError, setNotice, activeIdentityRef, editorOwner, reload, editorSessionRef, api, projectId, setWorkspace, setImpactCursor, setImpactCounts, setConflict, candidateRevisionId, workspace, pendingPlanningSave, lastSavedRevisionIdRef, onFormalChange, editorIdentity, view, brief, outlineNodes, setOutlineError, feedbackTarget, plans, draftTitle, draftSceneRows, lockedSceneGroups, selectedChapter, pendingChapterSave, chapterStorageKey, unreadableDrafts, busy, dirty, reviewDirty, reviewSessionKey, reviewUiSnapshot, reviewDirtyRef, setReviewDirty, onSaveHandlerChange, loading, agentRunning, artifactLocked, conflict, onPlanningSaveHandlerChange, dirtyRef, runningNow, onDirtyChange, onDiscardHandlerChange, discardCurrentReviewUi }: Input) {
  async function perform(actionName: string, action: () => Promise<void>, success: string, validate?: (next: CreativeWorkspace) => void): Promise<boolean> {
    if (writingRef.current || taskControls.hasPendingRequests() || supplementalEditor.active) return false;
    const finishWrite = beginWrite(actionName);
    if (!finishWrite) return false;
    setNotice(null);
    try {
      await action();
      if (activeIdentityRef.current !== editorOwner) return false;
      await reload(validate);
      if (activeIdentityRef.current !== editorOwner) return false;
      setNotice(success);
      return true;
    } catch (reason) {
      if (activeIdentityRef.current !== editorOwner) return false;
      if (reason instanceof StoppedExecutionError) {
        setNotice(reason.message);
        try { await reload(); } catch {
          if (activeIdentityRef.current === editorOwner) setNotice(`${reason.message}内容状态待重新读取。`);
        }
        return false;
      }
      const message = reason instanceof Error ? reason.message : "操作失败";
      setError(message);
      if (/revision|版本|candidate|候选|conflict|冲突/i.test(message)) {
        const session = editorSessionRef.current;
        try {
          const [next, impactPage] = await Promise.all([
            api?.runtime.getCreativeWorkspace(projectId),
            api?.runtime.listPlanningImpacts(projectId, null, 25),
          ]);
          if (activeIdentityRef.current !== editorOwner) return false;
          if (next && impactPage) {
            setWorkspace({ ...next, impacts: impactPage.data, openImpactCount: impactPage.counts.open });
            setImpactCursor(impactPage.nextCursor);
            setImpactCounts(impactPage.counts);
            setConflict({
              baseCandidateRevisionId: session?.baseCandidateRevisionId ?? null,
              serverCandidateRevisionId: candidateRevisionId(next),
              baseContent: session?.baseContent ?? "（基线不可用）",
            });
          }
        } catch {
          if (activeIdentityRef.current !== editorOwner) return false;
          setConflict({
            baseCandidateRevisionId: session?.baseCandidateRevisionId ?? null,
            serverCandidateRevisionId: workspace ? candidateRevisionId(workspace) : null,
            baseContent: session?.baseContent ?? "（基线不可用）",
          });
        }
      }
      return false;
    } finally {
      finishWrite();
    }
  }

  async function savePlanning(kind: CreativeArtifactKind, payload: CreativeArtifactPayload, expectedRevision: number, expectedCandidate: string | null, linkedImpacts: string[], action: string, success: string): Promise<boolean> {
    if (!api) return false;
    const signature = JSON.stringify({ payload, linkedImpacts });
    const previousReceipt = pendingPlanningSave.current;
    if (previousReceipt?.owner === editorOwner && previousReceipt.signature !== signature) {
      setError("规划已保存，请先核对保存结果；本地新修改仍保留，不会再次保存。");
      return false;
    }
    let sourceError: string | null = null;
    const completed = await perform(action, async () => {
      const pending = pendingPlanningSave.current;
      if (pending?.owner === editorOwner) return;
      const saved = await api.runtime.saveArtifact(projectId, kind, payload, expectedRevision, expectedCandidate, linkedImpacts);
      if (activeIdentityRef.current !== editorOwner) return;
      pendingPlanningSave.current = { owner: editorOwner, signature, revisionId: saved.revisionId };
      lastSavedRevisionIdRef.current = saved.revisionId;
    }, success, next => {
      const pending = pendingPlanningSave.current;
      if (!pending || pending.owner !== editorOwner || next.project.id !== projectId || next.artifacts[kind].candidate?.id !== pending.revisionId) {
        sourceError = "已保存的规划来源已变化，请重新核对；本地编辑仍保留。";
        throw new Error(sourceError);
      }
    });
    if (activeIdentityRef.current !== editorOwner) return false;
    if (completed) {
      pendingPlanningSave.current = null;
      if (kind === "outline") {
        try { await onFormalChange(); }
        catch (reason) {
          if (activeIdentityRef.current === editorOwner) setError(`大纲已保存，但目录刷新失败：${reason instanceof Error ? reason.message : "请重新读取作品"}`);
        }
      }
    }
    else if (pendingPlanningSave.current?.owner === editorOwner) {
      setError(previous => sourceError ?? `${previous ? `${previous}。` : ""}规划已保存，但读取结果尚未确认。再次保存只会核对该保存版本，不会重复写入；本地编辑仍保留。`);
    }
    return completed;
  }

  async function saveCurrent(baselineWorkspace?: CreativeWorkspace): Promise<boolean> {
    const activeWorkspace = baselineWorkspace ?? workspace;
    if (!api || !activeWorkspace) return false;
    const session = editorSessionRef.current;
    if (!session || session.identity !== editorIdentity) throw new Error("编辑会话已切换，请重新打开当前内容");
    const expectedRevision = session.baseProjectRevision;
    const expectedCandidateRevisionId = session.baseCandidateRevisionId;
    if (view === "brief") {
      return savePlanning("brief", brief, expectedRevision, expectedCandidateRevisionId, [], "正在保存创作简报…", "创作简报已保存为候选版本。");
    } else if (view === "outline") {
      const firstInvalid = firstOutlineEditorError(outlineNodes);
      if (firstInvalid) {
        setOutlineError(firstInvalid);
        setError("大纲还有未完成字段；已定位到第一处错误。");
        return false;
      }
      setOutlineError(null);
      const linkedImpactIds = feedbackTarget?.targetKind === "outline" ? [feedbackTarget.impactId] : [];
      return savePlanning("outline", { nodes: outlineNodes }, expectedRevision, expectedCandidateRevisionId, linkedImpactIds, "正在保存全书大纲…", linkedImpactIds.length ? "全书大纲候选已链接原反馈；采用后将自动关闭该反馈。" : "全书大纲已保存为候选版本。");
    } else if (view === "plans") {
      const outline = activeWorkspace.artifacts.outline.working ?? activeWorkspace.artifacts.outline.accepted;
      if (!outline?.payload) throw new Error("请先保存并选用全书大纲，再保存近章规划");
      const defaultNodeId = (outline.payload.nodes as OutlineNode[]).at(-1)?.id;
      const payload: RollingPlanPayload = { sourceOutlineRevisionId: outline.id, chapters: plans.map(({ id, dependencyKeys, ...chapter }) => ({ ...chapter, dependencyKeys: dependencyKeys.split(",").map((key) => key.trim()).filter(Boolean), chapterPlanId: chapter.chapterPlanId || undefined, clientKey: chapter.chapterPlanId ? undefined : id, primaryNodeId: chapter.primaryNodeId || defaultNodeId || "", publicTitle: chapter.publicTitle ?? "", writerGuidance: chapter.writerGuidance ?? "", publicCommitments: chapter.publicCommitments ?? [], allowances: chapter.allowances ?? [] })), window: plans.length };
      const linkedImpactIds = feedbackTarget?.targetKind === "plan" ? [feedbackTarget.impactId] : [];
      return savePlanning("plan", payload, expectedRevision, expectedCandidateRevisionId, linkedImpactIds, "正在保存近章规划…", linkedImpactIds.length ? "近章规划候选已链接原反馈；采用后将自动关闭该反馈。" : "近章规划已保存为候选版本。");
    } else {
      if (!draftTitle.trim()) {
        setError("请填写章节标题，再保存。");
        return false;
      }
      if (draftSceneRows.some((scene) => !scene.body.trim())) {
        setError("请填写空场景正文，或删除不需要的空场景。");
        return false;
      }
      const sceneGroups = draftSceneRows.map((scene) => ({ id: scene.id, sceneId: scene.sceneId ?? null, body: scene.body, locked: lockedSceneGroups.includes(scene.id) }));
      const normalizedBody = sceneGroups.map((scene) => scene.body.trim()).join("\n\n---\n\n");
      const payload = { ...(selectedChapter ? { chapterId: selectedChapter.id, chapterNumber: selectedChapter.number } : { chapterNumber: 1 }), title: draftTitle.trim(), body: normalizedBody, sceneGroups };
      const signature = JSON.stringify(payload);
      const completed = await perform("正在保存这一章…", async () => {
        const pending = pendingChapterSave.current;
        if (pending?.scope === editorIdentity && pending.signature !== signature) throw new Error("章节已保存，请先重新读取保存结果再继续编辑。");
        const saved = pending?.scope === editorIdentity ? pending : await api.runtime.saveDraftCheckpoint(projectId, payload, expectedRevision, expectedCandidateRevisionId);
        if (activeIdentityRef.current !== editorOwner) return;
        pendingChapterSave.current = { scope: editorIdentity, signature, revisionId: saved.revisionId };
        lastSavedRevisionIdRef.current = saved.revisionId;
        await onFormalChange();
      }, "这一章已保存；发布后更新已发布内容。", next => {
        if (next.project.id !== projectId || candidateRevisionId(next) !== pendingChapterSave.current?.revisionId) throw new Error("已保存的章节内容已变化，请重新读取确认；本地编辑仍保留。");
      });
      if (activeIdentityRef.current === editorOwner) {
        if (completed) {
          pendingChapterSave.current = null;
          if (chapterStorageKey) { try { localStorage.removeItem(chapterStorageKey); unreadableDrafts.current.delete(chapterStorageKey); } catch { setError("正文已保存，本地恢复记录尚未清理。"); } }
        }
        else if (pendingChapterSave.current?.scope === editorIdentity) setError("章节已保存，但读取结果失败。重新读取不会重复保存，正文仍保留。");
      }
      return completed;
    }
  }

  async function savePendingChanges(): Promise<boolean> {
    if (writingRef.current || busy || taskControls.hasPendingRequests()) return false;
    if (supplementalEditor.current.current) return supplementalEditor.current.current.save();
    if (dirty && !(await saveCurrent())) return false;
    if (reviewDirty) {
      writeReviewUi(reviewSessionKey(), reviewUiSnapshot);
      reviewDirtyRef.current = false;
      setReviewDirty(false);
    }
    return true;
  }

  useEffect(() => {
    if (!onSaveHandlerChange) return;
    onSaveHandlerChange(savePendingChanges);
    return () => onSaveHandlerChange(null);
  });

  const canSavePlanningForDiscussion = view !== "chapters" && Boolean(api && workspace) && dirty && !reviewDirty
    && !supplementalEditor.active && !busy && !loading && !agentRunning && !artifactLocked && !conflict;
  const planningSaveAvailable = useRef(false);
  planningSaveAvailable.current = canSavePlanningForDiscussion;
  useEffect(() => {
    if (!onPlanningSaveHandlerChange) return;
    onPlanningSaveHandlerChange(canSavePlanningForDiscussion ? async () => {
      if (activeIdentityRef.current !== editorOwner || !planningSaveAvailable.current || !dirtyRef.current || reviewDirtyRef.current
        || supplementalEditor.current.current || writingRef.current || runningNow.current || taskControls.hasPendingRequests()) return false;
      if (!await saveCurrent() || activeIdentityRef.current !== editorOwner || dirtyRef.current || reviewDirtyRef.current
        || supplementalEditor.current.current) return false;
      onDirtyChange(false);
      return true;
    } : null);
    return () => onPlanningSaveHandlerChange(null);
  });

  useEffect(() => {
    if (!onDiscardHandlerChange) return;
    onDiscardHandlerChange(discardCurrentReviewUi);
    return () => onDiscardHandlerChange(null);
  });

  return { saveCurrent, perform, canSavePlanningForDiscussion };
}

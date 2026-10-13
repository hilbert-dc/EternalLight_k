import { useEffect,useRef,useState,type Dispatch,type RefObject,type SetStateAction } from "react";
import { chapterDraftKey,readChapterDraft } from "./chapter-draft-storage";
import { chapterEditorContent } from "./chapter-editor-content";
import { displayChapters,savedChapterTitle } from "./chapter-title";
import { newDraftScene,rebaseEditorBaseline } from "./creative-product-logic";
import type { CreativeWorkspaceProps } from "./creative-workspace-props";
import type { EditorSession,ReviewUiSnapshot } from "./creative-workspace-types";
import { mergeChapterDraft,resolveChapterDraft } from "./merge-chapter-draft";
import type { useSupplementalEditor } from "./supplemental-editor";
import type { Chapter,CreativeArtifactKind,CreativeWorkspace,NovelCodexApi } from "./types";
import { useCreativeSave } from "./useCreativeSave";
import { usePlanningDraft } from "./usePlanningDraft";

type Input = Pick<CreativeWorkspaceProps, "authorKey" | "api" | "projectId" | "view" | "state" | "refreshToken" | "agentRunning" | "taskControls" | "onFormalChange" | "onDirtyChange" | "onChapterTitlePreview" | "onSaveHandlerChange" | "onPlanningSaveHandlerChange" | "onDiscardHandlerChange"> & {
  selectedChapter: Chapter;
  supplementalEditor: ReturnType<typeof useSupplementalEditor>;
  publicationPreparing: boolean;
  chapterAcceptActivity: object | null;
  reviewDirty: boolean;
  reviewDirtyRef: RefObject<boolean>;
  reviewUiSnapshot: ReviewUiSnapshot;
  setReviewDirty: Dispatch<SetStateAction<boolean>>;
  feedbackTarget: { impactId: string; targetKind: "outline" | "plan"; targetNodeId: string; targetLabel: string } | null;
  setError: Dispatch<SetStateAction<string | null>>;
  setNotice: Dispatch<SetStateAction<string | null>>;
  readReviewSessionKey: () => string;
  discardCurrentReviewUi: () => boolean;
};

/** Owns the editable baseline, recovery record and saved-but-unconfirmed receipt.
 * Callers receive commands; session identity and mutable storage stay private.
 */
export function useWorkspaceEditor({ authorKey, api, projectId, view, state, selectedChapter, refreshToken, agentRunning, taskControls, supplementalEditor, publicationPreparing, chapterAcceptActivity, reviewDirty, reviewDirtyRef, reviewUiSnapshot, setReviewDirty, feedbackTarget, setError, setNotice, onFormalChange, onDirtyChange, onChapterTitlePreview, onSaveHandlerChange, onPlanningSaveHandlerChange, onDiscardHandlerChange, discardCurrentReviewUi, readReviewSessionKey }: Input) {
  const writingRef = useRef(false);
  const writeOperationRef = useRef<object | null>(null);
  const [workspace, setWorkspace] = useState<CreativeWorkspace | null>(null);
  const [dirty, setDirty] = useState(false);
  const [loading, setLoading] = useState(true);
  const [workspaceBusy, setBusy] = useState(false);
  const [busyAction, setBusyAction] = useState("");
  const planningDraft = usePlanningDraft(() => setDirty(true));
  const [draftSceneRows, setDraftSceneRows] = useState<NonNullable<CreativeWorkspace["chapters"][number]["candidate"]>["sceneGroups"]>([]);
  const [draftTitle, setDraftTitle] = useState("");
  const pendingChapterSave = useRef<{ scope: string; signature: string; revisionId: string } | null>(null);
  const [lockedSceneGroups, setLockedSceneGroups] = useState<string[]>([]);
  const [historyRetry, setHistoryRetry] = useState(0);
  const [conflict, setConflict] = useState<{ baseCandidateRevisionId: string | null; serverCandidateRevisionId: string | null; baseContent: string } | null>(null);
  const [impactCursor, setImpactCursor] = useState<string | null>(null);
  const [impactCounts, setImpactCounts] = useState({ total: 0, open: 0, retained: 0, lockedConflict: 0 });
  const lastSavedRevisionIdRef = useRef<string | null>(null);
  const pendingPlanningSave = useRef<{ owner: object; signature: string; revisionId: string } | null>(null);
  const dirtyRef = useRef(false);
  const loadedProjectRef = useRef<string | null>(null);
  const editorSessionRef = useRef<EditorSession | null>(null);
  const restoredDraftKey = useRef<string | null>(null);
  const unreadableDrafts = useRef(new Set<string>());
  const [draftStorageOwner, setDraftStorageOwner] = useState<object | null>(null);
  const chapterMergeBase = useRef<CreativeWorkspace["chapters"][number]["candidate"]>(null);
  const chapterTitleOwnerRef = useRef<object | null>(null);
  const chapterBaselineRef = useRef<{ identity: string; content: string } | null>(null);
  const runningNow = useRef(agentRunning);
  runningNow.current = agentRunning;
  const { brief, outlineNodes, plans, outlineError, reportOutlineError: setOutlineError } = planningDraft;
  const busy = workspaceBusy || taskControls.pending || publicationPreparing || chapterAcceptActivity !== null;
  const viewKind: CreativeArtifactKind | "draft" = view === "plans" ? "plan" : view === "chapters" ? "draft" : view;
  const artifactLocked = viewKind !== "draft" && workspace?.artifacts[viewKind].locked === true;
  const editorIdentity = `${projectId}:${view}:${view === "chapters" ? selectedChapter?.id ?? "new" : viewKind}`;
  const activeIdentityRef = useRef({ scope: editorIdentity, api });
  if (activeIdentityRef.current.scope !== editorIdentity || activeIdentityRef.current.api !== api) activeIdentityRef.current = { scope: editorIdentity, api };
  const editorOwner = activeIdentityRef.current;
  const chapterStorageKey = view === "chapters" && selectedChapter ? chapterDraftKey(authorKey, projectId, selectedChapter.id) : null;
  if (pendingPlanningSave.current && pendingPlanningSave.current.owner !== editorOwner) pendingPlanningSave.current = null;
  useEffect(() => {
    activeIdentityRef.current = editorOwner;
    writeOperationRef.current = null;
    writingRef.current = false;
    setBusy(false);
    setBusyAction("");
    return () => {
      if (activeIdentityRef.current === editorOwner) activeIdentityRef.current = { ...editorOwner };
    };
  }, [editorOwner]);
  function candidateRevision(next: CreativeWorkspace) {
    return viewKind === "draft"
      ? next.chapters.find((chapter) => chapter.id === selectedChapter?.id)?.candidate ?? null
      : next.artifacts[viewKind].candidate;
  }

  function candidateRevisionId(next: CreativeWorkspace): string | null {
    return candidateRevision(next)?.id ?? null;
  }

  function candidateContent(next: CreativeWorkspace): string {
    const revision = candidateRevision(next);
    return revision ? "body" in revision ? `${revision.title}\n\n${revision.body}` : JSON.stringify(revision.payload, null, 2) : "（当时没有候选版本）";
  }

  function loadEditor(next: CreativeWorkspace) {
    if (next.project.id !== projectId) return;
    chapterTitleOwnerRef.current = next.project.id === projectId ? editorOwner : null;
    const candidate = candidateRevisionId(next);
    editorSessionRef.current = { identity: editorIdentity, baseProjectRevision: next.revision, baseCandidateRevisionId: candidate, baseContent: candidateContent(next) };
    setConflict(null);
    chapterMergeBase.current = view === "chapters" ? next.chapters.find(item => item.id === selectedChapter?.id)?.candidate ?? null : null;
    if (view !== "chapters") {
      planningDraft.load(next, view);
    } else {
      const chapter = next.chapters.find((item) => item.id === selectedChapter?.id);
      const revision = chapter?.candidate ?? chapter?.accepted;
      const title = chapter ? savedChapterTitle(chapter) : "";
      const groups = revision?.sceneGroups ?? [];
      const scenes = groups.length ? groups : [newDraftScene()];
      const locked = groups.filter((group) => group.locked).map((group) => group.id);
      chapterBaselineRef.current = { identity: editorIdentity, content: chapterEditorContent(title, scenes, locked) };
      setDraftTitle(title);
      setDraftSceneRows(scenes);
      setLockedSceneGroups(locked);
    }
    dirtyRef.current = false;
    setDirty(false);
    setDraftStorageOwner(editorOwner);
    const restoreChapterDraft = chapterStorageKey !== restoredDraftKey.current;
    restoredDraftKey.current = chapterStorageKey;
    if (chapterStorageKey && restoreChapterDraft) {
      try {
        const recovery = readChapterDraft(chapterStorageKey);
        if (recovery) {
          const locked = recovery.sceneGroups.filter(scene => scene.locked).map(scene => scene.id);
          const content = chapterEditorContent(recovery.title, recovery.sceneGroups, locked);
          if (content !== chapterBaselineRef.current?.content) {
            setDraftTitle(recovery.title); setDraftSceneRows(recovery.sceneGroups); setLockedSceneGroups(locked);
            editorSessionRef.current = { identity: editorIdentity, baseProjectRevision: recovery.baseProjectRevision, baseCandidateRevisionId: recovery.baseCandidateRevisionId, baseContent: recovery.baseContent };
            if (candidate !== recovery.baseCandidateRevisionId) {
              chapterMergeBase.current = null;
              setConflict({ baseCandidateRevisionId: recovery.baseCandidateRevisionId, serverCandidateRevisionId: candidate, baseContent: recovery.baseContent });
            }
            dirtyRef.current = true; setDirty(true); setNotice("已恢复尚未保存的章节正文，请核对后保存。");
          }
        }
      } catch (reason) { unreadableDrafts.current.add(chapterStorageKey); setError(reason instanceof Error ? reason.message : "本地草稿读取失败，原记录仍保留。"); }
    }
  }

  useEffect(() => {
    if (!chapterStorageKey || draftStorageOwner !== editorOwner || dirtyRef.current !== dirty || unreadableDrafts.current.has(chapterStorageKey)) return;
    const session = editorSessionRef.current;
    if (!session || session.identity !== editorIdentity) return;
    try {
      if (dirty) localStorage.setItem(chapterStorageKey, JSON.stringify({ title: draftTitle,
        sceneGroups: draftSceneRows.map(scene => ({ id: scene.id, sceneId: scene.sceneId ?? null, body: scene.body, locked: lockedSceneGroups.includes(scene.id) })),
        baseProjectRevision: session.baseProjectRevision, baseCandidateRevisionId: session.baseCandidateRevisionId, baseContent: session.baseContent }));
      else localStorage.removeItem(chapterStorageKey);
    } catch { setError("本地章节草稿暂未保存，请先保存正文或复制备份再离开。"); }
  }, [chapterStorageKey, draftStorageOwner, editorOwner, dirty, draftTitle, draftSceneRows, lockedSceneGroups, workspace]);

  function updateChapterDirty(title: string, scenes: typeof draftSceneRows, locked: string[]) {
    const baseline = chapterBaselineRef.current;
    const changed = baseline?.identity !== editorIdentity || chapterEditorContent(title, scenes, locked) !== baseline.content;
    dirtyRef.current = changed;
    setDirty(changed);
  }

  async function reload(validate?: (next: CreativeWorkspace) => void) {
    if (!api) return;
    const next = await api.runtime.getCreativeWorkspace(projectId);
    const impactPage = await api.runtime.listPlanningImpacts(projectId, null, 25);
    if (activeIdentityRef.current !== editorOwner) return;
    validate?.(next);
    setWorkspace({ ...next, impacts: impactPage.data, openImpactCount: impactPage.counts.open });
    setImpactCursor(impactPage.nextCursor);
    setImpactCounts(impactPage.counts);
    loadEditor(next);
    setLoading(false);
  }

  useEffect(() => {
    let cancelled = false;
    const projectChanged = loadedProjectRef.current !== projectId;
    loadedProjectRef.current = projectId;
    if (!projectChanged) {
      void Promise.all([api?.runtime.getCreativeWorkspace(projectId), api?.runtime.listPlanningImpacts(projectId, null, 25)]).then(([next, impactPage]) => {
        if (!cancelled && next && impactPage) {
          setWorkspace({ ...next, impacts: impactPage.data, openImpactCount: impactPage.counts.open });
          setImpactCursor(impactPage.nextCursor);
          setImpactCounts(impactPage.counts);
          const session = editorSessionRef.current;
          const serverCandidateRevisionId = candidateRevisionId(next);
          if ((dirtyRef.current || reviewDirtyRef.current || supplementalEditor.current.current)
            && session?.identity === editorIdentity && serverCandidateRevisionId !== session.baseCandidateRevisionId) {
            setConflict({ baseCandidateRevisionId: session.baseCandidateRevisionId, serverCandidateRevisionId, baseContent: session.baseContent });
          }
          setLoading(false);
        }
      }).catch((reason) => {
        if (!cancelled) { setError(reason instanceof Error ? reason.message : "工作区刷新失败"); setLoading(false); }
      });
      return () => { cancelled = true; };
    }
    dirtyRef.current = false;
    restoredDraftKey.current = null;
    editorSessionRef.current = null;
    setWorkspace(null);
    setLoading(true);
    setError(null);
    setNotice(null);
    setDirty(false);
    planningDraft.reset();
    setDraftSceneRows([]);
    if (!api) { setLoading(false); return; }
    void Promise.all([api.runtime.getCreativeWorkspace(projectId), api.runtime.listPlanningImpacts(projectId, null, 25)]).then(([next, impactPage]) => {
      if (!cancelled) {
        setWorkspace({ ...next, impacts: impactPage.data, openImpactCount: impactPage.counts.open });
        setImpactCursor(impactPage.nextCursor);
        setImpactCounts(impactPage.counts);
        setLoading(false);
      }
    }).catch((reason) => {
      if (!cancelled) { setError(reason instanceof Error ? reason.message : "工作区加载失败"); setLoading(false); }
    });
    return () => { cancelled = true; };
  }, [api, projectId, refreshToken]);

  useEffect(() => {
    if (!api || !(agentRunning || taskControls.resuming) || dirty) return;
    let cancelled = false;
    let pending = false;
    const refreshGeneration = async () => {
      if (pending) return;
      pending = true;
      try {
        const next = await api.runtime.getCreativeWorkspace(projectId);
        if (!cancelled) setWorkspace((current) => current ? { ...next, impacts: current.impacts, openImpactCount: current.openImpactCount } : next);
      } catch { /* The completion refresh reports persistent Runtime failures. */ }
      finally { pending = false; }
    };
    void refreshGeneration();
    const timer = window.setInterval(() => void refreshGeneration(), taskControls.resuming ? 1500 : 800);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [api, agentRunning, taskControls.resuming, dirty, projectId]);

  useEffect(() => {
    if (!workspace) return;
    const session = editorSessionRef.current;
    if (!session || session.identity !== editorIdentity) {
      loadEditor(workspace);
      return;
    }
    const serverCandidateRevisionId = candidateRevisionId(workspace);
    if (dirty || dirtyRef.current) {
      if (serverCandidateRevisionId !== session.baseCandidateRevisionId) {
        const base = chapterMergeBase.current;
        const remote = view === "chapters" ? workspace.chapters.find(item => item.id === selectedChapter?.id)?.candidate : null;
        const merged = workspace.project.id === projectId && base && remote && base.id === session.baseCandidateRevisionId
          && !busy && !writingRef.current && !reviewDirtyRef.current && !supplementalEditor.current.current && !pendingChapterSave.current
          && base.chapterId === remote.chapterId && JSON.stringify(base.sourcePlanRefs) === JSON.stringify(remote.sourcePlanRefs)
          ? mergeChapterDraft(base, { title: draftTitle, sceneGroups: draftSceneRows.map(scene => ({ ...scene, locked: lockedSceneGroups.includes(scene.id) })) }, remote) : null;
        if (merged && remote) {
          const locked = merged.sceneGroups.filter(scene => scene.locked).map(scene => scene.id);
          const baseline = chapterEditorContent(remote.title, remote.sceneGroups, remote.sceneGroups.filter(scene => scene.locked).map(scene => scene.id));
          const changed = chapterEditorContent(merged.title, merged.sceneGroups, locked) !== baseline;
          chapterMergeBase.current = remote;
          chapterBaselineRef.current = { identity: editorIdentity, content: baseline };
          editorSessionRef.current = { ...session, baseProjectRevision: workspace.revision, baseCandidateRevisionId: remote.id, baseContent: candidateContent(workspace) };
          setDraftTitle(merged.title); setDraftSceneRows(merged.sceneGroups); setLockedSceneGroups(locked);
          dirtyRef.current = changed; setDirty(changed); setConflict(null);
          setNotice(changed ? "已合并后台保存的独立场景修改，当前正文仍未保存。" : "当前正文已与最新保存版本一致。");
        } else setConflict({ baseCandidateRevisionId: session.baseCandidateRevisionId, serverCandidateRevisionId, baseContent: session.baseContent });
      }
      return;
    }
    loadEditor(workspace);
  }, [workspace, editorIdentity]);

  useEffect(() => {
    const session = editorSessionRef.current;
    const revisionId = session?.baseCandidateRevisionId;
    if (!api || view !== "chapters" || !selectedChapter || !workspace || workspace.project.id !== projectId
      || !dirty || !conflict || !revisionId || chapterMergeBase.current || busy || writingRef.current
      || pendingChapterSave.current || reviewDirtyRef.current || supplementalEditor.current.current) return;
    let active = true;
    const owner = editorOwner;
    void api.runtime.getChapterDraftHistory(projectId, selectedChapter.id, revisionId).then(result => {
      if (!active || activeIdentityRef.current !== owner || editorSessionRef.current !== session || writingRef.current || pendingChapterSave.current) return;
      if (result.projectId !== projectId || result.revision.id !== revisionId || result.revision.chapterId !== selectedChapter.id) {
        throw new Error("历史章节归属不一致，已保留本地草稿。");
      }
      chapterMergeBase.current = result.revision;
      // Reuse the current-workspace merge path and its source/permission barriers.
      setWorkspace(current => current === workspace ? { ...current } : current);
    }).catch(reason => {
      if (active && activeIdentityRef.current === owner && editorSessionRef.current === session) {
        setError(reason instanceof Error ? reason.message : "历史章节读取失败，已保留本地草稿。");
      }
    });
    return () => { active = false; };
  }, [api, view, selectedChapter?.id, workspace, projectId, dirty, conflict, busy, editorOwner, draftTitle, draftSceneRows, lockedSceneGroups, historyRetry]);

  const chapterTitlePreviewReady = view === "chapters" && !loading && workspace?.project.id === projectId
    && chapterTitleOwnerRef.current === editorOwner && editorSessionRef.current?.identity === editorIdentity;
  useEffect(() => {
    if (!onChapterTitlePreview || !chapterTitlePreviewReady || !selectedChapter) return;
    return onChapterTitlePreview({ projectId, chapterId: selectedChapter.id, title: draftTitle });
  }, [onChapterTitlePreview, chapterTitlePreviewReady, projectId, selectedChapter?.id, draftTitle]);

  const visibleChapters = displayChapters(state.novelEdition.chapters,
    workspace?.project.id === projectId ? workspace.chapters.map(chapter => ({ id: chapter.id, title: savedChapterTitle(chapter) })) : [],
    chapterTitlePreviewReady && selectedChapter
      ? { projectId, chapterId: selectedChapter.id, title: draftTitle } : null);

  const { saveCurrent, perform, canSavePlanningForDiscussion } = useCreativeSave({ beginWrite, writingRef, taskControls, supplementalEditor, setBusy, setBusyAction, setError, setNotice, activeIdentityRef, editorOwner, reload, editorSessionRef, api, projectId, setWorkspace, setImpactCursor, setImpactCounts, setConflict, candidateRevisionId, workspace, pendingPlanningSave, lastSavedRevisionIdRef, onFormalChange, editorIdentity, view, brief, outlineNodes, setOutlineError, feedbackTarget, plans, draftTitle, draftSceneRows, lockedSceneGroups, selectedChapter, pendingChapterSave, chapterStorageKey, unreadableDrafts, busy, dirty, reviewDirty, reviewSessionKey: readReviewSessionKey, reviewUiSnapshot, reviewDirtyRef, setReviewDirty, onSaveHandlerChange, loading, agentRunning, artifactLocked, conflict, onPlanningSaveHandlerChange, dirtyRef, runningNow, onDirtyChange, onDiscardHandlerChange, discardCurrentReviewUi });
  async function resolveChapterConflict(preference: "local" | "remote") {
    const session = editorSessionRef.current, base = chapterMergeBase.current;
    if (!api || busy || writingRef.current || !session || !base || view !== "chapters" || !selectedChapter || !conflict) return;
    const owner = editorOwner;
    const expected = conflict.serverCandidateRevisionId;
    const finishWrite = beginWrite("正在整合章节冲突…");
    if (!finishWrite) return;
    try {
      const next = await api.runtime.getCreativeWorkspace(projectId);
      if (activeIdentityRef.current !== owner || editorSessionRef.current !== session) return;
      if (next.project.id !== projectId) throw new Error("读取到的作品归属不一致，本地稿仍保留。");
      const remote = next.chapters.find(item => item.id === selectedChapter.id)?.candidate;
      if (!remote || remote.id !== expected) {
        setWorkspace(next);
        throw new Error("服务器版本再次变化，请核对最新内容后重新选择。");
      }
      if (base.id !== session.baseCandidateRevisionId || base.chapterId !== remote.chapterId
        || JSON.stringify(base.sourcePlanRefs) !== JSON.stringify(remote.sourcePlanRefs)) {
        throw new Error("章节来源已变化，无法按原基线整合；本地稿仍保留。");
      }
      const merged = resolveChapterDraft(base, { title: draftTitle, sceneGroups: draftSceneRows.map(scene => ({ ...scene, locked: lockedSceneGroups.includes(scene.id) })) }, remote, preference);
      if (!merged) throw new Error("场景身份或保护状态无法安全整合，请核对后手动处理；本地稿仍保留。");
      const locked = merged.sceneGroups.filter(scene => scene.locked).map(scene => scene.id);
      const baseline = chapterEditorContent(remote.title, remote.sceneGroups, remote.sceneGroups.filter(scene => scene.locked).map(scene => scene.id));
      const changed = chapterEditorContent(merged.title, merged.sceneGroups, locked) !== baseline;
      chapterMergeBase.current = remote;
      chapterBaselineRef.current = { identity: editorIdentity, content: baseline };
      editorSessionRef.current = { ...session, baseProjectRevision: next.revision, baseCandidateRevisionId: remote.id, baseContent: candidateContent(next) };
      setDraftTitle(merged.title); setDraftSceneRows(merged.sceneGroups); setLockedSceneGroups(locked);
      dirtyRef.current = changed; setDirty(changed); setWorkspace(next); setConflict(null);
      setNotice(changed ? "已按选择整合冲突，当前正文仍未保存。" : "当前正文已与最新保存版本一致。");
    } catch (reason) {
      if (activeIdentityRef.current === owner) setError(reason instanceof Error ? reason.message : "冲突整合失败，本地稿仍保留。");
    } finally {
      finishWrite();
    }
  }

  async function saveLocalAsLatestCandidate() {
    if (!api || busy || taskControls.hasPendingRequests()) return;
    const owner = editorOwner;
    const session = editorSessionRef.current;
    if (!session || session.identity !== editorIdentity) return;
    const finishWrite = beginWrite("正在载入最新版并保存本地稿…");
    if (!finishWrite) return;
    try {
      const next = await api.runtime.getCreativeWorkspace(projectId);
      if (activeIdentityRef.current !== owner || editorSessionRef.current !== session) return;
      if (next.project.id !== projectId) throw new Error("读取到的作品归属不一致，本地稿仍保留。");
      editorSessionRef.current = rebaseEditorBaseline(session, {
        baseProjectRevision: next.revision,
        baseCandidateRevisionId: candidateRevisionId(next),
        baseContent: candidateContent(next),
      });
      setWorkspace((current) => current ? { ...next, impacts: current.impacts, openImpactCount: current.openImpactCount } : next);
      setConflict(null);
      finishWrite();
      await saveCurrent(next);
    } catch (reason) {
      if (activeIdentityRef.current === owner) setError(reason instanceof Error ? reason.message : "无法基于服务器最新版保存本地稿");
    } finally {
      finishWrite();
    }
  }

  async function loadMoreImpacts() {
    if (!api || !workspace || !impactCursor || busy || taskControls.hasPendingRequests()) return;
    const finishWrite = beginWrite("正在读取更多规划反馈…");
    if (!finishWrite) return;
    try {
      const page = await api.runtime.listPlanningImpacts(projectId, impactCursor, 25);
      if (activeIdentityRef.current !== editorOwner) return;
      setWorkspace((current) => current ? { ...current, impacts: [...current.impacts, ...page.data.filter((item) => !current.impacts.some((existing) => existing.id === item.id))] } : current);
      setImpactCursor(page.nextCursor);
      setImpactCounts(page.counts);
    } catch (reason) {
      if (activeIdentityRef.current === editorOwner) setError(reason instanceof Error ? reason.message : "规划反馈加载失败");
    } finally {
      finishWrite();
    }
  }

  function beginWrite(label: string) {
    if (activeIdentityRef.current !== editorOwner || writingRef.current || taskControls.hasPendingRequests()) return null;
    const operation = {};
    writeOperationRef.current = operation;
    writingRef.current = true; setBusy(true); setBusyAction(label); setError(null);
    return () => {
      if (writeOperationRef.current !== operation) return;
      writeOperationRef.current = null;
      writingRef.current = false; setBusy(false); setBusyAction("");
    };
  }
  function discardDraft() {
    if (chapterStorageKey) {
      try { localStorage.removeItem(chapterStorageKey); unreadableDrafts.current.delete(chapterStorageKey); }
      catch { setError("无法清理本地草稿，尚未放弃修改。"); return false; }
    }
    if (workspace) loadEditor(workspace);
    return true;
  }
  useEffect(() => { dirtyRef.current = dirty; }, [dirty]);
  return { workspace, dirty, loading, busy, busyAction, planningDraft, draftTitle, draftSceneRows, lockedSceneGroups, conflict, editorIdentity, editorOwner, visibleChapters, impactCursor, impactCounts, saveCurrent, perform, canSavePlanningForDiscussion, reload, loadEditor, loadMoreImpacts, resolveChapterConflict, saveLocalAsLatestCandidate,
    isCurrent: (owner: object) => activeIdentityRef.current === owner,
    isRunning: () => runningNow.current,
    isDirty: () => dirtyRef.current,
    isWriting: () => writingRef.current,
    sessionReady: () => editorSessionRef.current?.identity === editorIdentity,
    savedRevision: () => lastSavedRevisionIdRef.current,
    hasPendingSave: pendingChapterSave.current?.scope === editorIdentity,
    canResolveConflict: Boolean(chapterMergeBase.current),
    beginWrite, discardDraft,
    changeTitle(title: string) { setDraftTitle(title); updateChapterDirty(title, draftSceneRows, lockedSceneGroups); },
    changeScenes(scenes: typeof draftSceneRows) { setDraftSceneRows(scenes); updateChapterDirty(draftTitle, scenes, lockedSceneGroups); },
    changeLocks(locked: string[]) { setLockedSceneGroups(locked); updateChapterDirty(draftTitle, draftSceneRows, locked); },
    restoreEmptyChapterTitle(title: string) { setDraftTitle(title); dirtyRef.current = false; setDirty(false); },
    async retryLoad() {
      setLoading(true); setError(null);
      try { await reload(); }
      catch (reason) { if (activeIdentityRef.current === editorOwner) setError(reason instanceof Error ? reason.message : "工作区加载失败"); }
      finally { if (activeIdentityRef.current === editorOwner) setLoading(false); }
    },
    retryHistory() { setError(null); setHistoryRetry(value => value + 1); },
    loadSavedWorkspace(next: CreativeWorkspace) { setWorkspace(next); loadEditor(next); },
    receiveCreatedChapter(next: CreativeWorkspace, impacts: Awaited<ReturnType<NovelCodexApi["runtime"]["listPlanningImpacts"]>>) {
      setWorkspace({ ...next, impacts: impacts.data, openImpactCount: impacts.counts.open });
      setImpactCursor(impacts.nextCursor); setImpactCounts(impacts.counts); setLoading(false);
    },
  };
}

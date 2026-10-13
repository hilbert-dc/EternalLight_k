import { useEffect, useMemo, useReducer, useRef } from "react";
import type { Chapter, CreativeWorkspace, NovelCodexApi } from "./types";
import type { SidebarChapterCreationRequest } from "./SidebarWorkDirectory";

type Receipt = { projectId: string; chapterId: string };
type State = {
  open: boolean;
  sequence: number;
  target: SidebarChapterCreationRequest | null;
  pending: (Receipt & { error: string }) | null;
  submitting: boolean;
};
type Action =
  | { type: "prepare"; request: SidebarChapterCreationRequest }
  | { type: "close" }
  | { type: "start" }
  | { type: "settle" }
  | { type: "recover"; receipt: Receipt; error: string }
  | { type: "complete" };
function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "prepare": return { ...state, open: true, sequence: state.sequence + 1, target: action.request };
    case "close": return { ...state, open: false };
    case "start": return { ...state, submitting: true };
    case "settle": return { ...state, submitting: false };
    case "recover": return { ...state, pending: { ...action.receipt, error: action.error } };
    case "complete": return { ...state, pending: null, open: false };
  }
}

export type CreatedChapterReadback = {
  chapter: Chapter;
  workspace: CreativeWorkspace;
  impacts: Awaited<ReturnType<NovelCodexApi["runtime"]["listPlanningImpacts"]>>;
};

/** One create receipt owns recovery: retry reads it; it never repeats the mutation. */
export function useChapterCreation({ api, projectId, view, workspace, request, canPrepare, blocked,
  onPrepared, beginWrite, onFormalChange, onReadback, onError }: {
  api: NovelCodexApi | undefined;
  projectId: string;
  view: string;
  workspace: CreativeWorkspace | null;
  request?: SidebarChapterCreationRequest | null;
  canPrepare: boolean;
  blocked: boolean;
  onPrepared?: (requestId: string) => void;
  beginWrite: () => (() => void) | null;
  onFormalChange: () => Promise<void>;
  onReadback: (result: CreatedChapterReadback) => void;
  onError: (message: string) => void;
}) {
  const [state, dispatch] = useReducer(reducer, { open: false, sequence: 0, target: null, pending: null, submitting: false });
  const consumed = useRef<string | null>(null);
  const receipt = useRef<Receipt | null>(null);
  const submission = useRef<object | null>(null);
  const owner = useMemo(() => ({}), [api, projectId, view]);
  const currentOwner = useRef<object | null>(owner); currentOwner.current = owner;
  useEffect(() => { currentOwner.current = owner; return () => { currentOwner.current = null; }; }, [owner]);
  useEffect(() => {
    if (!request || request.projectId !== projectId || view !== "chapters" || !canPrepare
      || state.pending?.projectId === projectId || state.submitting || workspace?.project.id !== projectId
      || consumed.current === request.requestId) return;
    consumed.current = request.requestId;
    dispatch({ type: "prepare", request });
    onPrepared?.(request.requestId);
  }, [request, projectId, view, canPrepare, workspace, onPrepared, state.pending, state.submitting]);

  async function create(title?: string): Promise<boolean> {
    if (!api || !workspace || blocked || submission.current || currentOwner.current !== owner) return false;
    const release = beginWrite();
    if (!release) return false;
    const operation = {}; submission.current = operation;
    const isCurrent = () => currentOwner.current === owner;
    dispatch({ type: "start" });
    try {
      let saved = receipt.current?.projectId === projectId ? receipt.current : null;
      if (!saved) {
        if (title === undefined) return false;
        const created = await api.runtime.materializeStandaloneChapter(projectId, title.trim(), workspace.revision,
          state.target?.projectId === projectId ? state.target.volumeId : null);
        if (!isCurrent()) return false;
        saved = { projectId, chapterId: created.chapterId };
        receipt.current = saved;
      }
      await onFormalChange();
      const [next, nextWorkspace, impacts] = await Promise.all([
        api.runtime.getState(projectId), api.runtime.getCreativeWorkspace(projectId),
        api.runtime.listPlanningImpacts(projectId, null, 25),
      ]);
      if (!isCurrent()) return false;
      const chapter = next.novelEdition.chapters.find(item => item.id === saved.chapterId);
      if (next.project.id !== projectId || nextWorkspace.project.id !== projectId
        || !chapter || !nextWorkspace.chapters.some(item => item.id === saved.chapterId)) {
        throw new Error("已创建章节尚未出现在工作区，请重新读取。");
      }
      onReadback({ chapter, workspace: nextWorkspace, impacts });
      receipt.current = null;
      dispatch({ type: "complete" });
      return true;
    } catch (reason) {
      if (!isCurrent()) return false;
      const message = reason instanceof Error ? reason.message : "章节创建失败";
      const saved = receipt.current;
      if (saved?.projectId === projectId) dispatch({ type: "recover", receipt: saved, error: message });
      else onError(message);
      return false;
    } finally {
      if (submission.current === operation) { submission.current = null; dispatch({ type: "settle" }); }
      release();
    }
  }
  return {
    open: state.open && state.target?.projectId === projectId,
    requestId: state.sequence,
    volumeTitle: state.target?.projectId === projectId ? state.target.volumeTitle ?? undefined : undefined,
    pending: state.pending?.projectId === projectId ? state.pending : null,
    submitting: state.submitting,
    create,
    recover: () => create(),
    close: () => dispatch({ type: "close" }),
  };
}

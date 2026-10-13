import { useEffect, useMemo, useReducer, useRef } from "react";
import type { CreativeEdition, MediaProject } from "../../shared/multimedia-types";
import type { NovelCodexApi, RuntimeState } from "../types";
import type { MediaEditionSource } from "./CreateMediaEditionDialog";
import { rememberSelectedEdition } from "./edition-selection";

type State = { context: { project: MediaProject; edition: CreativeEdition } | null; busy: boolean;
  catalog: { projectId: string; editions: CreativeEdition[]; selectedId: string | null };
  creating: boolean; source: MediaEditionSource | null; error: string | null };
type Action = { type: "context"; context: State["context"] } | { type: "catalog"; catalog: State["catalog"] }
  | { type: "opened"; context: State["context"]; catalog: State["catalog"] }
  | { type: "refresh"; projectId: string; editions: CreativeEdition[] }
  | { type: "busy"; busy: boolean } | { type: "create"; open: boolean }
  | { type: "source"; source: MediaEditionSource | null } | { type: "error"; error: string | null } | { type: "clearTransient" };
function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "context": return { ...state, context: action.context };
    case "catalog": return { ...state, catalog: action.catalog };
    case "opened": return { ...state, context: action.context, catalog: action.catalog, error: null, source: null };
    case "refresh": return action.projectId === state.catalog.projectId ? { ...state, catalog: { ...state.catalog, editions: action.editions } } : state;
    case "busy": return { ...state, busy: action.busy };
    case "create": return { ...state, creating: action.open };
    case "source": return { ...state, source: action.source };
    case "error": return { ...state, error: action.error };
    case "clearTransient": return { ...state, source: null, error: null };
  }
}
export function useEditionNavigation({ api, projectId, authorKey, onBegin, onLoaded, onEnd }: {
  api?: NovelCodexApi; projectId: string; authorKey: string;
  onBegin: () => void; onLoaded: (novel: RuntimeState | null) => void; onEnd: () => void;
}) {
  const [state, dispatch] = useReducer(reducer, { context: null, busy: false, catalog: { projectId: "", editions: [], selectedId: null }, creating: false, source: null, error: null });
  const owner = useMemo(() => ({}), [api, projectId, authorKey]);
  const currentOwner = useRef(owner); currentOwner.current = owner;
  const opening = useRef(false), epoch = useRef(0);
  // Startup polling and explicit selection share this epoch; neither can replace
  // an edition opened by the other with an older response.
  useEffect(() => { currentOwner.current = owner; epoch.current++; opening.current = false; return () => { if (currentOwner.current === owner) currentOwner.current = {}; }; }, [owner]);
  const reportError = (error: string | null) => dispatch({ type: "error", error });
  async function open(requested: CreativeEdition) {
    if (!api || opening.current || requested.projectId !== projectId) throw new Error("当前无法切换作品版本。");
    opening.current = true; const ticket = ++epoch.current;
    onBegin(); reportError(null);
    const current = () => currentOwner.current === owner && ticket === epoch.current;
    try {
      const { editions } = await api.multimedia.listEditions(projectId);
      if (!current()) return;
      const edition = editions.find(item => item.id === requested.id && item.projectId === requested.projectId && item.scopeId === requested.scopeId && item.branchId === requested.branchId && item.medium === requested.medium);
      if (!edition) throw new Error("作品版本归属已变化，请重新读取。");
      const media = edition.medium !== "novel" ? await api.multimedia.readWorkspace(projectId, edition.id, edition.branchId) : null;
      const novel = edition.medium === "novel" ? await api.runtime.getState(projectId) : null;
      if (!current()) return;
      if (media && (media.edition.id !== edition.id || media.project.id !== projectId)) throw new Error("作品版本回执归属不匹配。");
      if (novel && novel.project.id !== projectId) throw new Error("作品版本回执归属不匹配。");
      onLoaded(novel);
      dispatch({ type: "opened", context: media ? { project: media.project, edition: media.edition } : null,
        catalog: { projectId, editions, selectedId: edition.id } });
      rememberSelectedEdition(authorKey, projectId, edition.id);
    } finally { if (current()) { opening.current = false; onEnd(); } }
  }
  async function refresh() {
    if (!api || !projectId) return;
    try { const { editions } = await api.multimedia.listEditions(projectId); if (currentOwner.current === owner) dispatch({ type: "refresh", projectId, editions }); }
    catch { if (currentOwner.current === owner) reportError("作品版本列表暂未刷新，请稍后重试。"); }
  }
  return { ...state, opening, epoch, open, refresh, reportError,
    presentContext: (context: State["context"]) => dispatch({ type: "context", context }),
    receiveCatalog: (catalog: State["catalog"]) => dispatch({ type: "catalog", catalog }),
    reportBusy: (busy: boolean) => dispatch({ type: "busy", busy }),
    showCreator: (open: boolean) => dispatch({ type: "create", open }),
    useSource: (source: MediaEditionSource | null) => dispatch({ type: "source", source }),
    clearTransient: () => dispatch({ type: "clearTransient" }),
  };
}

import { mergeMediaDraft } from "./merge-media-draft";
import { useCallback, useEffect, useRef, useState } from "react";
import type { MediaAsset, MediaExportResult, MediaSelection, MediaSelector, MultimediaApi, MultimediaDocument, MultimediaWorkspace } from "../../shared/multimedia-types";
import type { MediaEditionSource } from "./CreateMediaEditionDialog";
import { AnimationEditor } from "./AnimationEditor";
import { ComicEditor } from "./ComicEditor";
import { MediaPreview } from "./MediaPreview";
import { MediaSourcesPanel } from "./MediaSourcesPanel";
import { mediaLabel, newMediaId, sameDocument } from "./editor-helpers";
import "./multimedia.css";

export interface MultimediaWorkspaceViewProps {
  api: MultimediaApi; authorKey: string; projectId: string; editionId: string; busy?: boolean; refreshToken?: number;
  onDirtyChange?: (dirty: boolean) => void;
  onSaveHandlerChange?: (handler: (() => Promise<boolean>) | null) => void;
  onDiscardHandlerChange?: (handler: (() => boolean) | null) => void;
  onSelectionChange?: (selection: MediaSelection | null) => void;
  onSourceSelectionChange?: (source: MediaEditionSource | null) => void;
}
type LocalDraft = { baseRevisionId: string | null; document: MultimediaDocument; saveAttempt?: { fingerprint: string; key: string } };
const friendlyError = (reason: unknown) => reason instanceof Error ? reason.message.replace(/^Error invoking remote method '[^']+':\s*(?:Error:\s*)?/, "") : "操作未完成，请重试。";
const draftKey = (authorKey: string, projectId: string, editionId: string) => `novel-codex:media-editor-draft:${JSON.stringify([authorKey, projectId, editionId])}`;
function readDraft(key: string): LocalDraft | null {
  try { const parsed = JSON.parse(localStorage.getItem(key) ?? "null"); return parsed && (parsed.document?.kind === "comic" || parsed.document?.kind === "animation") && (parsed.baseRevisionId === null || typeof parsed.baseRevisionId === "string") ? parsed : null; } catch { return null; }
}
function clearDraft(key: string) { try { localStorage.removeItem(key); } catch { /* The durable save receipt remains authoritative. */ } }
function mergeAssets(left: MediaAsset[], right: MediaAsset[]) { return [...new Map([...left, ...right].map(asset => [asset.assetRevisionId, asset])).values()]; }

export function MultimediaWorkspaceView(props: MultimediaWorkspaceViewProps) {
  return <MultimediaWorkspaceSession key={JSON.stringify([props.authorKey, props.projectId, props.editionId])} {...props} />;
}
function MultimediaWorkspaceSession({ api, authorKey, projectId, editionId, busy = false, refreshToken = 0, ...callbacks }: MultimediaWorkspaceViewProps) {
  const [workspace, setWorkspace] = useState<MultimediaWorkspace | null>(null);
  const [document, setDocument] = useState<MultimediaDocument | null>(null);
  const [expectedRevisionId, setExpectedRevisionId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true), [pending, setPending] = useState<"save" | "import" | "export" | null>(null);
  const [error, setError] = useState<string | null>(null), [notice, setNotice] = useState<string | null>(null);
  const [libraryOpen, setLibraryOpen] = useState(false), [dragging, setDragging] = useState(false), [sourcesOpen, setSourcesOpen] = useState(false);
  const [result, setResult] = useState<MediaExportResult | null>(null);
  const [selection, setSelection] = useState<{ label: string; assetRevisionId: string | null; selector: MediaSelector } | null>(null);
  const alive = useRef(true), request = useRef(0), operation = useRef(false), restored = useRef(false);
  const base = useRef<MultimediaWorkspace | null>(null);
  const saveAttempt = useRef<{ fingerprint: string; key: string } | undefined>(undefined);
  const handlers = useRef(callbacks); handlers.current = callbacks;
  const current = useRef({ workspace, document, expectedRevisionId, busy }); current.current = { workspace, document, expectedRevisionId, busy };
  const key = draftKey(authorKey, projectId, editionId);
  const viewStorageKey = `novel-codex:media-editor-view:${JSON.stringify([authorKey, projectId, editionId])}`;
  const dirty = Boolean(workspace && document && (!sameDocument(workspace.document, document) || expectedRevisionId !== workspace.revisionId));
  const dirtyRef = useRef(dirty); dirtyRef.current = dirty;
  useEffect(() => { alive.current = true; return () => { alive.current = false; request.current++; handlers.current.onDirtyChange?.(false); handlers.current.onSelectionChange?.(null); handlers.current.onSourceSelectionChange?.(null); }; }, []);
  const load = useCallback(async () => {
    if (operation.current) return;
    const ticket = ++request.current; setLoading(true); setError(null);
    try {
      const value = await api.readWorkspace(projectId, editionId);
      if (!alive.current || ticket !== request.current || operation.current) return;
      if (value.edition.projectId !== projectId || value.edition.id !== editionId || value.project.id !== projectId) throw new Error("读取到的作品归属不一致，尚未打开。");
      const previous = current.current.workspace;
      if (previous && (previous.edition.scopeId !== value.edition.scopeId || previous.edition.branchId !== value.edition.branchId)) throw new Error("作品范围已变化，当前编辑仍保留。");
      const draft = !restored.current ? readDraft(key) : null; restored.current = true;
      if (!dirtyRef.current) {
        if (draft && draft.document.kind === value.document.kind && !sameDocument(draft.document, value.document)) {
          setDocument(draft.document); setExpectedRevisionId(draft.baseRevisionId); saveAttempt.current = draft.saveAttempt;
          base.current = draft.baseRevisionId === value.revisionId ? value : null;
          setNotice(base.current ? "已恢复尚未保存的修改。" : "已恢复本地修改，保存版本已有变化。请核对后再保存。");
        } else { setDocument(value.document); setExpectedRevisionId(value.revisionId); base.current = value; }
      } else if (current.current.expectedRevisionId !== value.revisionId) {
        const original = base.current, local = current.current.document;
        const sameSources = original && JSON.stringify([original.sources, original.edition.sources, original.edition.canonSnapshotId]) === JSON.stringify([value.sources, value.edition.sources, value.edition.canonSnapshotId]);
        const merged = original && local && sameSources && original.revisionId === current.current.expectedRevisionId
          ? mergeMediaDraft(original.document, local, value.document) : null;
        if (merged) {
          setDocument(merged); setExpectedRevisionId(value.revisionId); base.current = value; saveAttempt.current = undefined;
          setNotice(sameDocument(merged, value.document) ? "已与最新保存版本一致。" : "已合并后台保存的独立修改，当前编辑仍未保存。");
        } else setNotice("作品已有新保存版本，修改存在冲突或原版本无法核对。当前编辑已保留，请核对后再保存。");
      }
      setWorkspace(value);
    } catch (reason) { if (alive.current && ticket === request.current) setError(friendlyError(reason)); }
    finally { if (alive.current && ticket === request.current) setLoading(false); }
  }, [api, projectId, editionId, key]);
  useEffect(() => { void load(); }, [load, refreshToken]);
  useEffect(() => {
    handlers.current.onDirtyChange?.(dirty);
    if (!document || !workspace) return;
    try { if (dirty) localStorage.setItem(key, JSON.stringify({ baseRevisionId: expectedRevisionId, document, saveAttempt: saveAttempt.current })); else localStorage.removeItem(key); }
    catch { setNotice("本地恢复空间不足，请保存当前修改后再离开。"); }
  }, [dirty, document, workspace, expectedRevisionId, key]);
  useEffect(() => {
    if (!workspace || !selection) { handlers.current.onSelectionChange?.(null); return; }
    handlers.current.onSelectionChange?.({ scope: { projectId, editionId, scopeId: workspace.edition.scopeId, branchId: workspace.edition.branchId }, revisionId: dirty ? null : workspace.revisionId, label: `${dirty ? "未保存 · " : ""}${selection.label}`, assetRevisionId: selection.assetRevisionId, selector: selection.selector });
  }, [workspace, selection, dirty, projectId, editionId]);
  useEffect(() => {
    const source = workspace?.documentSource;
    const currentSource = source && source.projectId === projectId && source.scopeId === workspace?.edition.scopeId && source.branchId === workspace?.edition.branchId && source.objectId === editionId && source.revisionId === workspace?.revisionId && source.selector.kind === "whole";
    handlers.current.onSourceSelectionChange?.(currentSource && !dirty ? { label: selection?.label ?? `${workspace.edition.title}（已保存内容）`, refs: [{ ...source, selector: selection?.selector ?? source.selector }] } : null);
  }, [workspace, selection, dirty, projectId, editionId]);
  const save = useCallback(async (): Promise<boolean> => {
    const state = current.current; if (!state.workspace || !state.document || operation.current || state.busy) return false;
    if (!dirtyRef.current && state.workspace.revisionId) return true;
    operation.current = true; request.current++; setLoading(false); setPending("save"); setError(null);
    try {
      const fingerprint = JSON.stringify({ expectedRevisionId: state.expectedRevisionId, document: state.document, sources: state.workspace.sources });
      if (saveAttempt.current?.fingerprint !== fingerprint) saveAttempt.current = { fingerprint, key: newMediaId("save") };
      try { localStorage.setItem(key, JSON.stringify({ baseRevisionId: state.expectedRevisionId, document: state.document, saveAttempt: saveAttempt.current })); } catch { /* Saving still works when browser recovery storage is full. */ }
      const value = await api.saveDocument(projectId, editionId, { expectedRevisionId: state.expectedRevisionId, document: state.document, idempotencyKey: saveAttempt.current.key, readSet: [], sources: state.workspace.sources });
      if (!alive.current) return false;
      if (value.edition.id !== editionId || value.project.id !== projectId || !value.revisionId) throw new Error("未收到当前作品的保存回执。");
      base.current = value; setWorkspace(value); setDocument(value.document); setExpectedRevisionId(value.revisionId); setNotice(null); setResult(null); saveAttempt.current = undefined; clearDraft(key); return true;
    } catch (reason) { if (alive.current) setError(friendlyError(reason)); return false; }
    finally { operation.current = false; if (alive.current) setPending(null); }
  }, [api, projectId, editionId, key]);
  const discard = useCallback(() => { if (operation.current || current.current.busy || !current.current.workspace) return false; const latest = current.current.workspace; base.current = latest; setDocument(latest.document); setExpectedRevisionId(latest.revisionId); setNotice(null); setError(null); saveAttempt.current = undefined; clearDraft(key); return true; }, [key]);
  useEffect(() => { handlers.current.onSaveHandlerChange?.(save); handlers.current.onDiscardHandlerChange?.(discard); return () => { handlers.current.onSaveHandlerChange?.(null); handlers.current.onDiscardHandlerChange?.(null); }; }, [save, discard]);
  async function importFiles(files?: File[]) {
    if (operation.current || busy || !workspace) return;
    operation.current = true; request.current++; setLoading(false); setPending("import"); setError(null);
    try {
      const bytes = files ? await Promise.all(files.map(async file => ({ name: file.name, bytes: new Uint8Array(await file.arrayBuffer()) }))) : undefined;
      if (!alive.current) return;
      const value = await api.importFiles(projectId, editionId, bytes);
      if (!alive.current) return;
      if (value.assets.some(asset => asset.projectId !== projectId || asset.scopeId !== workspace.edition.scopeId)) throw new Error("导入素材归属不一致，尚未加入作品。");
      setWorkspace(previous => previous ? { ...previous, assets: mergeAssets(previous.assets, value.assets) } : previous);
      if (value.assets.length) { setLibraryOpen(true); setNotice(`已导入 ${value.assets.length} 个素材。选中画格或片段后即可使用。`); }
    } catch (reason) { if (alive.current) setError(friendlyError(reason)); }
    finally { operation.current = false; if (alive.current) setPending(null); }
  }
  async function exportDocument(format: "png" | "pdf" | "mp4" | "project") {
    if (operation.current || busy || dirty || !workspace?.revisionId) return;
    operation.current = true; setPending("export"); setError(null);
    try {
      const value = await api.exportEdition(projectId, editionId, { expectedRevisionId: workspace.revisionId, format });
      if (!alive.current) return;
      if (value.editionId !== editionId || value.revisionId !== workspace.revisionId) throw new Error("导出回执与当前保存版本不一致。");
      setResult(value);
    } catch (reason) { if (alive.current) setError(friendlyError(reason)); }
    finally { operation.current = false; if (alive.current) setPending(null); }
  }
  async function openExport() { if (!result) return; try { const value = await api.openExport(projectId, result.id); if (alive.current && !value.opened) setError("文件尚未打开，请重试。"); } catch (reason) { if (alive.current) setError(friendlyError(reason)); } }
  async function adoptCanon(canonSnapshotId: string) {
    if (operation.current || busy || dirty || !workspace) return;
    operation.current = true; request.current++; setLoading(false); setPending("save"); setError(null);
    const fingerprint = JSON.stringify({ revisionId: workspace.revisionId, canonSnapshotId });
    if (saveAttempt.current?.fingerprint !== fingerprint) saveAttempt.current = { fingerprint, key: newMediaId("adopt-canon") };
    try {
      const value = await api.patchDocument(projectId, editionId, { expectedRevisionId: workspace.revisionId, idempotencyKey: saveAttempt.current.key, operations: [{ kind: "adoptCanonSnapshot", canonSnapshotId }], readSet: [], sources: [] });
      if (!alive.current) return;
      if (value.project.id !== projectId || value.edition.id !== editionId || !value.revisionId || value.edition.canonSnapshotId !== canonSnapshotId) throw Error("未收到当前作品采用设定的回执。");
      base.current = value; setWorkspace(value); setDocument(value.document); setExpectedRevisionId(value.revisionId); setNotice("已保存使用新设定的作品版本。原版本保留。"); setResult(null); saveAttempt.current = undefined;
    } catch (reason) { if (alive.current) setError(friendlyError(reason)); throw reason; }
    finally { operation.current = false; if (alive.current) setPending(null); }
  }
  const disabled = busy || Boolean(pending) || loading;
  return <section className={`multimedia-workspace${dragging ? " mm-dragging" : ""}`} aria-label={document ? `${mediaLabel(document)}创作` : "作品创作"}
    onDragOver={event => { if (event.dataTransfer.types.includes("Files")) { event.preventDefault(); setDragging(true); } }} onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDragging(false); }}
    onDrop={event => { event.preventDefault(); setDragging(false); if (event.dataTransfer.files.length) void importFiles([...event.dataTransfer.files]); }}
    onPaste={event => { const files = [...event.clipboardData.files]; if (files.length) { event.preventDefault(); void importFiles(files); } }}>
    <header className="mm-workspace-header"><div><span className="mm-medium-label">{document ? mediaLabel(document) : "作品"}</span><h2>{workspace?.edition.title ?? "正在打开…"}</h2></div><div className="mm-inline-actions"><button type="button" disabled={disabled || !workspace} onClick={() => void importFiles()}>导入素材</button><button type="button" aria-expanded={libraryOpen} onClick={() => setLibraryOpen(value => !value)} disabled={!workspace?.assets.length}>素材{workspace?.assets.length ? ` ${workspace.assets.length}` : ""}</button><details className="mm-export-menu"><summary aria-disabled={disabled || dirty || !workspace?.revisionId}>导出</summary><div>{(document?.kind === "comic" ? ["png", "pdf", "project"] as const : ["mp4", "project"] as const).map(format => <button type="button" key={format} disabled={disabled || dirty || !workspace?.revisionId} onClick={event => { event.currentTarget.closest("details")?.removeAttribute("open"); void exportDocument(format); }}>{({ png: "页面图片", pdf: "PDF", mp4: "动画 MP4", project: "可编辑项目" })[format]}</button>)}</div></details><button type="button" className="primary-button" disabled={disabled || !document || (!dirty && Boolean(workspace?.revisionId))} onClick={() => void save()}>{pending === "save" ? "保存中…" : "保存"}</button></div></header>
    <div className="mm-status-row" role="status">{pending === "import" ? "正在导入素材…" : pending === "export" ? "正在生成导出文件…" : loading ? "正在读取作品…" : dirty ? "有未保存修改" : workspace?.revisionId ? "已保存" : "尚未保存"}</div>
    {error && <div className="mm-error" role="alert">{error}{!workspace && <button type="button" onClick={() => void load()}>重新读取</button>}</div>}
    {notice && <p className="mm-notice" role="status">{notice}</p>}
    {result && <div className="mm-export-result"><span>已导出 {result.name}</span><button type="button" onClick={() => void openExport()}>打开文件</button></div>}
    {libraryOpen && workspace && <section className="mm-asset-library" aria-label="作品素材"><div className="mm-inspector-heading"><h3>作品素材</h3><button type="button" onClick={() => setLibraryOpen(false)}>收起</button></div><div className="mm-asset-grid">{workspace.assets.map(asset => <article key={asset.assetRevisionId}><MediaPreview asset={asset} /><p>{asset.name}</p></article>)}</div></section>}
    {workspace && <div className="mm-source-entry"><button type="button" className="text-button" aria-expanded={sourcesOpen} onClick={() => setSourcesOpen(value => !value)}>设定与来源{workspace.edition.sources.length ? ` · ${workspace.edition.sources.length} 处改编来源` : ""}</button></div>}
    {workspace && sourcesOpen && <MediaSourcesPanel api={api} authorKey={authorKey} workspace={workspace} busy={disabled || dirty} source={!dirty && workspace.documentSource ? { ...workspace.documentSource, selector: selection?.selector ?? workspace.documentSource.selector } : null} onAdopt={adoptCanon} />}
    {workspace && document && (document.kind === "comic" ? <ComicEditor viewStorageKey={viewStorageKey} document={document} assets={workspace.assets} disabled={disabled} onChange={setDocument} onSelection={setSelection} onImport={() => void importFiles()} /> : <AnimationEditor viewStorageKey={viewStorageKey} document={document} assets={workspace.assets} disabled={disabled} onChange={setDocument} onSelection={setSelection} onImport={() => void importFiles()} readSubtitleFont={api.readSubtitleFont} />)}
  </section>;
}

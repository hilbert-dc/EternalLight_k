import { useEffect, useRef, useState } from "react";
import type { CreativeEdition, CreativeMedium, MediaSourceRef, MultimediaApi } from "../../shared/multimedia-types";
import { AdaptationSourcePicker, type SelectedAdaptationSource } from "./AdaptationSourcePicker";
import { mediaSourceIdentity } from "./source-identity";
import { newMediaId } from "./editor-helpers";
const mediaLabel = (medium: string) => ({ novel: "小说", comic: "漫画", animation: "动画" })[medium] ?? medium;
import "./edition-switcher.css";

export interface MediaEditionSource { label: string; refs: MediaSourceRef[] }
type CreateRequest = Parameters<MultimediaApi["createEdition"]>[1];
type Props = {
  api: MultimediaApi; authorKey: string; projectId: string; source: MediaEditionSource | null;
  onClose: () => void; onCreated: (edition: CreativeEdition) => Promise<void>;
};
const pendingKey = (author: string, project: string) => `novel-codex:pending-edition:${JSON.stringify([author, project])}`;
function readPending(key: string, projectId: string): CreateRequest | null {
  const raw = localStorage.getItem(key);
  if (raw === null) return null;
  const value = JSON.parse(raw);
  if (!value || typeof value.title !== "string" || !["comic", "animation"].includes(value.medium)
    || typeof value.idempotencyKey !== "string" || !value.idempotencyKey || !Array.isArray(value.sources)
    || value.sources.length > 100 || value.sources.some((s: MediaSourceRef) => !s || s.projectId !== projectId
      || !s.scopeId || !s.branchId || !s.objectId || !s.revisionId || !s.digest || !s.selector)
    || Object.keys(value).some(key => !["title", "medium", "sources", "idempotencyKey"].includes(key))) {
    throw Error("待核对的版本建立记录不可读取，已保留记录，未发送新的建立请求。");
  }
  return value;
}
export function CreateMediaEditionDialog(props: Props) {
  return <EditionCreation key={pendingKey(props.authorKey, props.projectId)} {...props} />;
}
function EditionCreation({ api, authorKey, projectId, source, onClose, onCreated }: Props) {
  const storageKey = pendingKey(authorKey, projectId);
  const [initial] = useState(() => {
    try { return { request: readPending(storageKey, projectId), error: null }; }
    catch { return { request: null, error: "无法读取待核对的建立请求。请恢复本地存储后重试，原记录不会被覆盖。" }; }
  });
  const [frozenSource, setFrozenSource] = useState<MediaEditionSource | null>(null);
  const availableSource = frozenSource ?? source;
  const [medium, setMedium] = useState<Extract<CreativeMedium, "comic" | "animation">>(initial.request?.medium === "animation" ? "animation" : "comic");
  const [selected, setSelected] = useState<SelectedAdaptationSource[]>(initial.request?.sources.map((source, i) => ({ source, label: `已固定来源 ${i + 1}` })) ?? []);
  const [submitted, setSubmitted] = useState(Boolean(initial.request));
  const created = useRef<CreativeEdition | null>(null);
  const [title, setTitle] = useState(initial.request?.title ?? "");
  const [useSource, setUseSource] = useState(false);
  const [error, setError] = useState<string | null>(initial.error), [saving, setSaving] = useState(false);
  const pending = useRef(false), mounted = useRef(true), form = useRef<HTMLFormElement>(null);
  const attempt = useRef<CreateRequest | null>(initial.request);
  useEffect(() => { mounted.current = true; const previous = document.activeElement as HTMLElement | null; form.current?.querySelector<HTMLInputElement>("input")?.focus(); return () => { mounted.current = false; if (previous?.isConnected) previous.focus({ preventScroll: true }); }; }, []);
  async function submit() {
    if (pending.current) return;
    const name = title.trim() || `${mediaLabel(medium)}版`;
    if (name.length > 120) { setError("版本名称不能超过 120 个字符。"); return; }
    const sources = [...(useSource ? availableSource?.refs ?? [] : []), ...selected.map(item => item.source)].filter((item, index, all) => all.findIndex(other => mediaSourceIdentity(other) === mediaSourceIdentity(item)) === index);
    if (sources.length > 100) { setError("一次最多选择 100 处来源，请减少选择。"); return; }
    if (useSource && !frozenSource && availableSource) setFrozenSource(structuredClone(availableSource));
    if (useSource && !availableSource?.refs.length) { setError("来源尚未保存，请回到作品保存后重新选择。"); return; }
    const request: CreateRequest = attempt.current ?? { title: name, medium, sources, idempotencyKey: newMediaId("create-edition") };
    try {
      const existing = readPending(storageKey, projectId);
      if (existing && JSON.stringify(existing) !== JSON.stringify(request)) throw Error("已有待核对的建立请求，请重新打开此窗口继续原请求。");
      localStorage.setItem(storageKey, JSON.stringify(request));
      attempt.current = request;
    } catch (reason) {
      setError(reason instanceof Error ? `无法保存建立请求：${reason.message}` : "无法保存建立请求，请恢复本地存储后重试。");
      return;
    }
    pending.current = true; setSubmitted(true); setSaving(true); setError(null);
    try {
      const edition = created.current ?? await api.createEdition(projectId, request);
      if (!mounted.current) return;
      if (edition.projectId !== projectId || edition.medium !== request.medium || !edition.id || edition.editionId !== edition.id || !edition.scopeId || !edition.branchId || JSON.stringify(edition.sources.map(mediaSourceIdentity)) !== JSON.stringify(request.sources.map(mediaSourceIdentity))) throw new Error("新版本回执与所选形式或来源不一致，请重新读取作品。");
      created.current = edition;
      await onCreated(edition);
      const stored = readPending(storageKey, projectId);
      if (stored?.idempotencyKey === request.idempotencyKey) localStorage.removeItem(storageKey);
      if (mounted.current) onClose();
    } catch (reason) { if (mounted.current) setError(reason instanceof Error ? reason.message : "作品版本未建立，请重试。"); }
    finally { pending.current = false; if (mounted.current) setSaving(false); }
  }
  return <div className="modal-backdrop" role="presentation"><form ref={form} className="project-dialog edition-create-dialog" role="dialog" aria-modal="true" aria-labelledby="edition-dialog-title" onSubmit={event => { event.preventDefault(); void submit(); }} onKeyDown={event => {
    if (event.key === "Escape" && !pending.current) { event.preventDefault(); onClose(); }
    if (event.key !== "Tab") return;
    const items = [...event.currentTarget.querySelectorAll<HTMLElement>('button,input,select')].filter(item => !item.matches(":disabled"));
    if (!items.length) { event.preventDefault(); return; }
    if (event.shiftKey && document.activeElement === items[0]) { event.preventDefault(); items.at(-1)!.focus(); }
    else if (!event.shiftKey && document.activeElement === items.at(-1)) { event.preventDefault(); items[0].focus(); }
  }}><div className="project-dialog-header"><h2 id="edition-dialog-title">新建作品版本</h2><button type="button" className="dialog-close" aria-label="关闭新版本" disabled={saving} onClick={onClose}>×</button></div>
    <p>为这部作品增加漫画或动画版本，各版本分别保存和编辑。</p>
    <fieldset disabled={saving || submitted} className="edition-create-fields"><label className="dialog-field"><span>作品形式</span><select value={medium} onChange={event => setMedium(event.target.value as typeof medium)}><option value="comic">漫画</option><option value="animation">动画</option></select></label>
      <label className="dialog-field"><span>版本名称</span><input value={title} maxLength={120} placeholder={`${mediaLabel(medium)}版`} onChange={event => setTitle(event.target.value)} /></label>
      {availableSource && !submitted && <div className="edition-source-choice"><label><input type="checkbox" checked={useSource} onChange={event => setUseSource(event.target.checked)} />以当前内容作为改编来源</label><p>{availableSource.label}</p></div>}
      {submitted ? <p>已固定 {attempt.current?.sources.length ?? 0} 处改编来源，将核对原建立请求。</p> : <AdaptationSourcePicker api={api} projectId={projectId} selected={selected} disabled={saving} onChange={setSelected} />}
    </fieldset>
    <p className="edition-create-note">{useSource || selected.length ? "建立后保留这份来源版本，可在新版本的对话中说明如何改编。" : "从空白开始，可直接在新版本中描述创作构想。"}</p>
    {submitted && <p className="edition-create-note">建立请求已记录，当前选择已固定。请重试以核对同一次建立结果；关闭或重新打开后仍会核对原请求，不会另建一个版本。</p>}
    {error && <p className="dialog-error" role="alert">{error}</p>}
    <div className="project-dialog-actions"><button type="button" className="secondary-button" disabled={saving} onClick={onClose}>{submitted ? "关闭" : "取消"}</button><button type="submit" className="primary-button" disabled={saving}>{saving ? "正在建立…" : submitted ? "重试建立或打开" : "建立版本"}</button></div>
  </form></div>;
}

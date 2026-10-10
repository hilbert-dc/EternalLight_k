import { useEffect, useRef, useState } from "react";
import type { CreativeEdition, CreativeMedium, MediaSourceRef, MultimediaApi } from "../../shared/multimedia-types";
import { newMediaId } from "./editor-helpers";
const mediaLabel = (medium: string) => ({ novel: "小说", comic: "漫画", animation: "动画" })[medium] ?? medium;
import "./edition-switcher.css";

export interface MediaEditionSource { label: string; refs: MediaSourceRef[] }
export function CreateMediaEditionDialog({ api, projectId, source, onClose, onCreated }: {
  api: MultimediaApi; projectId: string; source: MediaEditionSource | null;
  onClose: () => void; onCreated: (edition: CreativeEdition) => Promise<void>;
}) {
  const [frozenSource, setFrozenSource] = useState<MediaEditionSource | null>(null);
  const availableSource = frozenSource ?? source;
  const [medium, setMedium] = useState<Extract<CreativeMedium, "comic" | "animation">>("comic");
  const [title, setTitle] = useState("");
  const [useSource, setUseSource] = useState(false);
  const [error, setError] = useState<string | null>(null), [saving, setSaving] = useState(false);
  const pending = useRef(false), mounted = useRef(true), form = useRef<HTMLFormElement>(null);
  const attempt = useRef<{ fingerprint: string; id: string } | null>(null);
  useEffect(() => { mounted.current = true; const previous = document.activeElement as HTMLElement | null; form.current?.querySelector<HTMLInputElement>("input")?.focus(); return () => { mounted.current = false; if (previous?.isConnected) previous.focus({ preventScroll: true }); }; }, []);
  async function submit() {
    if (pending.current) return;
    const name = title.trim() || `${mediaLabel(medium)}版`;
    if (name.length > 120) { setError("版本名称不能超过 120 个字符。"); return; }
    const sources = useSource ? availableSource?.refs ?? [] : [];
    if (useSource && !frozenSource && availableSource) setFrozenSource(structuredClone(availableSource));
    if (useSource && !sources.length) { setError("来源尚未保存，请回到作品保存后重新选择。"); return; }
    const fingerprint = JSON.stringify({ projectId, title: name, medium, sources });
    if (attempt.current?.fingerprint !== fingerprint) attempt.current = { fingerprint, id: newMediaId("create-edition") };
    pending.current = true; setSaving(true); setError(null);
    try {
      const edition = await api.createEdition(projectId, { title: name, medium, sources, idempotencyKey: attempt.current.id });
      if (!mounted.current) return;
      if (edition.projectId !== projectId || edition.medium !== medium || !edition.id || edition.editionId !== edition.id || !edition.scopeId || !edition.branchId || JSON.stringify(edition.sources) !== JSON.stringify(sources)) throw new Error("新版本回执与所选形式或来源不一致，请重新读取作品。");
      await onCreated(edition);
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
    <fieldset disabled={saving} className="edition-create-fields"><label className="dialog-field"><span>作品形式</span><select value={medium} onChange={event => setMedium(event.target.value as typeof medium)}><option value="comic">漫画</option><option value="animation">动画</option></select></label>
      <label className="dialog-field"><span>版本名称</span><input value={title} maxLength={120} placeholder={`${mediaLabel(medium)}版`} onChange={event => setTitle(event.target.value)} /></label>
      {availableSource && <div className="edition-source-choice"><label><input type="checkbox" checked={useSource} onChange={event => setUseSource(event.target.checked)} />以当前内容作为改编来源</label><p>{availableSource.label}</p></div>}
    </fieldset>
    <p className="edition-create-note">{useSource ? "建立后保留这份来源版本，可在新版本的对话中说明如何改编。" : "从空白开始，可直接在新版本中描述创作构想。"}</p>
    {error && <p className="dialog-error" role="alert">{error}</p>}
    <div className="project-dialog-actions"><button type="button" className="secondary-button" disabled={saving} onClick={onClose}>取消</button><button type="submit" className="primary-button" disabled={saving}>{saving ? "正在建立…" : "建立版本"}</button></div>
  </form></div>;
}

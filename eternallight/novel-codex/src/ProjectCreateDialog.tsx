import { useEffect, useRef, useState } from "react";
import "./account-layout.css";
import type { CreativeMedium } from "../shared/multimedia-types";

export function ProjectCreateDialog({ onClose, onSubmit }: { onClose: () => void; onSubmit: (title: string, logline: string, medium: CreativeMedium, idempotencyKey: string) => Promise<void> }) {
  const [medium, setMedium] = useState<CreativeMedium>("novel");
  const submissionKey = useRef(crypto.randomUUID());
  const [title, setTitle] = useState("");
  const [logline, setLogline] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const submitting = useRef(false);
  const dialog = useRef<HTMLFormElement | null>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLInputElement>("input")?.focus();
    return () => {
      const target = previous?.isConnected && previous !== document.body ? previous
        : document.querySelector<HTMLElement>(".project-switcher:not(:disabled)");
      target?.focus({ preventScroll: true });
    };
  }, []);
  async function submit() {
    if (submitting.current) return;
    const normalizedTitle = title.trim();
    if (!normalizedTitle) { setError("请填写作品名称。"); return; }
    if (normalizedTitle.length > 120) { setError("作品名称不能超过 120 个字符。"); return; }
    if (logline.trim().length > 500) { setError("作品简介不能超过 500 个字符。"); return; }
    submitting.current = true;
    setSaving(true);
    setError(null);
    try {
      await onSubmit(normalizedTitle, logline, medium, submissionKey.current);
      onClose();
    } catch (reason) {
      const message = reason instanceof Error
        ? reason.message.replace(/^Error invoking remote method '[^']+':\s*(?:Error:\s*)?/, "")
        : "作品创建失败，请稍后重试。";
      setError(message || "作品创建失败，请稍后重试。");
    } finally { submitting.current = false; setSaving(false); }
  }
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape" && !submitting.current) onClose(); };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose, saving]);
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !submitting.current) onClose(); }}>
    <form ref={dialog} className="project-dialog project-create-dialog" role="dialog" aria-modal="true" aria-labelledby="project-dialog-title" aria-describedby="project-dialog-description" tabIndex={-1}
      onSubmit={(event) => { event.preventDefault(); void submit(); }} onKeyDown={event => {
        if (event.key !== "Tab") return;
        const items = [...event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex="0"]')];
        const first = items[0], last = items.at(-1);
        if (!first) { event.preventDefault(); dialog.current?.focus(); }
        else if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog.current)) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
      }}>
      <div className="project-dialog-header"><div><span className="eyebrow">新作品</span><h2 id="project-dialog-title">新建作品</h2></div><button className="dialog-close" type="button" aria-label="关闭" onClick={() => { if (!submitting.current) onClose(); }} disabled={saving}>×</button></div>
      <p id="project-dialog-description" className="project-dialog-description">选择作品形式，直接开始创作。之后也可以为同一作品创建其他形式。</p>
      <label className="dialog-field"><span>作品形式</span><select value={medium} disabled={saving} onChange={event => { setMedium(event.target.value as CreativeMedium); submissionKey.current = crypto.randomUUID(); }}><option value="novel">小说</option><option value="comic">漫画</option><option value="animation">动画</option></select></label>
      <label className="dialog-field"><span>作品名称 <em>必填</em></span><input value={title} maxLength={120} placeholder="例如：雾中的回声" onChange={(event) => { setTitle(event.target.value); setError(null); }} /></label>
      <label className="dialog-field"><span>一句话简介 <small>可选</small></span><textarea value={logline} maxLength={500} placeholder="描述这部作品的核心冲突或气质" onChange={(event) => { setLogline(event.target.value); setError(null); }} onKeyDown={(event) => { if ((event.metaKey || event.ctrlKey) && event.key === "Enter") { event.preventDefault(); void submit(); } }} /></label>
      {error && <div className="dialog-error" role="alert">{error}</div>}
      <div className="project-dialog-actions"><button className="secondary-button" type="button" onClick={() => { if (!submitting.current) onClose(); }} disabled={saving}>取消</button><button className="primary-button" type="submit" disabled={saving}>{saving ? "创建中…" : "创建作品"}</button></div>
    </form>
  </div>;
}

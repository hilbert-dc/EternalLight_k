import { useEffect, useRef, useState } from "react";
import "./export-dialog.css";
import type { AuthorExportOptions, AuthorExportResult, AuthorExportScope } from "./types";

const choices: { scope: AuthorExportScope; label: string }[] = [
  { scope: "manuscript", label: "正式正文（已采用）" },
  { scope: "planning", label: "正式规划（公开指引）" },
  { scope: "candidates", label: "候选稿（当前未采用正文与公开规划）" },
  { scope: "private", label: "作者私密资料（完整正式规划与私密事实）" },
];

export function ExportDialog({ onClose, onExport }: { onClose: () => void; onExport: (options: AuthorExportOptions) => Promise<AuthorExportResult> }) {
  const [scopes, setScopes] = useState<AuthorExportScope[]>(["manuscript"]);
  const [format, setFormat] = useState<AuthorExportOptions["format"]>("markdown");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [receipt, setReceipt] = useState("");
  const dialog = useRef<HTMLElement>(null);
  const inFlight = useRef(false);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("input")?.focus();
    return () => previous?.focus();
  }, []);
  async function submit() {
    if (inFlight.current || !scopes.length) return;
    inFlight.current = true; setBusy(true); setError(""); setReceipt("");
    try {
      const result = await onExport({ scopes, format });
      const url = URL.createObjectURL(new Blob([result.content], { type: result.mimeType }));
      const anchor = document.createElement("a");
      anchor.href = url; anchor.download = result.fileName;
      document.body.appendChild(anchor); anchor.click(); anchor.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      setReceipt(`已生成作品版本 ${result.projectRevision} 的导出文件：${result.fileName}`);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "导出失败，请重试。");
    } finally { inFlight.current = false; setBusy(false); }
  }
  return <div className="modal-backdrop" role="presentation"><section ref={dialog} className="project-dialog" role="dialog" aria-modal="true" aria-labelledby="export-title" aria-describedby="export-description" tabIndex={-1} onKeyDown={(event) => {
    if (event.key === "Escape" && !busy) onClose();
    if (event.key === "Tab") {
      const elements = Array.from(dialog.current?.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), select:not(:disabled)") ?? []);
      const first = elements[0]; const last = elements.at(-1);
      if (!first) { event.preventDefault(); dialog.current?.focus(); }
      else if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog.current)) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    }
  }}><h2 id="export-title">导出作品</h2><p id="export-description">仅导出当前已保存内容；未保存编辑不会进入文件。正式正文只包含已采用内容。公开规划仅含公开指引；完整简报、大纲内部安排需选择私密资料。</p><fieldset disabled={busy}><legend>导出范围</legend>{choices.map(({ scope, label }) => <label className="export-scope" key={scope}><span><input type="checkbox" checked={scopes.includes(scope)} onChange={(event) => { setReceipt(""); setScopes(event.target.checked ? [...scopes, scope] : scopes.filter((value) => value !== scope)); }} /> {label}</span></label>)}</fieldset>{scopes.includes("private") && <p role="note">文件将包含作者私密内容。若同时选择候选稿，也会包含完整候选规划；请确认分享对象。</p>}<label className="dialog-field"><span>文件格式</span><select disabled={busy} value={format} onChange={(event) => { setReceipt(""); setFormat(event.target.value as AuthorExportOptions["format"]); }}><option value="markdown">Markdown (.md)</option><option value="txt">纯文本 (.txt)</option></select></label>{error && <p role="alert" className="dialog-error">{error}</p>}{receipt && <p role="status">{receipt}</p>}<div className="project-dialog-actions"><button type="button" className="secondary-button" disabled={busy} onClick={onClose}>关闭</button><button type="button" className="primary-button" disabled={busy || !scopes.length} onClick={() => void submit()}>{busy ? "正在导出…" : "下载文件"}</button></div></section></div>;
}

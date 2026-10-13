import { useEffect, useRef, type ReactNode } from "react";
import "./chapter-publication.css";

export function ChapterPublicationDialog({ busy, inline = false, onClose, children }: { busy: boolean; inline?: boolean; onClose: () => void; children: ReactNode }) {
  const dialog = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (inline) return;
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.focus();
    return () => { if (previous?.isConnected) previous.focus({ preventScroll: true }); };
  }, [inline]);
  if (inline) return <>{children}</>;
  return <div className="modal-backdrop"><section className="project-dialog chapter-publication-dialog" ref={dialog} tabIndex={-1}
    role="dialog" aria-modal="true" aria-labelledby="chapter-publication-title" onKeyDown={event => {
      if (event.key === "Escape" && !busy) { event.stopPropagation(); onClose(); }
      if (event.key !== "Tab") return;
      const items = [...event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex="0"]')];
      const first = items[0], last = items.at(-1);
      if (!first) { event.preventDefault(); return; }
      if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog.current)) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    }}>
    <div className="project-dialog-header"><h2 id="chapter-publication-title">发布前检查</h2><button type="button" className="dialog-close" aria-label="关闭发布前检查" disabled={busy} onClick={onClose}>×</button></div>
    <p>检查这一章的故事设定变化和保护冲突；确认发布后，整章正文与决定的故事设定一起更新。</p>
    {children}
  </section></div>;
}

import { useEffect, useRef, type ReactNode } from "react";
import "./commitment-panel.css";

export function CommitmentManagementDialog({ protectedInput, dirty, onClose, children }: {
  protectedInput: boolean; dirty: boolean; onClose: () => void; children: ReactNode;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    return () => {
      dialog.current?.close();
      if (previous?.isConnected) previous.focus({ preventScroll: true });
    };
  }, []);
  return <dialog ref={dialog} className="commitment-management-dialog" aria-labelledby="commitment-management-title"
    onCancel={event => { event.preventDefault(); if (!protectedInput) onClose(); }}>
    <div className="project-dialog-header"><h2 id="commitment-management-title">调整伏笔建议</h2>
      <button type="button" className="dialog-close" aria-label="关闭伏笔调整" disabled={protectedInput} onClick={onClose}>×</button></div>
    {protectedInput && <p role="status">{dirty ? "请先保存或放弃本地修改，再关闭。" : "当前操作完成后可关闭。"}</p>}
    {children}
  </dialog>;
}

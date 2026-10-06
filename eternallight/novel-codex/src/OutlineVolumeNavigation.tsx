import { useEffect, useId, useState } from "react";
import type { WorkCatalog } from "./types";
import "./outline-volume.css";

type NavigationProps = {
  catalog: WorkCatalog | null;
  error: string | null;
  busy: boolean;
  active: boolean;
  selectedVolumeId: string | null;
  onNavigate: () => void;
  onSelect: (volumeId: string) => void;
  onRetry: () => void;
};

export function OutlineVolumeNavigation({ catalog, error, busy, active, selectedVolumeId, onNavigate, onSelect, onRetry }: NavigationProps) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  useEffect(() => { if (active && selectedVolumeId) setOpen(true); }, [active, selectedVolumeId]);
  const volumes = [...(catalog?.volumes ?? [])].sort((left, right) => left.position - right.position || left.id.localeCompare(right.id));
  const selectionDisabled = busy || Boolean(error) || !catalog;
  return <div className="outline-volume-navigation">
    <div className="outline-volume-root">
      <button type="button" className={`nav-item ${active ? "active" : ""}`} aria-current={active && !selectedVolumeId ? "page" : undefined} disabled={busy} onClick={onNavigate}>
        <span className="nav-icon" aria-hidden="true">☷</span><span className="outline-volume-label">全书大纲</span>
      </button>
      <button type="button" className="outline-volume-toggle" aria-label={open ? "收起大纲下的卷" : "展开大纲下的卷"} aria-expanded={open} aria-controls={listId} onClick={() => setOpen(value => !value)}><span aria-hidden="true">{open ? "⌄" : "›"}</span></button>
    </div>
    <div id={listId} className="outline-volume-children" hidden={!open}>
      {error && <div className="outline-volume-error" role="alert"><p>{error}</p><button type="button" disabled={busy} onClick={onRetry}>重试读取作品卷</button></div>}
      {!catalog && !error && <p role="status">正在读取作品卷…</p>}
      {catalog && volumes.length === 0 && <p>暂无作品卷</p>}
      {volumes.length > 0 && <ul>{volumes.map(volume => <li key={volume.id}>
        <button type="button" data-outline-volume-id={volume.id} className={active && selectedVolumeId === volume.id ? "active" : ""} aria-current={active && selectedVolumeId === volume.id ? "page" : undefined} title={volume.title} disabled={selectionDisabled} onClick={() => { if (!selectionDisabled) onSelect(volume.id); }}>{volume.title}</button>
      </li>)}</ul>}
    </div>
  </div>;
}

import "./module-records.css";

export function ModuleRecordsButton({ label, open = false, onClick }: { label: string; open?: boolean; onClick: () => void }) {
  return <button type="button" className="module-records-button" aria-label="查看修改记录" title={`${label} · 修改记录`}
    aria-expanded={open} aria-controls={open ? "module-modification-records" : undefined} onClick={onClick}>
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
      <path d="M3 10a9 9 0 1 1 2.7 8.5M3 4v6h6M12 7v5l3 2" strokeLinecap="round" strokeLinejoin="round" />
    </svg><span>修改记录</span>
  </button>;
}

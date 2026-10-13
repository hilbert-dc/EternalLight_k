import { useCallback, useState } from "react";

import { IndexAuditPanel } from "./IndexAuditPanel";
import { summarizeDiagnostics, type AuditKind, type DiagnosticState } from "./diagnostic-state";
import type { NovelCodexApi } from "./types";

const kinds: AuditKind[] = ["search", "entity", "relation", "knowledge", "snapshotKnowledge", "fact"];

export function WorkspaceDiagnostics({ api, projectId, projectTitle, refreshToken }: {
  api: NovelCodexApi; projectId: string; projectTitle?: string; refreshToken: number;
}) {
  const [activated, setActivated] = useState(false);
  const [states, setStates] = useState<Partial<Record<AuditKind, DiagnosticState>>>({});
  const onState = useCallback((kind: AuditKind, state: DiagnosticState) => {
    setStates((previous) => previous[kind]?.status === state.status
      && previous[kind]?.verifiedConsistent === state.verifiedConsistent ? previous : { ...previous, [kind]: state });
  }, []);
  const summary = summarizeDiagnostics(kinds.map((kind) => states[kind]));

  return <details className="workspace-diagnostics" aria-label="本地数据诊断"
    onToggle={(event) => { if (event.currentTarget.open) setActivated(true); }}>
    <summary><strong>检查本地数据</strong><span className={summary.attention ? "diagnostic-attention" : ""}
      role={summary.attention ? "alert" : undefined}>{activated ? summary.caption : "排查检索或资料显示异常"}</span></summary>
    {projectTitle && <p className="diagnostic-project">当前作品：{projectTitle}</p>}
    <p>日常写作无需逐项运行。这里核对本地数据与索引，不检查情节或文学质量；诊断不会改写正文。</p>
    <div className="workspace-diagnostic-panels">
      {activated && kinds.map((kind) => <IndexAuditPanel key={`${projectId}-${kind}`} api={api} projectId={projectId}
        refreshToken={refreshToken} kind={kind} onState={onState} />)}
    </div>
  </details>;
}

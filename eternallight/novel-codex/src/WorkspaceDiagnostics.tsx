import { useCallback, useState } from "react";

import { IndexAuditPanel } from "./IndexAuditPanel";
import { summarizeDiagnostics, type AuditKind, type DiagnosticState } from "./diagnostic-state";
import type { NovelCodexApi } from "./types";

const kinds: AuditKind[] = ["search", "entity", "relation", "knowledge", "snapshotKnowledge", "fact"];

export function WorkspaceDiagnostics({ api, projectId, refreshToken }: {
  api: NovelCodexApi; projectId: string; refreshToken: number;
}) {
  const [states, setStates] = useState<Partial<Record<AuditKind, DiagnosticState>>>({});
  const onState = useCallback((kind: AuditKind, state: DiagnosticState) => {
    setStates((previous) => previous[kind]?.status === state.status
      && previous[kind]?.verifiedConsistent === state.verifiedConsistent ? previous : { ...previous, [kind]: state });
  }, []);
  const summary = summarizeDiagnostics(kinds.map((kind) => states[kind]));

  return <details className="workspace-diagnostics" aria-label="系统诊断">
    <summary><strong>系统诊断</strong><span className={summary.attention ? "diagnostic-attention" : ""}
      role={summary.attention ? "alert" : undefined}>{summary.caption}</span></summary>
    <p>这些检查分别核对检索、人物、关系、知识、历史快照和确认事实，不代表全局故事知识或文学质量已通过。</p>
    <div className="workspace-diagnostic-panels">
      {kinds.map((kind) => <IndexAuditPanel key={`${projectId}-${kind}`} api={api} projectId={projectId}
        refreshToken={refreshToken} kind={kind} onState={onState} />)}
    </div>
  </details>;
}

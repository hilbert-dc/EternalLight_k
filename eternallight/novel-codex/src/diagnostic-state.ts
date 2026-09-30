import type { EntityAuditJobSummary, FactAuditJobSummary, KnowledgeAuditJobSummary, RelationAuditJobSummary, SearchAuditJobSummary, SnapshotKnowledgeAuditJobSummary } from "./types";

export type AuditJob = SearchAuditJobSummary | EntityAuditJobSummary | RelationAuditJobSummary | KnowledgeAuditJobSummary | SnapshotKnowledgeAuditJobSummary | FactAuditJobSummary;
export type AuditKind = "search" | "entity" | "relation" | "knowledge" | "snapshotKnowledge" | "fact";
export interface DiagnosticState {
  status: AuditJob["status"] | "loading" | "unavailable" | "notStarted";
  verifiedConsistent: boolean | null;
}

export function summarizeDiagnostics(states: (DiagnosticState | undefined)[]) {
  const loading = states.filter((item) => !item || item.status === "loading").length;
  const active = states.filter((item) => item?.status === "queued" || item?.status === "running").length;
  const paused = states.filter((item) => item?.status === "paused").length;
  const attention = states.filter((item) => item && (
    item.status === "failed" || item.status === "stale" || item.status === "unavailable"
    || (item.status === "completed" && item.verifiedConsistent !== true)
  )).length;
  const labels = [attention ? `${attention} 项需处理` : "", active ? `${active} 项进行中` : "",
    paused ? `${paused} 项已暂停` : "", loading ? "读取中" : ""].filter(Boolean);
  return { attention, caption: labels.length ? labels.join(" · ") : "六类按需校验" };
}

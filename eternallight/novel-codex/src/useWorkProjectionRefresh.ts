import { useEffect, useRef } from "react";
import type { WorkTaskSummary } from "./types";

// Persisted identities invalidate read projections; they never assert delivery
// or cause execution. Exclude usage/status churn from full workspace reads.
export function workProjectionKey(tasks: WorkTaskSummary[], projectId: string): string {
  const identities = new Set<string>();
  for (const task of tasks) {
    if (task.projectId !== projectId) continue;
    for (const step of task.steps) {
      if (step.target.chapterId) identities.add(JSON.stringify(["chapter", step.target.chapterId]));
      for (const ref of step.outputRefs) identities.add(JSON.stringify(["output", ref.kind, ref.objectId, ref.revisionId]));
      for (const change of step.changes) identities.add(JSON.stringify(["change", change.commitId, change.resultRevision, change.undoneByCommitId]));
    }
    for (const evidence of task.deliveryReport.evidence) {
      if (evidence.referenceId && evidence.kind !== "completionReport") {
        identities.add(JSON.stringify(["evidence", evidence.kind, evidence.referenceId, evidence.state]));
      }
    }
  }
  return JSON.stringify([...identities].sort());
}

export function useWorkProjectionRefresh(scope: object, projectId: string, tasks: WorkTaskSummary[], loaded: boolean, invalidate: () => void) {
  const previous = useRef<{ scope: object; key: string } | null>(null);
  const callback = useRef(invalidate); callback.current = invalidate;
  const key = workProjectionKey(tasks, projectId);
  useEffect(() => {
    if (!loaded) return;
    const prior = previous.current;
    previous.current = { scope, key };
    // Initial project reads already load every projection. Only subsequent
    // persisted changes invalidate them; do not restart the task reader.
    if (prior?.scope === scope && prior.key !== key) callback.current();
  }, [scope, key, loaded]);
}

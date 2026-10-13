import { useEffect, useRef, useState } from "react";
import { revisionPredecessorIsCurrent } from "./revision-outcome-decision";
import type { NovelCodexApi, RevisionRepairPreparation } from "./types";

// A preparation labels the editable composer; only submitPrompt can start a discussion.
export function useRevisionRepair({ api, projectId, activeProjectId, blocked, composer }: {
  api: NovelCodexApi | undefined; projectId: string; activeProjectId: string; blocked: boolean; composer: string;
}) {
  const [prepared, setPrepared] = useState<RevisionRepairPreparation | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sourceRef = useRef(prepared);
  const readRef = useRef<object | null>(null);
  const mounted = useRef(true);
  const guardKey = JSON.stringify([projectId, activeProjectId, blocked, composer]);
  const guard = useRef({ api, key: guardKey, token: {}, blocked, projectId, activeProjectId });
  if (guard.current.key !== guardKey || guard.current.api !== api) guard.current = { api, key: guardKey, token: {}, blocked, projectId, activeProjectId };
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; readRef.current = null; }; }, []);

  function cancel() {
    sourceRef.current = null; readRef.current = null;
    setPrepared(null); setChecking(false); setError(null);
  }
  useEffect(() => { cancel(); }, [projectId, activeProjectId]);

  function prepare(value: RevisionRepairPreparation) {
    if (guard.current.blocked || value.projectId !== guard.current.projectId
      || value.projectId !== guard.current.activeProjectId || !mounted.current) return false;
    readRef.current = null; sourceRef.current = value;
    setPrepared(value); setChecking(false); setError(null);
    return true;
  }

  async function preflight() {
    const value = sourceRef.current, scope = guard.current, read = {};
    if (!value || scope.blocked || readRef.current || value.projectId !== scope.projectId
      || value.projectId !== scope.activeProjectId || !api) return null;
    readRef.current = read; setChecking(true); setError(null);
    const current = () => mounted.current && readRef.current === read && sourceRef.current === value
      && guard.current.token === scope.token;
    try {
      const receipt = await api.runtime.getRevisionOutcome(value.projectId, value.source.writerTaskId);
      if (!current()) return null;
      // Runtime's current state checks the actual draft head and published knowledge batch.
      if (!revisionPredecessorIsCurrent(receipt, value.source, "repairDraft")
        || receipt?.assessment.chapterId !== value.chapterId
        || receipt.assessment.draftRevisionId !== value.draftRevisionId
        || receipt.assessment.knowledgeBatchId !== value.knowledgeBatchId) {
        throw new Error("改稿来源或最新作者决策已变化，请取消此来源并重新读取修改结果。");
      }
      return value.source;
    } catch (reason) {
      if (current()) setError(reason instanceof Error ? reason.message : "改稿讨论来源核对失败");
      return null;
    } finally {
      if (mounted.current && readRef.current === read) { readRef.current = null; setChecking(false); }
    }
  }

  return { prepared, checking, error, prepare, cancel, preflight };
}

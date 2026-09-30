import { useEffect, useMemo, useRef, useState } from "react";
import { DesiredOutcomes } from "./DesiredOutcomes";
import { RevisionOutcomeDecisionPanel } from "./RevisionOutcomeDecisionPanel";
import type { DesiredOutcome, NovelCodexApi, RevisionOutcome, RevisionOutcomeReport, RevisionPredecessor, RevisionRepairPreparation } from "./types";

const statusLabels: Record<RevisionOutcomeReport["outcomes"][number]["status"], string> = {
  supported: "有证据支持", unmet: "未满足", insufficientEvidence: "证据不足",
};

function matchesCriteria(receipt: RevisionOutcome, writerTaskId: string, chapterId: string, criteria: DesiredOutcome[]) {
  const report = receipt.assessment;
  return receipt.authority === "candidateAnalysis" && ["current", "stale"].includes(receipt.state)
    && report?.writerTaskId === writerTaskId && report.chapterId === chapterId
    && Array.isArray(report.outcomes) && report.outcomes.length === criteria.length
    && criteria.every(criterion => report.outcomes.filter(outcome => outcome.id === criterion.id).length === 1)
    && report.outcomes.every(outcome => {
      const criterion = criteria.find(item => item.id === outcome.id);
      return criterion && Object.hasOwn(statusLabels, outcome.status)
        && typeof outcome.reason === "string" && outcome.reason.trim().length > 0 && [...outcome.reason].length <= 1000
        && Array.isArray(outcome.evidence) && outcome.evidence.length <= 4
        && (outcome.status === "insufficientEvidence" || outcome.evidence.length > 0)
        && outcome.evidence.every(item => criterion.groupIds.includes(item.groupId)
          && typeof item.quote === "string" && [...item.quote].length > 0 && [...item.quote].length <= 1000
          && Number.isSafeInteger(item.start) && Number.isSafeInteger(item.end) && item.start >= 0 && item.end > item.start);
    });
}

export function RevisionOutcomeDetails({ api, projectId, writerTaskId, chapterId, desiredOutcomes, version, disabled = false, onPrepareContinuation, onPrepareRepair }: {
  api: NovelCodexApi | undefined; projectId: string; writerTaskId: string; chapterId: string;
  desiredOutcomes: DesiredOutcome[]; version: string;
  disabled?: boolean; onPrepareContinuation?: (predecessor: RevisionPredecessor) => void;
  onPrepareRepair?: (preparation: RevisionRepairPreparation) => void;
}) {
  // Polling produces fresh target objects; only changed content invalidates a read.
  const criteriaKey = JSON.stringify(desiredOutcomes);
  const scope = useMemo(() => ({ api, projectId, writerTaskId, chapterId, criteriaKey, version }),
    [api, projectId, writerTaskId, chapterId, criteriaKey, version]);
  const [open, setOpen] = useState(false);
  const detailsRef = useRef<HTMLDetailsElement>(null);
  const [retry, setRetry] = useState(0);
  const [state, setState] = useState<{ scope: typeof scope; receipt: RevisionOutcome | null; error: string | null } | null>(null);
  useEffect(() => {
    if (!open || !detailsRef.current?.open) return;
    let active = true;
    setState(null);
    void (async () => {
      try {
        if (!scope.api?.runtime?.getRevisionOutcome) throw new Error("修改结果读取接口不可用");
        const receipt = await scope.api.runtime.getRevisionOutcome(scope.projectId, scope.writerTaskId);
        const criteria: DesiredOutcome[] = JSON.parse(scope.criteriaKey);
        if (receipt && !matchesCriteria(receipt, scope.writerTaskId, scope.chapterId, criteria)) throw new Error("判断与当前写作任务、章节或已批准标准不匹配");
        if (active) setState({ scope, receipt, error: null });
      } catch (error) {
        if (active) setState({ scope, receipt: null, error: error instanceof Error ? error.message : "读取修改结果失败" });
      }
    })();
    return () => { active = false; };
  }, [open, retry, scope]);
  const visible = state?.scope === scope ? state : null;
  return <details ref={detailsRef} className="revision-outcome-details" onToggle={event => setOpen(event.currentTarget.open)}>
    <summary>查看修改结果 · 候选判断</summary>
    {open && <>
      <p>模型对本次候选正文的判断，不是创作目标达成认证，也不会采用正文或事实。</p>
      <small>写作尝试 {writerTaskId}</small>
      <DesiredOutcomes outcomes={desiredOutcomes} approved />
      {!visible ? <p role="status">读取修改结果中…</p> : visible.error ? <div role="alert">
        <p>{visible.error}</p><button type="button" onClick={() => setRetry(value => value + 1)}>重试读取修改结果</button>
      </div> : !visible.receipt ? <p role="status">尚无修改结果判断；写作或知识分析结束不代表标准已满足。</p> : <>
        <p role="status">{visible.receipt.state === "current" ? "当前来源 · 候选判断" : "已过期 · 来源已变化，以下判断不能代表当前正文"}</p>
        <dl><dt>原正文版本</dt><dd>{visible.receipt.assessment.originalRevisionId}</dd>
          <dt>候选正文版本</dt><dd>{visible.receipt.assessment.draftRevisionId}</dd>
          <dt>知识批次</dt><dd>{visible.receipt.assessment.knowledgeBatchId}</dd>
          <dt>判断记录</dt><dd>{visible.receipt.revisionId}</dd></dl>
        <ul aria-label="逐项修改结果">{desiredOutcomes.map(criterion => {
          const outcome = visible.receipt!.assessment.outcomes.find(item => item.id === criterion.id)!;
          return <li key={criterion.id}>
            <strong>{criterion.id} · {statusLabels[outcome.status]}</strong>
            <p>{outcome.reason}</p>
            {outcome.evidence.length === 0 ? <small>没有可定位的正文证据。</small> : outcome.evidence.map((item, index) =>
              <figure key={index}><blockquote style={{ whiteSpace: "pre-wrap" }}>{item.quote}</blockquote>
                <figcaption>场景 {item.groupId} · Unicode 字符区间 [{item.start}, {item.end})（候选场景正文）</figcaption></figure>)}
          </li>;
        })}</ul>
        <RevisionOutcomeDecisionPanel key={`${projectId}:${writerTaskId}:${version}:${retry}:${visible.receipt.revisionId}`} api={api} projectId={projectId}
          receipt={visible.receipt} disabled={disabled} onPrepareContinuation={onPrepareContinuation} onPrepareRepair={onPrepareRepair} />
      </>}
      {visible && !visible.error && <button type="button" onClick={() => setRetry(value => value + 1)}>重新读取修改结果</button>}
    </>}
  </details>;
}

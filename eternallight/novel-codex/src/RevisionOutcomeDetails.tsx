import { useEffect, useMemo, useRef, useState } from "react";
import { DesiredOutcomes } from "./DesiredOutcomes";
import { RevisionOutcomeDecisionPanel } from "./RevisionOutcomeDecisionPanel";
import type { ChapterAcceptanceReport, DesiredOutcome, NovelCodexApi, RevisionOutcome, RevisionOutcomeReport, RevisionPredecessor, RevisionRepairPreparation } from "./types";
import { CreationIssueList, CreationIssueReference, useCreationIssueContext, isCreationIssueLink, issueLinksMatch } from "./CreationIssues";

const statusLabels: Record<RevisionOutcomeReport["outcomes"][number]["status"], string> = {
  supported: "有证据支持", unmet: "未满足", insufficientEvidence: "证据不足",
};

type ReportCriterion = { id: string; criterion: string; groupIds?: string[] };

function reportCriteria(acceptance: ChapterAcceptanceReport["chapterAcceptance"], desiredOutcomes: DesiredOutcome[]): ReportCriterion[] {
  return [...acceptance.requirements.map(requirement => ({ id: requirement.id, criterion: requirement.text })), ...desiredOutcomes];
}

function matchesCriteria(receipt: RevisionOutcome, writerTaskId: string, chapterId: string, criteria: ReportCriterion[]) {
  const report = receipt.assessment;
  return receipt.authority === "candidateAnalysis" && ["current", "stale"].includes(receipt.state)
    && report?.writerTaskId === writerTaskId && report.chapterId === chapterId
    && Array.isArray(report.outcomes) && report.outcomes.length === criteria.length
    && Array.isArray(report.uncoveredRequirements)
    && report.uncoveredRequirements.every(requirement => isCreationIssueLink(requirement.issue)
      && typeof requirement.requirement === "string" && requirement.requirement.trim()
      && typeof requirement.reason === "string" && requirement.reason.trim()
      && Array.isArray(requirement.authorSourceChecks) && requirement.authorSourceChecks.length > 0 && requirement.authorSourceChecks.length <= 4
      && requirement.authorSourceChecks.every(source => typeof source.sourceId === "string" && source.sourceId.trim()
        && typeof source.contentHash === "string" && /^[a-f0-9]{64}$/.test(source.contentHash)
        && typeof source.quote === "string" && source.quote.trim()))
    && issueLinksMatch(receipt.issues, [...report.outcomes.map(outcome => outcome.issue), ...report.uncoveredRequirements.map(requirement => requirement.issue)])
    && criteria.every(criterion => report.outcomes.filter(outcome => outcome.id === criterion.id).length === 1)
    && report.outcomes.every(outcome => {
      const criterion = criteria.find(item => item.id === outcome.id);
      return criterion && Object.hasOwn(statusLabels, outcome.status)
        && typeof outcome.reason === "string" && outcome.reason.trim().length > 0 && [...outcome.reason].length <= 1000
        && Array.isArray(outcome.evidence) && outcome.evidence.length <= 4
        && (outcome.status === "insufficientEvidence" || outcome.evidence.length > 0)
        && outcome.evidence.every(item => typeof item.groupId === "string" && item.groupId.trim().length > 0
          && (!criterion.groupIds || criterion.groupIds.includes(item.groupId))
          && typeof item.quote === "string" && [...item.quote].length > 0 && [...item.quote].length <= 1000
          && Number.isSafeInteger(item.start) && Number.isSafeInteger(item.end) && item.start >= 0 && item.end > item.start);
    });
}

export function RevisionOutcomeDetails({ api, projectId, writerTaskId, chapterId, chapterAcceptance, desiredOutcomes, version, disabled = false, onPrepareContinuation, onPrepareRepair }: {
  api: NovelCodexApi | undefined; projectId: string; writerTaskId: string; chapterId: string;
  chapterAcceptance: ChapterAcceptanceReport["chapterAcceptance"]; desiredOutcomes: DesiredOutcome[]; version: string;
  disabled?: boolean; onPrepareContinuation?: (predecessor: RevisionPredecessor) => void;
  onPrepareRepair?: (preparation: RevisionRepairPreparation) => void;
}) {
  const managedIssues = useCreationIssueContext();
  // Polling produces fresh target objects; only changed content invalidates a read.
  const criteria = reportCriteria(chapterAcceptance, desiredOutcomes);
  const criteriaKey = JSON.stringify(criteria);
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
        const criteria: ReportCriterion[] = JSON.parse(scope.criteriaKey);
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
    <summary>查看检查结果</summary>
    {open && <>
      <p>模型对本次候选正文的判断，不是创作目标达成认证，也不会采用正文或事实。</p>
      <small>写作尝试 {writerTaskId}</small>
      <section aria-label="已冻结的章节标准"><strong>章节标准</strong>
        <ul>{chapterAcceptance.requirements.map(requirement => <li key={requirement.id}>{requirement.text}</li>)}</ul>
        <p>篇幅要求：{chapterAcceptance.length.requirement}</p>
      </section>
      <DesiredOutcomes outcomes={desiredOutcomes} approved />
      {!visible ? <p role="status">读取修改结果中…</p> : visible.error ? <div role="alert">
        <p>{visible.error}</p><button type="button" onClick={() => setRetry(value => value + 1)}>重试读取修改结果</button>
      </div> : !visible.receipt ? <p role="status">尚无修改结果判断；写作或知识分析结束不代表标准已满足。</p> : <>
        <p role="status">{visible.receipt.state === "current" ? "当前来源 · 候选判断" : "已过期 · 来源已变化，以下判断不能代表当前正文"}</p>
        <dl><dt>原正文版本</dt><dd>{visible.receipt.assessment.originalRevisionId ?? "首次起草"}</dd>
          <dt>候选正文版本</dt><dd>{visible.receipt.assessment.draftRevisionId}</dd>
          <dt>知识批次</dt><dd>{visible.receipt.assessment.knowledgeBatchId ?? "直接对照正文"}</dd>
          <dt>判断记录</dt><dd>{visible.receipt.revisionId}</dd></dl>
        {visible.receipt.assessment.uncoveredRequirements.length > 0 && <section aria-label="尚未覆盖的作者要求">
          <strong>原要求仍需核对</strong>
          <ul>{visible.receipt.assessment.uncoveredRequirements.map(requirement => <li key={requirement.issue.issueId}>
            <strong>{requirement.requirement}</strong><p>{requirement.reason}</p>
            {requirement.authorSourceChecks.map((source, index) => <blockquote key={index}>{source.quote}</blockquote>)}
            <CreationIssueReference issueId={requirement.issue.issueId} />
          </li>)}</ul>
          <p>这些要求未包含在此前的检查中，需要补齐并重新核对后再交付。</p>
        </section>}
        <ul aria-label="逐项修改结果">{criteria.map(criterion => {
          const outcome = visible.receipt!.assessment.outcomes.find(item => item.id === criterion.id)!;
          return <li key={criterion.id}>
            <strong>{criterion.criterion} · {statusLabels[outcome.status]}</strong>
            <p>{outcome.reason}</p>
            {outcome.issue && <CreationIssueReference issueId={outcome.issue.issueId} />}
            {outcome.evidence.length === 0 ? <small>没有可定位的正文证据。</small> : outcome.evidence.map((item, index) =>
              <figure key={index}><blockquote style={{ whiteSpace: "pre-wrap" }}>{item.quote}</blockquote>
                <figcaption>场景 {item.groupId} · Unicode 字符区间 [{item.start}, {item.end})（候选场景正文）</figcaption></figure>)}
          </li>;
        })}</ul>
        {!managedIssues && <CreationIssueList issues={visible.receipt.issues} />}
        <RevisionOutcomeDecisionPanel key={`${projectId}:${writerTaskId}:${version}:${retry}:${visible.receipt.revisionId}`} api={api} projectId={projectId}
          receipt={visible.receipt} disabled={disabled} onPrepareContinuation={onPrepareContinuation} onPrepareRepair={onPrepareRepair} />
      </>}
      {visible && !visible.error && <button type="button" onClick={() => setRetry(value => value + 1)}>重新读取修改结果</button>}
    </>}
  </details>;
}

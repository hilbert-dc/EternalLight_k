import { createContext, useContext, useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import type { CreationIssue, CreationIssueLink, CreationIssueObservation, CreationIssueSummary, NovelCodexApi } from "./types";
import "./creation-issues.css";

const statuses = { open: "待处理", stale: "待重新核对", disputed: "判断有分歧，待核对", resolved: "已复核解决" };
const observationStatuses = { unmet: "未满足", insufficientEvidence: "依据不足", supported: "有证据支持" };
const id = (value: unknown): value is string => typeof value === "string" && value.length > 0 && value.trim() === value
  && !/\p{Cc}/u.test(value) && new TextEncoder().encode(value).length <= 160;
const stableId = (value: unknown): value is string => typeof value === "string" && /^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/.test(value);
const text = (value: unknown): value is string => typeof value === "string" && value.trim().length > 0;
const nullableId = (value: unknown) => value === null || id(value);
const nullableCriterionId = (value: unknown) => value === null || stableId(value)
  || typeof value === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && Number(value) >= 0;
const record = (value: unknown): Record<string, unknown> | null => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null;
const unique = <T,>(values: T[], identity: (value: T) => string) => new Set(values.map(identity)).size === values.length;

export function isCreationIssueLink(value: unknown): value is CreationIssueLink {
  const link = record(value), created = record(link?.newIssue);
  return !!link && stableId(link.issueId) && (link.newIssue === null || !!created && text(created.summary))
    && Array.isArray(link.resolutionOf) && link.resolutionOf.every(stableId) && unique(link.resolutionOf, value => value);
}
function isObservation(value: unknown): value is CreationIssueObservation {
  const entry = record(value);
  return !!entry && stableId(entry.issueId) && stableId(entry.observationId) && id(entry.taskId) && nullableId(entry.reportRevisionId)
    && nullableId(entry.draftRevisionId) && nullableCriterionId(entry.criterionId)
    && typeof entry.status === "string" && Object.hasOwn(observationStatuses, entry.status) && text(entry.reason)
    && ["revisionOutcome", "deliveryReview"].includes(String(entry.source))
    && typeof entry.current === "boolean" && typeof entry.recheckRequired === "boolean" && typeof entry.superseded === "boolean"
    && Array.isArray(entry.resolutionOf) && entry.resolutionOf.every(stableId) && unique(entry.resolutionOf, value => value)
    && Array.isArray(entry.evidence);
}
export function isCreationIssue(value: unknown): value is CreationIssue {
  const issue = record(value);
  return !!issue && stableId(issue.issueId) && text(issue.summary) && typeof issue.status === "string" && Object.hasOwn(statuses, issue.status)
    && nullableId(issue.chapterId) && nullableId(issue.currentDraftRevisionId)
    && integer(issue.observationCount) && typeof issue.hasMoreObservations === "boolean"
    && Array.isArray(issue.observations) && issue.observations.every(isObservation)
    && issue.observations.every(entry => entry.issueId === issue.issueId)
    && issue.observationCount >= issue.observations.length && issue.hasMoreObservations === (issue.observationCount > issue.observations.length)
    && unique(issue.observations, entry => entry.observationId);
}
export function issueLinksMatch(issues: unknown, links: (CreationIssueLink | null)[]): issues is CreationIssue[] {
  return Array.isArray(issues) && issues.every(isCreationIssue) && unique(issues, entry => entry.issueId)
    && links.every(link => link === null || isCreationIssueLink(link) && issues.filter(issue => issue.issueId === link.issueId).length === 1);
}
function isSummary(value: unknown): value is CreationIssueSummary {
  const summary = record(value);
  return !!summary && [summary.totalCount, summary.openCount, summary.staleCount, summary.disputedCount].every(integer)
    && typeof summary.hasMore === "boolean" && Array.isArray(summary.entries) && summary.entries.length <= 2
    && summary.entries.every(entry => isCreationIssue(entry) && entry.observations.length === 0) && unique(summary.entries, entry => entry.issueId)
    && Number(summary.totalCount) >= summary.entries.length
    && summary.hasMore === (Number(summary.totalCount) > summary.entries.length)
    && Number(summary.openCount) + Number(summary.staleCount) + Number(summary.disputedCount) <= Number(summary.totalCount);
}

const IssueContext = createContext<{ open: (issueId: string) => void } | null>(null);
export function CreationIssueReference({ issueId }: { issueId: string }) {
  const context = useContext(IssueContext);
  return <button type="button" className="text-button creation-issue-reference" onClick={event => {
    if (context) context.open(issueId);
    else if (stableId(issueId)) {
      const detail = event.currentTarget.closest(".revision-outcome-details")?.querySelector<HTMLDetailsElement>(`[data-creation-issue="${issueId}"]`);
      if (detail) { detail.open = true; detail.scrollIntoView({ block: "nearest" }); }
    }
  }}>查看相关问题</button>;
}
export function useCreationIssueContext() { return useContext(IssueContext) !== null; }

function ObservationEvidence({ evidence, technical = false }: { evidence: unknown[]; technical?: boolean }) {
  return <>{evidence.map((item, index) => {
    const source = record(item), quote = source && text(source.quote) ? source.quote : null;
    return technical ? <div key={index}>{source && id(source.groupId) && integer(source.start) && integer(source.end)
      ? <dl><dt>所引场景</dt><dd>{source.groupId}</dd><dt>所引场景内字符区间</dt><dd>[{source.start}, {source.end})</dd></dl>
      : <details><summary>查看引用来源记录</summary><pre>{JSON.stringify(item, null, 2)}</pre></details>}</div>
      : quote ? <div key={index}><blockquote>{quote}</blockquote><small>{source && id(source.groupId) ? "正文引用" : "原文引用"}</small></div> : null;
  })}</>;
}

function IssueRow({ issue, scope, api, projectId, workTaskId, focus }: {
  issue: CreationIssue; scope: object; api?: NovelCodexApi; projectId?: string; workTaskId?: string; focus?: { issueId: string } | null;
}) {
  const detail = useRef<HTMLDetailsElement>(null);
  const live = useRef<object | null>(scope); live.current = scope;
  useEffect(() => () => { if (live.current === scope) live.current = null; }, [scope]);
  const flight = useRef<{ scope: object; ticket: object } | null>(null);
  const automaticAttempt = useRef<object | null>(null);
  const [page, setPage] = useState<{ scope: object; entries: CreationIssueObservation[]; next: number | null } | null>(null);
  const [pendingScope, setPending] = useState<object | null>(null), [failure, setError] = useState<{ scope: object; message: string } | null>(null);
  const pending = pendingScope === scope, error = failure?.scope === scope ? failure.message : null;
  useEffect(() => { if (focus?.issueId === issue.issueId && detail.current) { detail.current.open = true; detail.current.scrollIntoView({ block: "nearest" }); } }, [focus, issue.issueId]);
  const visible = page?.scope === scope ? page : null;
  const observations = visible?.entries ?? issue.observations;
  const more = visible ? visible.next !== null : issue.hasMoreObservations;
  const load = async (automatic = false) => {
    if (live.current !== scope || flight.current?.scope === scope || automatic && automaticAttempt.current === scope) return;
    automaticAttempt.current = scope;
    if (!api?.runtime?.getWorkIssues || !projectId || !workTaskId) { setError({ scope, message: "问题观点读取接口不可用，请重新读取任务。" }); return; }
    const ticket = { scope, ticket: {} }; flight.current = ticket; setPending(scope); setError(null);
    try {
      const reply = await api.runtime.getWorkIssues(projectId, workTaskId, { issueId: issue.issueId, cursor: visible?.next ?? 0, limit: 16 });
      if (live.current !== scope || flight.current !== ticket) return;
      if (!("issueId" in reply) || reply.issueId !== issue.issueId || !Array.isArray(reply.entries) || reply.entries.length > 16
        || !reply.entries.every(entry => isObservation(entry) && entry.issueId === issue.issueId) || !integer(reply.totalCount) || !(reply.nextCursor === null || integer(reply.nextCursor))
        || reply.totalCount < reply.entries.length || reply.nextCursor !== null && (!reply.entries.length || reply.nextCursor === (visible?.next ?? 0))) throw new Error("问题观点的来源或分页无法核对，请重新读取任务。");
      const entries = [...(visible?.entries ?? []), ...reply.entries];
      if (!unique(entries, entry => entry.observationId)) throw new Error("问题观点分页重复，尚未合并。请重新读取任务。");
      setPage({ scope, entries, next: reply.nextCursor });
    } catch (reason) { if (live.current === scope) setError({ scope, message: reason instanceof Error ? reason.message : "问题观点读取失败。" }); }
    finally { if (flight.current === ticket) { flight.current = null; if (live.current === scope) setPending(null); } }
  };
  useEffect(() => { if (detail.current?.open && !observations.length && issue.observationCount > 0 && !visible && !error) void load(true); }, [scope, issue.issueId]);
  return <details ref={detail} className={`creation-issue is-${issue.status}`} data-creation-issue={issue.issueId}
    onToggle={event => { if (event.currentTarget.open && !observations.length && issue.observationCount > 0 && !visible && !error) void load(true); }}>
    <summary><strong>{issue.summary}</strong><span>{statuses[issue.status]}</span></summary>
    <div className="creation-issue-observations">
      {!visible && issue.hasMoreObservations && observations.length > 0 && <p>仅显示最近 {issue.observations.length} 条观点，共 {issue.observationCount} 条。</p>}
      {!observations.length && pending && <p role="status">正在读取检查观点…</p>}
      {observations.map(observation => <section key={observation.observationId}>
        <p><strong>{observation.source === "revisionOutcome" ? "独立正文检查" : "交付核对"} · {observationStatuses[observation.status]}</strong></p>
        {(observation.recheckRequired || !observation.current || observation.superseded) && <p className="creation-issue-observation-state">{observation.superseded ? "已由后续检查取代" : observation.recheckRequired ? "这条观点仍需重新核对" : "这条观点尚不能代表当前正文"}</p>}
        <p>{observation.reason}</p><ObservationEvidence evidence={observation.evidence} />
        {observation.resolutionOf.length > 0 && <small>这次复核回应了先前 {observation.resolutionOf.length} 条观点；处理状态以问题记录为准。</small>}
        <details><summary>查看检查来源</summary><dl><dt>观点记录</dt><dd>{observation.observationId}</dd><dt>检查任务</dt><dd>{observation.taskId}</dd><dt>正文版本</dt><dd>{observation.draftRevisionId ?? "未绑定正文"}</dd><dt>报告版本</dt><dd>{observation.reportRevisionId ?? "交付记录"}</dd><dt>标准</dt><dd>{observation.criterionId ?? "整体目标"}</dd>
          {observation.resolutionOf.length > 0 && <><dt>回应的观点</dt><dd>{observation.resolutionOf.join("、")}</dd></>}</dl><ObservationEvidence evidence={observation.evidence} technical /></details>
      </section>)}
      {more && api && workTaskId && <button type="button" className="text-button" disabled={pending} onClick={() => void load()}>{pending ? "正在读取观点…" : visible ? "读取更多观点" : "查看完整观点"}</button>}
      {more && (!api || !workTaskId) && <p>完整观点需从本轮任务的问题记录读取。</p>}
      {error && <p role="alert">{error}<button type="button" className="text-button" disabled={pending} onClick={() => void load()}>重试读取</button></p>}
    </div>
  </details>;
}

export function CreationIssueList({ issues }: { issues: CreationIssue[] }) {
  const scope = useMemo(() => ({}), [issues]);
  return issues.length ? <section className="creation-issues" aria-label="相关问题记录">{issues.map(issue => <IssueRow key={issue.issueId} issue={issue} scope={scope} />)}</section> : null;
}

export function WorkIssueScope({ api, projectId, taskId, version, summary, onRetry, children }: {
  api?: NovelCodexApi; projectId: string; taskId: string; version: number; summary: CreationIssueSummary; onRetry: () => void;
  children: (panel: ReactNode) => ReactNode;
}) {
  const summaryKey = JSON.stringify(summary);
  const scope = useMemo(() => ({}), [api, projectId, taskId, version, summaryKey]);
  const live = useRef<object | null>(scope); live.current = scope;
  useEffect(() => () => { if (live.current === scope) live.current = null; }, [scope]);
  const flight = useRef<{ scope: object; ticket: object } | null>(null);
  const panelRef = useRef<HTMLElement>(null);
  const [page, setPage] = useState<{ scope: object; entries: CreationIssue[]; next: number | null } | null>(null);
  const [focus, setFocus] = useState<{ issueId: string } | null>(null), [pendingScope, setPending] = useState<object | null>(null), [failure, setError] = useState<{ scope: object; message: string } | null>(null);
  const pending = pendingScope === scope, error = failure?.scope === scope ? failure.message : null;
  const valid = isSummary(summary), visible = page?.scope === scope ? page : null;
  const entries = visible?.entries ?? (valid ? summary.entries : []);
  const more = visible ? visible.next !== null : valid && summary.hasMore;
  const load = async (requestedIssue?: string) => {
    if (live.current !== scope || flight.current?.scope === scope || !valid) return;
    if (!api?.runtime?.getWorkIssues) { setError({ scope, message: "问题记录读取接口不可用，请重新读取任务。" }); return; }
    const ticket = { scope, ticket: {} }; flight.current = ticket; setPending(scope); setError(null);
    let cursor = visible?.next ?? 0, merged = visible?.entries ?? [];
    const cursors = new Set<number>();
    try {
      do {
        if (cursors.has(cursor)) throw new Error("问题记录分页未推进，请重新读取任务。");
        cursors.add(cursor);
        const reply = await api.runtime.getWorkIssues(projectId, taskId, { issueId: null, cursor, limit: 16 });
        if (live.current !== scope || flight.current !== ticket) return;
        if ("issueId" in reply || !Array.isArray(reply.entries) || reply.entries.length > 16 || !reply.entries.every(entry => isCreationIssue(entry) && entry.observations.length === 0)
          || !integer(reply.totalCount) || !(reply.nextCursor === null || integer(reply.nextCursor))
          || reply.totalCount < reply.entries.length || reply.nextCursor !== null && (!reply.entries.length || reply.nextCursor === cursor)) throw new Error("问题记录的来源或分页无法核对，请重新读取任务。");
        merged = [...merged, ...reply.entries];
        if (!unique(merged, entry => entry.issueId)) throw new Error("问题记录分页重复，尚未合并。请重新读取任务。");
        setPage({ scope, entries: merged, next: reply.nextCursor });
        if (!requestedIssue || merged.some(issue => issue.issueId === requestedIssue)) break;
        if (reply.nextCursor === null) throw new Error("引用的问题未出现在当前任务记录中，请重新读取任务。");
        cursor = reply.nextCursor;
      } while (merged.length < summary.totalCount);
      if (requestedIssue) {
        if (!merged.some(issue => issue.issueId === requestedIssue)) throw new Error("引用的问题未出现在当前任务记录中，请重新读取任务。");
        setFocus({ issueId: requestedIssue });
      }
    } catch (reason) { if (live.current === scope) setError({ scope, message: reason instanceof Error ? reason.message : "问题记录读取失败。" }); }
    finally { if (flight.current === ticket) { flight.current = null; if (live.current === scope) setPending(null); } }
  };
  const open = (issueId: string) => {
    panelRef.current?.scrollIntoView({ block: "nearest" });
    if (entries.some(issue => issue.issueId === issueId)) setFocus({ issueId });
    else void load(issueId);
  };
  const panel = !valid ? <p role="alert">问题记录无法核对。<button type="button" className="text-button" onClick={onRetry}>重新读取任务</button></p>
    : entries.length > 0 || error || pending ? <section ref={panelRef} className="creation-issues" aria-label="本轮问题记录">
      {entries.map(issue => <IssueRow key={issue.issueId} issue={issue} scope={scope} api={api} projectId={projectId} workTaskId={taskId} focus={focus} />)}
      {more && <button type="button" className="text-button" disabled={pending} onClick={() => void load()}>{pending ? "正在读取问题…" : "查看更多问题"}</button>}
      {error && <p role="alert">{error}<button type="button" className="text-button" disabled={pending} onClick={() => void load()}>重试读取</button></p>}
    </section> : null;
  return <IssueContext.Provider value={{ open }}>{children(panel)}</IssueContext.Provider>;
}

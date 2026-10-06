import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import type { CreativeWorkspace, IdentityCatalog, IdentityDecision, NovelCodexApi } from "./types";
import "./identity-decision.css";

type ReadyReview = { id: string; artifactKind: string; artifactRevisionId: string; status: string; projectRevision: number };

function readyIdentityReview(workspace: CreativeWorkspace): ReadyReview | null {
  const candidateId = workspace.artifacts.identity.candidate?.id;
  if (!candidateId) return null;
  return (workspace.reviews as unknown as ReadyReview[]).find((review) =>
    review.artifactKind === "identity" && review.artifactRevisionId === candidateId && review.status === "ready" && review.projectRevision === workspace.revision) ?? null;
}

function currentIdentityCommit(workspace: CreativeWorkspace): string | null {
  const latest = [...workspace.commits].sort((left, right) => right.result_revision - left.result_revision)[0];
  return latest?.operation_kind === "accept" && latest.receipt.artifactKind === "identity" && latest.result_revision === workspace.revision ? latest.id : null;
}

type Props = { api?: NovelCodexApi; projectId: string; refreshToken: number;
  agentRunning?: boolean; onFormalChange: () => Promise<void> };

export function IdentityDecisionPanel(props: Props) {
  return <IdentityDecisionSession key={props.projectId} {...props} />;
}
function IdentityDecisionSession({ api, projectId, refreshToken, agentRunning = false, onFormalChange }: Props) {
  const owner = useMemo(() => ({ active: false, writing: false, serial: 0 }), [api]);
  const running = useRef(agentRunning); running.current = agentRunning;
  useLayoutEffect(() => { owner.active = true; return () => { owner.active = false; owner.serial++; }; }, [owner]);
  const [rawCatalog, setCatalog] = useState<IdentityCatalog | null>(null);
  const [rawWorkspace, setWorkspace] = useState<CreativeWorkspace | null>(null);
  const [dataOwner, setDataOwner] = useState(owner);
  const catalog = dataOwner === owner ? rawCatalog : null;
  const workspace = dataOwner === owner ? rawWorkspace : null;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showAllGroups, setShowAllGroups] = useState(false);
  const [expandedNames, setExpandedNames] = useState<Set<string>>(() => new Set());

  useEffect(() => {
    setShowAllGroups(false);
    setExpandedNames(new Set());
  }, [projectId]);

  const reload = useCallback(async () => {
    if (!api || !owner.active) return;
    const serial = ++owner.serial;
    let result;
    try { result = await Promise.all([api.runtime.getIdentityCatalog(projectId), api.runtime.getCreativeWorkspace(projectId)]); }
    catch (reason) { if (owner.active && serial === owner.serial) throw reason; return; }
    const [nextCatalog, nextWorkspace] = result;
    if (!owner.active || serial !== owner.serial) return;
    if (nextWorkspace.project.id !== projectId) throw new Error("人物资料不属于当前作品。");
    setDataOwner(owner); setCatalog(nextCatalog); setWorkspace(nextWorkspace);
  }, [api, projectId, owner]);
  useEffect(() => {
    setError(null);
    void reload().catch(reason => { if (owner.active) setError(String(reason)); });
  }, [reload, refreshToken]);

  async function stage(nextDecisions: (accepted: IdentityDecision[]) => IdentityDecision[]) {
    if (!api || !owner.active || owner.writing || running.current || !catalog?.ambiguityScanComplete || catalog.truncated) return;
    owner.writing = true;
    setBusy(true);
    setError(null);
    try {
      const [fresh, freshCatalog] = await Promise.all([api.runtime.getCreativeWorkspace(projectId), api.runtime.getIdentityCatalog(projectId)]);
      if (!owner.active || running.current) return;
      if (fresh.project.id !== projectId || !freshCatalog.ambiguityScanComplete || freshCatalog.truncated
        || JSON.stringify(freshCatalog) !== JSON.stringify(catalog)) throw new Error("人物来源已变化，请查看最新资料后再决定。");
      if (readyIdentityReview(fresh)) throw new Error("已有待采用的身份决议，请先处理它。");
      const accepted = fresh.artifacts.identity.accepted?.payload.decisions ?? [];
      const candidate = await api.runtime.saveIdentityDecisions(
        projectId,
        nextDecisions(accepted),
        fresh.revision,
        fresh.artifacts.identity.candidate?.id ?? null,
      );
      if (!owner.active || running.current) return;
      await api.runtime.createReview(projectId, "identity", candidate.revisionId, [], {});
      if (!owner.active) return;
      await reload();
    } catch (reason) {
      if (!owner.active) return;
      setError(String(reason));
      await reload().catch(() => undefined);
    } finally {
      owner.writing = false;
      if (owner.active) setBusy(false);
    }
  }

  async function accept() {
    if (!api || !workspace || !owner.active || owner.writing || running.current) return;
    const review = readyIdentityReview(workspace);
    if (!review) return;
    owner.writing = true;
    setBusy(true);
    setError(null);
    try {
      await api.runtime.acceptReview(projectId, review.id, workspace.revision, {});
      if (!owner.active) return;
      await reload();
      if (owner.active) await onFormalChange();
    } catch (reason) {
      if (!owner.active) return;
      setError(String(reason));
      await reload().catch(() => undefined);
    } finally {
      owner.writing = false;
      if (owner.active) setBusy(false);
    }
  }

  async function reject() {
    if (!api || !workspace || !owner.active || owner.writing || running.current) return;
    const review = readyIdentityReview(workspace);
    if (!review) return;
    owner.writing = true;
    setBusy(true);
    setError(null);
    try {
      await api.runtime.rejectIdentityReview(projectId, review.id, workspace.revision);
      if (!owner.active) return;
      await reload();
    } catch (reason) {
      if (!owner.active) return;
      setError(String(reason));
      await reload().catch(() => undefined);
    } finally {
      owner.writing = false;
      if (owner.active) setBusy(false);
    }
  }

  async function undo() {
    if (!api || !workspace || !owner.active || owner.writing || running.current) return;
    const commitId = currentIdentityCommit(workspace);
    if (!commitId) return;
    owner.writing = true;
    setBusy(true);
    setError(null);
    try {
      await api.runtime.undoCommit(projectId, commitId, workspace.revision);
      if (!owner.active) return;
      await reload();
      if (owner.active) await onFormalChange();
    } catch (reason) {
      if (!owner.active) return;
      setError(String(reason));
      await reload().catch(() => undefined);
    } finally {
      owner.writing = false;
      if (owner.active) setBusy(false);
    }
  }

  const blocked = busy || agentRunning || !!error || !workspace || !catalog;
  const pending = workspace ? readyIdentityReview(workspace) : null;
  const undoCommitId = workspace ? currentIdentityCommit(workspace) : null;
  const acceptedDecisions = workspace?.artifacts.identity.accepted?.payload.decisions ?? [];
  const entries = new Map(catalog?.entries.map((entry) => [entry.id, entry]) ?? []);
  if (catalog && workspace && catalog.ambiguityScanComplete && !catalog.truncated
    && !catalog.ambiguousNames.length && !pending && !undoCommitId && !acceptedDecisions.length && !error) return null;
  return <section className="identity-decision-panel" aria-label="人物身份决议">
    <header><div><h3>人物身份</h3><p>同名只提示可能相关，不会自动合并。决定采用后才改变知识查询。</p></div><button type="button" disabled={!api || busy} onClick={() => { setError(null); void reload().catch((reason: unknown) => { if (owner.active) setError(String(reason)); }); }}>刷新</button></header>
    {!api && <p>本地 Runtime 尚未连接。</p>}
    {catalog?.truncated && <p className="identity-warning">身份目录已截断，暂不允许作出决议。请缩小作品范围后重试。</p>}
    {pending && <div className="identity-pending"><span>待审阅 · {workspace?.artifacts.identity.candidate?.payload.decisions.length ?? 0} 条身份决议</span><div className="identity-actions"><button type="button" disabled={blocked} onClick={() => void reject()}>撤回候选</button><button type="button" disabled={blocked} onClick={() => void accept()}>采用决议</button></div></div>}
    {undoCommitId && <button type="button" className="identity-undo" disabled={blocked} onClick={() => void undo()}>撤销最近一次身份决议</button>}
    {acceptedDecisions.length > 0 && <div className="identity-accepted"><strong>已采用的身份决议</strong>{acceptedDecisions.map((decision, index) => <div className="identity-accepted-row" key={`${decision.kind}-${index}`}>
      <div className="identity-accepted-people"><IdentityReference id={decision.kind === "same" ? decision.fromId : decision.leftId} entries={entries} chapters={workspace?.chapters ?? []} />
        <p className="identity-relation">{decision.kind === "same" ? "与下方人物是同一身份" : "与下方人物是不同身份"}</p>
        <IdentityReference id={decision.kind === "same" ? decision.toId : decision.rightId} entries={entries} chapters={workspace?.chapters ?? []} /></div>
      <button type="button" disabled={blocked || Boolean(pending)} onClick={() => void stage((accepted) => accepted.filter((_, position) => position !== index))}>移除此决议并审阅</button></div>)}</div>}
    {catalog && catalog.ambiguousNames.length === 0 && <p className="identity-empty">当前没有同名的不同身份。</p>}
    {catalog && catalog.ambiguousNames.length > 20 && <button type="button" onClick={() => setShowAllGroups((value) => !value)}>{showAllGroups ? "收起候选" : `显示全部 ${catalog.ambiguousNames.length} 组候选`}</button>}
    {catalog?.ambiguousNames.slice(0, showAllGroups ? undefined : 20).map((group) => {
      const pairs = group.canonicalIds.flatMap((left, index) =>
        group.canonicalIds.slice(index + 1).map((right) => ({ left, right })));
      return <article className="identity-group" key={group.name}>
      <div className="identity-group-heading"><strong>{group.name}</strong><span>{group.authorDecidedDistinct ? "已确认不同" : "待确认"}</span></div>
      {pairs.slice(0, expandedNames.has(group.name) ? undefined : 30).map(({ left, right }) => {
        const leftEntry = entries.get(left);
        const rightEntry = entries.get(right);
        const disabled = blocked || Boolean(pending) || !catalog.ambiguityScanComplete || group.authorDecidedDistinct || !leftEntry || !rightEntry || leftEntry.kind !== rightEntry.kind;
        return <div className="identity-pair" key={`${left}:${right}`}>
          <IdentityReference id={left} entries={entries} chapters={workspace?.chapters ?? []} />
          <IdentityReference id={right} entries={entries} chapters={workspace?.chapters ?? []} />
          <div className="identity-actions"><button type="button" disabled={disabled} onClick={() => void stage((accepted) => [...accepted, { kind: "same", fromId: right, toId: left }])}>确认为同一人物</button><button type="button" disabled={disabled} onClick={() => void stage((accepted) => [...accepted, { kind: "distinct", leftId: left, rightId: right }])}>确认是不同人物</button></div>
        </div>;
      })}
      {pairs.length > 30 && <button type="button" onClick={() => setExpandedNames((current) => {
        const next = new Set(current);
        if (next.has(group.name)) next.delete(group.name); else next.add(group.name);
        return next;
      })}>{expandedNames.has(group.name) ? "收起配对" : `展开其余 ${pairs.length - 30} 个配对`}</button>}
    </article>})}
    {error && <p className="identity-error" role="alert">{error}</p>}
  </section>;
}

function IdentityReference({ id, entries, chapters }: { id: string; entries: Map<string, IdentityCatalog["entries"][number]>; chapters: CreativeWorkspace["chapters"] }) {
  const entry = entries.get(id);
  const names = [...new Set(entry?.claims.flatMap(claim => [claim.displayName, ...claim.aliases]).filter(name => name.trim()) ?? [])];
  const first = entry?.claims[0];
  const chapterName = (objectId: string) => {
    const chapter = chapters.find(item => objectId === `draft-${item.id}`);
    return chapter ? `第 ${chapter.number} 章 · ${chapter.title}` : "本作品其他来源";
  };
  return <div className="identity-person" data-identity-id={id}>
    <strong>{names[0] ?? "人物名称暂缺"}</strong>
    {first ? <><small>{chapterName(first.sourceObjectId)}</small><blockquote>{first.evidence.quote}</blockquote></> : <p className="identity-source-missing">人物来源暂缺，不能凭名称猜测对应身份。</p>}
    <details><summary>名称与来源记录</summary><dl><div><dt>内部身份标识</dt><dd>{id}</dd></div>{names.length > 1 && <div><dt>其他名称</dt><dd>{names.slice(1).join("、")}</dd></div>}</dl>
      {entry?.claims.map((claim, index) => <div className="identity-source-record" key={index}><strong>{chapterName(claim.sourceObjectId)}</strong><blockquote>{claim.evidence.quote}</blockquote><small>正文来源：{claim.sourceObjectId} · 修订：{claim.sourceRevisionId}</small></div>)}
    </details>
  </div>;
}

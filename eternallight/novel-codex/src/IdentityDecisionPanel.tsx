import { useCallback, useEffect, useState } from "react";

import type { CreativeWorkspace, IdentityCatalog, IdentityDecision, NovelCodexApi } from "./types";

type ReadyReview = { id: string; artifactKind: string; artifactRevisionId: string; status: string };

function readyIdentityReview(workspace: CreativeWorkspace): ReadyReview | null {
  const candidateId = workspace.artifacts.identity.candidate?.id;
  if (!candidateId) return null;
  return (workspace.reviews as unknown as ReadyReview[]).find((review) =>
    review.artifactKind === "identity" && review.artifactRevisionId === candidateId && review.status === "ready") ?? null;
}

function currentIdentityCommit(workspace: CreativeWorkspace): string | null {
  const latest = [...workspace.commits].sort((left, right) => right.result_revision - left.result_revision)[0];
  return latest?.operation_kind === "accept" && latest.receipt.artifactKind === "identity" ? latest.id : null;
}

export function IdentityDecisionPanel({ api, projectId, refreshToken, onFormalChange }: {
  api?: NovelCodexApi;
  projectId: string;
  refreshToken: number;
  onFormalChange: () => Promise<void>;
}) {
  const [catalog, setCatalog] = useState<IdentityCatalog | null>(null);
  const [workspace, setWorkspace] = useState<CreativeWorkspace | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showAllGroups, setShowAllGroups] = useState(false);
  const [expandedNames, setExpandedNames] = useState<Set<string>>(() => new Set());

  useEffect(() => {
    setShowAllGroups(false);
    setExpandedNames(new Set());
  }, [projectId]);

  const reload = useCallback(async () => {
    if (!api) return;
    const [nextCatalog, nextWorkspace] = await Promise.all([
      api.runtime.getIdentityCatalog(projectId),
      api.runtime.getCreativeWorkspace(projectId),
    ]);
    setCatalog(nextCatalog);
    setWorkspace(nextWorkspace);
  }, [api, projectId]);

  useEffect(() => {
    let mounted = true;
    if (!api) return;
    void Promise.all([api.runtime.getIdentityCatalog(projectId), api.runtime.getCreativeWorkspace(projectId)])
      .then(([nextCatalog, nextWorkspace]) => {
        if (!mounted) return;
        setCatalog(nextCatalog);
        setWorkspace(nextWorkspace);
        setError(null);
      })
      .catch((reason: unknown) => { if (mounted) setError(String(reason)); });
    return () => { mounted = false; };
  }, [api, projectId, refreshToken]);

  async function stage(nextDecisions: (accepted: IdentityDecision[]) => IdentityDecision[]) {
    if (!api || busy || !catalog?.ambiguityScanComplete) return;
    setBusy(true);
    setError(null);
    try {
      const fresh = await api.runtime.getCreativeWorkspace(projectId);
      if (readyIdentityReview(fresh)) throw new Error("已有待采用的身份决议，请先处理它。");
      const accepted = fresh.artifacts.identity.accepted?.payload.decisions ?? [];
      const candidate = await api.runtime.saveIdentityDecisions(
        projectId,
        nextDecisions(accepted),
        fresh.revision,
        fresh.artifacts.identity.candidate?.id ?? null,
      );
      await api.runtime.createReview(projectId, "identity", candidate.revisionId, [], {});
      await reload();
    } catch (reason) {
      setError(String(reason));
      await reload().catch(() => undefined);
    } finally {
      setBusy(false);
    }
  }

  async function accept() {
    if (!api || !workspace || busy) return;
    const review = readyIdentityReview(workspace);
    if (!review) return;
    setBusy(true);
    setError(null);
    try {
      await api.runtime.acceptReview(projectId, review.id, workspace.revision, {});
      await reload();
      await onFormalChange();
    } catch (reason) {
      setError(String(reason));
      await reload().catch(() => undefined);
    } finally {
      setBusy(false);
    }
  }

  async function reject() {
    if (!api || !workspace || busy) return;
    const review = readyIdentityReview(workspace);
    if (!review) return;
    setBusy(true);
    setError(null);
    try {
      await api.runtime.rejectIdentityReview(projectId, review.id, workspace.revision);
      await reload();
    } catch (reason) {
      setError(String(reason));
      await reload().catch(() => undefined);
    } finally {
      setBusy(false);
    }
  }

  async function undo() {
    if (!api || !workspace || busy) return;
    const commitId = currentIdentityCommit(workspace);
    if (!commitId) return;
    setBusy(true);
    setError(null);
    try {
      await api.runtime.undoCommit(projectId, commitId, workspace.revision);
      await reload();
      await onFormalChange();
    } catch (reason) {
      setError(String(reason));
      await reload().catch(() => undefined);
    } finally {
      setBusy(false);
    }
  }

  const pending = workspace ? readyIdentityReview(workspace) : null;
  const undoCommitId = workspace ? currentIdentityCommit(workspace) : null;
  const acceptedDecisions = workspace?.artifacts.identity.accepted?.payload.decisions ?? [];
  const entries = new Map(catalog?.entries.map((entry) => [entry.id, entry]) ?? []);
  return <section className="identity-decision-panel" aria-label="人物身份决议">
    <header><div><h3>人物身份</h3><p>同名只提示可能相关，不会自动合并。决定采用后才改变知识查询。</p></div><button type="button" disabled={!api || busy} onClick={() => void reload().catch((reason: unknown) => setError(String(reason)))}>刷新</button></header>
    {!api && <p>本地 Runtime 尚未连接。</p>}
    {catalog?.truncated && <p className="identity-warning">身份目录已截断，暂不允许作出决议。请缩小作品范围后重试。</p>}
    {pending && <div className="identity-pending"><span>待审阅 · {workspace?.artifacts.identity.candidate?.payload.decisions.length ?? 0} 条身份决议</span><div className="identity-actions"><button type="button" disabled={busy} onClick={() => void reject()}>撤回候选</button><button type="button" disabled={busy} onClick={() => void accept()}>采用决议</button></div></div>}
    {undoCommitId && <button type="button" className="identity-undo" disabled={busy} onClick={() => void undo()}>撤销最近一次身份决议</button>}
    {acceptedDecisions.length > 0 && <div className="identity-accepted"><strong>已采用的身份决议</strong>{acceptedDecisions.map((decision, index) => <div key={`${decision.kind}-${index}`}><span>{decision.kind === "same" ? `${decision.fromId} 与 ${decision.toId} 是同一身份` : `${decision.leftId} 与 ${decision.rightId} 是不同身份`}</span><button type="button" disabled={busy || Boolean(pending)} onClick={() => void stage((accepted) => accepted.filter((_, position) => position !== index))}>移除此决议并审阅</button></div>)}</div>}
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
        const disabled = busy || Boolean(pending) || !catalog.ambiguityScanComplete || group.authorDecidedDistinct || !leftEntry || !rightEntry || leftEntry.kind !== rightEntry.kind;
        return <div className="identity-pair" key={`${left}:${right}`}>
          <div><span>{leftEntry?.claims[0]?.displayName ?? left} · {left}</span><small>{leftEntry?.claims[0]?.evidence.quote ?? "暂无原文证据"}</small></div>
          <div><span>{rightEntry?.claims[0]?.displayName ?? right} · {right}</span><small>{rightEntry?.claims[0]?.evidence.quote ?? "暂无原文证据"}</small></div>
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

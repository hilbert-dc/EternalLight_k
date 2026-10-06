import { useEffect, useRef, useState } from "react";
import type { CommitmentDetail, CommitmentPage, CommitmentNavigationRequest, DiscussionSource, CreativeWorkspace, HistoricalKnowledgeState, NovelCodexApi } from "./types";
import "./story-reference.css";
import { StoryReferenceControls, type ReferenceFilter } from "./StoryReferenceControls";

export type StoryReferenceKind = "timeline" | "world" | "plot";
type Props = { api?: NovelCodexApi; projectId: string; view: StoryReferenceKind; refreshToken: number;
  onDiscuss: (prompt: string, source?: DiscussionSource) => void; onOpenCommitment: (request: CommitmentNavigationRequest) => void; onOpenChapter: (chapterId: string) => void };
type Data = { api: NovelCodexApi; projectId: string; view: StoryReferenceKind; workspace: CreativeWorkspace;
  knowledge?: HistoricalKnowledgeState; commitments?: CommitmentPage };
const copy = {
  timeline: ["故事时间与事件", "只列出已分析的事件或明确记录的故事时刻，未指定时间不推断先后。"],
  world: ["故事设定", "正文分析中的主张及作者确认状态；人物认知与关系不等于世界真相。"],
  plot: ["伏笔与悬念", "已建立的独立承诺，不代表全部情节线；确认也不代表已经兑现。"],
} as const;
const kinds: Record<string, string> = { world: "故事主张", state: "属性", relation: "关系", epistemic: "人物认知", event: "事件", proposition: "命题" };
const commitmentStates = { open: "尚未兑现", partiallyFulfilled: "部分兑现", fulfilled: "已兑现", abandoned: "已放弃" };
const valueText = (value: unknown) => typeof value === "string" ? value : value === undefined ? "" : JSON.stringify(value);

export function StoryReferenceView({ api, projectId, view, refreshToken, onDiscuss, onOpenCommitment, onOpenChapter }: Props) {
  const [data, setData] = useState<Data | null>(null);
  const [rawLoading, setLoading] = useState(false);
  const [rawError, setError] = useState<string | null>(null);
  const [statusOwner, setStatusOwner] = useState<object | null>(null);
  const [retry, setRetry] = useState(0);
  const [detail, setDetail] = useState<{ owner: Data; value: CommitmentDetail } | null>(null);
  const [rawDetailError, setDetailError] = useState<string | null>(null);
  const [rawPaging, setPaging] = useState(false);
  const [rawTrimmed, setTrimmed] = useState(0);
  const current = useRef({ api, projectId, view, epoch: 0 });
  if (current.current.api !== api || current.current.projectId !== projectId || current.current.view !== view) {
    current.current = { api, projectId, view, epoch: current.current.epoch + 1 };
  }
  const scope = current.current;
  const [filterState, setFilter] = useState<{ owner: typeof scope; value: ReferenceFilter } | null>(null);
  const filter = filterState?.owner === scope ? filterState.value : "all";
  const loading = statusOwner === scope ? rawLoading : Boolean(api && projectId);
  const error = statusOwner === scope ? rawError : null;
  const detailError = statusOwner === scope ? rawDetailError : null;
  const paging = statusOwner === scope && rawPaging;
  const trimmed = statusOwner === scope ? rawTrimmed : 0;
  const readSerial = useRef(0);
  const detailSerial = useRef(0);
  const pageFlight = useRef(false);
  const visible = data && data.api === api && data.projectId === projectId && data.view === view ? data : null;
  useEffect(() => {
    if (!api || !projectId) return;
    let active = true;
    const ticket = ++readSerial.current;
    setStatusOwner(scope); setLoading(true); setError(null); setDetail(null); setDetailError(null); setTrimmed(0);
    pageFlight.current = false; setPaging(false); detailSerial.current += 1;
    void (async () => {
      try {
        const workspace = await api.runtime.getCreativeWorkspace(projectId);
        if (!active || current.current !== scope) return;
        if (workspace.project.id !== projectId) throw new Error("作品资料来源不一致，请重新读取。");
        const next: Data = { api, projectId, view, workspace };
        if (view === "timeline" || view === "world") {
          next.knowledge = await api.runtime.getHistoricalKnowledgeState(projectId, workspace.headSnapshotId);
          if (next.knowledge.snapshotId !== workspace.headSnapshotId
            || (next.knowledge as HistoricalKnowledgeState & { projectId: string }).projectId !== projectId) throw new Error("故事资料快照不一致，请重新读取。");
        } else if (view === "plot") {
          next.commitments = await api.runtime.listCommitments(projectId, null, 20);
          if (next.commitments.snapshotId !== workspace.headSnapshotId) throw new Error("伏笔目录来源已变化，请重新读取。");
        }
        if (active && current.current === scope && readSerial.current === ticket) setData(next);
      } catch (cause) {
        if (active && current.current === scope && readSerial.current === ticket) setError(cause instanceof Error ? cause.message : "资料读取失败");
      } finally { if (active && current.current === scope && readSerial.current === ticket) setLoading(false); }
    })();
    return () => { active = false; readSerial.current += 1; detailSerial.current += 1; };
  }, [api, projectId, view, refreshToken, retry, scope]);

  async function more() {
    if (!api || !visible?.commitments?.nextCursor || loading || error || pageFlight.current) return;
    const owner = visible; const ticket = readSerial.current;
    pageFlight.current = true; setPaging(true);
    try {
      const next = await api.runtime.listCommitments(projectId, owner.commitments!.nextCursor, 20);
      if (current.current !== scope || ticket !== readSerial.current) return;
      if (next.snapshotId !== owner.commitments!.snapshotId) throw new Error("目录快照已变化，请重新读取；不会混合不同版本。");
      const rows = [...owner.commitments!.data, ...next.data];
      setTrimmed(count => count + Math.max(0, rows.length - 200));
      setData({ ...owner, commitments: { ...next, data: rows.slice(-200) } });
    } catch (cause) { if (current.current === scope && ticket === readSerial.current) setError(String(cause)); }
    finally { if (current.current === scope && ticket === readSerial.current) { pageFlight.current = false; setPaging(false); } }
  }
  async function openCommitment(id: string) {
    if (!api || !visible || loading || error) return;
    const owner = visible; const ticket = ++detailSerial.current;
    setDetail(null); setDetailError(null);
    try {
      const value = await api.runtime.getCommitment(projectId, id, owner.workspace.headSnapshotId);
      if (value.projectId !== projectId || value.payload.id !== id || value.snapshotId !== owner.workspace.headSnapshotId) throw new Error("伏笔详情来源不一致，请重新读取。");
      if (current.current === scope && ticket === detailSerial.current) setDetail({ owner, value });
    } catch (cause) { if (current.current === scope && ticket === detailSerial.current) setDetailError(String(cause)); }
  }
  function source(objectId: string, revisionId: string) {
    const chapter = visible?.workspace.chapters.find(chapter => objectId === `draft-${chapter.id}`);
    const status = chapter?.accepted?.id === revisionId ? "已发布正文" : chapter?.candidate?.id === revisionId ? "已保存编辑稿" : "历史分析来源";
    return <div className="story-reference-source"><span>{chapter ? `第 ${chapter.number} 章 · ${chapter.title}` : "本作品其他来源"} · {status}</span>
      {chapter && <button type="button" className="text-button" onClick={() => onOpenChapter(chapter.id)}>查看所在章节</button>}</div>;
  }
  const entries = visible?.knowledge?.entries ?? [];
  const timeline = entries.filter(entry => entry.assertion.kind === "event" || typeof entry.assertion.storyTime === "number")
    .sort((left, right) => (left.assertion.storyTime ?? Infinity) - (right.assertion.storyTime ?? Infinity));
  const shownTimeline = timeline.filter(entry => filter === "all" || (filter === "known" ? typeof entry.assertion.storyTime === "number" : entry.assertion.storyTime == null));
  const commitments = visible?.commitments?.data ?? [];
  const shownCommitments = commitments.filter(item => filter === "all" || (filter === "fulfilled" ? item.status === "fulfilled" : item.status === "open" || item.status === "partiallyFulfilled"));
  const world = entries.filter(entry => entry.assertion.kind !== "event");
  function claim(entry: HistoricalKnowledgeState["entries"][number]) {
    const quote = entry.assertion.evidence?.quote;
    const sourceChapter = visible?.workspace.chapters.find(chapter => entry.sourceObjectId === `draft-${chapter.id}`);
    const sourceLabel = `${sourceChapter ? `第 ${sourceChapter.number} 章` : "本作品资料"} · ${(entry.assertion.predicate || "原文依据").slice(0, 30)} · ${(quote || "暂无摘录").slice(0, 40)}`;
    return <article className="story-reference-card" key={`${entry.modelingBatchId}:${entry.assertionIndex}`}
      data-discussion-revision={entry.sourceRevisionId} data-discussion-object={entry.sourceObjectId}
      data-discussion-locator={`${entry.modelingBatchId}:${entry.assertionIndex}`} data-discussion-label={sourceLabel}>
      <header><strong>{entry.assertion.predicate || kinds[entry.assertion.kind] || "正文主张"}</strong><span>{entry.confirmationState === "confirmed" ? "作者已确认" : "未确认候选"} · {kinds[entry.assertion.kind] || "正文主张"}</span></header>
      {entry.assertion.kind !== "event" && entry.assertion.value !== undefined && <p className="story-reference-value">{valueText(entry.assertion.value)}</p>}
      {quote && <blockquote>{quote}</blockquote>}
      {source(entry.sourceObjectId, entry.sourceRevisionId)}
      <button type="button" className="secondary-button" disabled={loading || Boolean(error)} onClick={() => onDiscuss(`请核对这条${kinds[entry.assertion.kind] || "正文主张"}的依据，先讨论，不改写正文。`, { label: sourceLabel, focus: { view, reference: { kind: "claim", projectId, snapshotId: visible!.workspace.headSnapshotId, objectId: entry.sourceObjectId, revisionId: entry.sourceRevisionId, modelingBatchId: entry.modelingBatchId, assertionIndex: entry.assertionIndex } } })}>在对话中讨论</button>
    </article>;
  }
  const [title, description] = copy[view];
  return <section className="view-panel story-reference" aria-label={title} aria-busy={loading || paging}
    data-discussion-revision="" data-discussion-snapshot={visible?.workspace.headSnapshotId}>
    <header className="view-intro"><p>{description}</p><button type="button" className="secondary-button" disabled={!api || !projectId || loading} onClick={() => setRetry(value => value + 1)}>重新读取资料</button></header>
    {view !== "world" && <StoryReferenceControls view={view} filter={filter} count={view === "timeline" ? shownTimeline.length : shownCommitments.length}
      loaded={view === "timeline" ? timeline.length : commitments.length} ready={Boolean(visible)} discussDisabled={!api || !projectId || loading || Boolean(error)}
      onFilter={value => { if (current.current === scope) setFilter({ owner: scope, value }); }} onDiscuss={onDiscuss} />}
    {!api || !projectId ? <p role="status">请先打开作品并连接本地运行时。</p> : <>
      {loading && <p role="status">{visible ? "正在更新，暂时保留上次读取的资料…" : "正在读取作品资料…"}</p>}
      {error && <p role="alert">资料读取失败：{error}{visible && "。以下为上次读取结果。"}</p>}
      {visible?.knowledge && (!visible.knowledge.coverage.complete || visible.knowledge.coverage.truncatedEntries || visible.knowledge.coverage.pendingSources.length > 0) && <p role="status">已读取范围不完整或仍有待分析正文；以下资料不代表全作品完整。</p>}
      {visible && <>
        {view === "timeline" && (shownTimeline.length ? <>
          {shownTimeline.some(entry => typeof entry.assertion.storyTime === "number") && <section className="story-reference-group"><h3>已记录的故事时刻</h3><p>数值是故事时刻，不能直接解释为日期或章节顺序。</p>
            {shownTimeline.filter(entry => typeof entry.assertion.storyTime === "number").map(entry => <div key={`${entry.modelingBatchId}:${entry.assertionIndex}`}><p>故事时刻 {entry.assertion.storyTime}</p>{claim(entry)}</div>)}</section>}
          {shownTimeline.some(entry => entry.assertion.storyTime == null) && <section className="story-reference-group"><h3>时间尚未指定</h3>{shownTimeline.filter(entry => entry.assertion.storyTime == null).map(claim)}</section>}
        </> : filter === "all" ? <div className="empty-state"><strong>已读取范围暂无事件或明确故事时刻</strong><p>可用上方对话入口分析已保存正文；没有记录不代表故事没有事件。</p></div> : null)}
        {view === "world" && (world.length ? <div className="story-reference-list">{world.map(claim)}</div> : <div className="empty-state"><strong>已读取范围暂无故事设定主张</strong><p>可以讨论并分析已写正文。属性与事件结构规则由下方独立管理。</p></div>)}
        {view === "plot" && <>
          {!commitments.length && filter === "all" && <div className="empty-state"><strong>此快照尚无已建立伏笔或悬念</strong><p>可用上方对话入口提取已保存正文的伏笔建议；提取建议需在章节中确认后才进入目录。</p></div>}
          <div className="story-reference-list">{shownCommitments.map(item => <article className="story-reference-card" key={item.id}
            data-discussion-revision={item.revisionId} data-discussion-object={item.id}
            data-discussion-locator={item.id} data-discussion-label={item.summary}>
            <header><strong>{item.summary}</strong><span>{item.confirmationState === "confirmed" ? "作者已确认" : "未确认候选"} · {commitmentStates[item.status]}</span></header>
            {!item.sourceCurrent && <p>正文证据已变化，需重新核对。</p>}
            <div className="story-reference-actions"><button type="button" className="secondary-button" disabled={loading || Boolean(error)} onClick={() => void openCommitment(item.id)}>查看原文与记录</button>
              <button type="button" className="text-button" disabled={loading || Boolean(error) || !item.sourceCurrent} onClick={() => onDiscuss("请讨论这条伏笔的铺垫与兑现方向，先不要修改或确认。", { label: `伏笔 · ${item.summary.slice(0, 60)}`, focus: { view: "plot", reference: { kind: "commitment", projectId, snapshotId: visible.workspace.headSnapshotId, commitmentId: item.id, revisionId: item.revisionId } } })}>在对话中讨论</button><button type="button" className="text-button" disabled={loading || Boolean(error) || !item.sourceCurrent} onClick={() => onOpenCommitment({ projectId, snapshotId: visible.workspace.headSnapshotId, commitmentId: item.id, revisionId: item.revisionId })}>到章节查看与决定</button></div>
          </article>)}</div>
          {visible.commitments?.nextCursor && <><p>还有未加载的承诺，当前列表不是完整目录。</p><button type="button" className="secondary-button" disabled={paging || loading || Boolean(error)} onClick={() => void more()}>{paging ? "正在读取…" : "加载更多"}</button></>}
          {trimmed > 0 && <p>已收起前 {trimmed} 条加载结果；对象未删除，重新读取可从目录首批开始。</p>}
          {detailError && <p role="alert">详情读取失败：{detailError}。可再次点击原条目只读核对。</p>}
          {detail?.owner === visible && <section className="story-reference-card" aria-label="伏笔来源详情"><h3>{detail.value.payload.summary}</h3><p>兑现标准：{detail.value.payload.fulfillmentCriteria}</p>
            {detail.value.payload.setupEvidenceRefs.map((anchor, index) => <div key={index}><blockquote>{anchor.quote}</blockquote>{source(anchor.objectId, anchor.revisionId)}</div>)}
            <button type="button" className="text-button" onClick={() => setDetail(null)}>收起记录</button></section>}
        </>}
      </>}
    </>}
  </section>;
}

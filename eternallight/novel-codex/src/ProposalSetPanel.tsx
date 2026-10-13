import { useEffect, useMemo, useRef, useState } from "react";

import type { CreativeWorkspace, NovelCodexApi, ProposalSetDetail, ProposalSetSummary } from "./types";
import { defaultProposalChoices, proposalKindLabels, proposalReviewCandidates, selectedProposalChoices } from "./proposal-selection";
import { ProposalFactChoices } from "./ProposalFactChoices";

type Props = {
  api: NovelCodexApi;
  projectId: string;
  workspace: CreativeWorkspace;
  busy: boolean;
  dirty: boolean;
  agentRunning: boolean;
  refreshToken: number;
  onFormalChange: () => Promise<void>;
};

async function operationKey(payload: unknown): Promise<string> {
  const bytes = new TextEncoder().encode(JSON.stringify(payload));
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return `proposal-ui-${Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

export function ProposalSetPanel({ api, projectId, workspace, busy, dirty, agentRunning, refreshToken, onFormalChange }: Props) {
  const [intent, setIntent] = useState("");
  const [selectedKinds, setSelectedKinds] = useState<string[]>([]);
  const [dependencies, setDependencies] = useState<Record<string, string[]>>({});
  const [sets, setSets] = useState<ProposalSetSummary[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [detail, setDetail] = useState<ProposalSetDetail | null>(null);
  const [selectedOperationIds, setSelectedOperationIds] = useState<string[]>([]);
  const [choices, setChoices] = useState<Record<string, Record<string, unknown>>>({});
  const [previewedSelection, setPreviewedSelection] = useState<string | null>(null);
  const previewEpoch = useRef(0);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const availableReviews = useMemo(() => proposalReviewCandidates(workspace), [workspace]);
  const selectedReviews = availableReviews.filter((item) => selectedKinds.includes(item.id));
  const locked = busy || dirty || agentRunning || pending;

  useEffect(() => {
    setActiveId(null);
    setDetail(null);
    setSets([]);
    setSelectedKinds([]);
    setDependencies({});
    setChoices({});
    setPreviewedSelection(null);
    previewEpoch.current += 1;
  }, [projectId]);

  useEffect(() => {
    if (!api.runtime.listProposalSets) return;
    let alive = true;
    void api.runtime.listProposalSets(projectId).then((page) => {
      if (!alive) return;
      setSets((current) => [...page.data, ...current.filter((item) => item.id === activeId && !page.data.some((latest) => latest.id === item.id))]);
      setNextCursor(page.nextCursor);
      setActiveId((current) => current ?? page.data[0]?.id ?? null);
    }).catch((reason: unknown) => { if (alive) setError(reason instanceof Error ? reason.message : "提案列表加载失败"); });
    return () => { alive = false; };
  }, [api, projectId, workspace.revision, workspace.headSnapshotId, refreshToken]);

  useEffect(() => {
    if (!api.runtime.getProposalSet || !api.runtime.previewProposalSet) return;
    if (!activeId) { setDetail(null); return; }
    const epoch = ++previewEpoch.current;
    setPreviewedSelection(null);
    setDetail(null);
    let alive = true;
    void api.runtime.getProposalSet(projectId, activeId).then((result) => {
      if (!alive || epoch !== previewEpoch.current) return;
      const all = result.application ? result.application.selectedOperationIds : result.operations.map((operation) => operation.operationId);
      const initialChoices = defaultProposalChoices(result);
      const selectedChoices = selectedProposalChoices(all, initialChoices);
      setDetail(result);
      setChoices(initialChoices);
      setSelectedOperationIds(all);
      return api.runtime.previewProposalSet(projectId, activeId, all, selectedChoices).then((preview) => ({ preview, fingerprint: JSON.stringify({ ids: all, choices: selectedChoices }) }));
    }).then((result) => { if (alive && result && epoch === previewEpoch.current) {
      setDetail(result.preview);
      setPreviewedSelection(result.fingerprint);
    } })
      .catch((reason: unknown) => { if (alive && epoch === previewEpoch.current) { setDetail(null); setError(reason instanceof Error ? reason.message : "提案加载失败"); } });
    return () => { alive = false; previewEpoch.current += 1; };
  }, [api, projectId, activeId, workspace.revision, workspace.headSnapshotId, refreshToken]);

  if (!api.runtime.listProposalSets) return null;

  async function perform(action: () => Promise<void>) {
    if (locked) return;
    setPending(true);
    setError(null);
    setNotice(null);
    try { await action(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : "提案操作失败"); }
    finally { setPending(false); }
  }

  async function create() {
    if (selectedReviews.length < 2 || selectedReviews.length > 20 || !intent.trim()) return;
    await perform(async () => {
      const operations = selectedReviews.map(({ id, review }) => ({
        operationId: id, reviewId: review.id,
        dependsOn: (dependencies[id] ?? []).filter((dependency) => selectedKinds.includes(dependency)),
      }));
      const request = { baseSnapshotId: workspace.headSnapshotId, intent: intent.trim(), operations };
      const saved = await api.runtime.createProposalSet(projectId, { ...request, idempotencyKey: await operationKey({ projectId, request }) });
      const page = await api.runtime.listProposalSets(projectId);
      setSets(page.data);
      setNextCursor(page.nextCursor);
      setActiveId(saved.id);
      setNotice("提案已保存为候选；尚未改变正式作品。请检查依赖后再提交。");
    });
  }

  async function previewSelection(ids: string[], nextChoices = choices) {
    if (!activeId) return;
    const epoch = ++previewEpoch.current;
    setPreviewedSelection(null);
    setError(null);
    setSelectedOperationIds(ids);
    setChoices(nextChoices);
    const selectedChoices = selectedProposalChoices(ids, nextChoices);
    try {
      const report = await api.runtime.previewProposalSet(projectId, activeId, ids, selectedChoices);
      if (epoch !== previewEpoch.current) return;
      setDetail(report);
      setPreviewedSelection(JSON.stringify({ ids, choices: selectedChoices }));
    }
    catch (reason) { if (epoch === previewEpoch.current) setError(reason instanceof Error ? reason.message : "依赖预览失败"); }
  }

  async function apply() {
    if (!activeId || !detail || detail.status !== "ready" || !detail.selection.applyReady || !previewCurrent) return;
    await perform(async () => {
      const selectionsByOperationId = selectedProposalChoices(selectedOperationIds, choices);
      const request = { selectedOperationIds, selectionsByOperationId };
      await api.runtime.applyProposalSet(projectId, activeId, { ...request, idempotencyKey: await operationKey({ projectId, activeId, request }) });
      setNotice("整组候选已采用，作品版本只推进一次。");
      await onFormalChange();
    });
  }

  async function undo() {
    if (!detail?.application || detail.status !== "applied" || detail.application.resultRevision !== workspace.revision) return;
    await perform(async () => {
      const applicationId = detail.application!.id;
      await api.runtime.undoProposalSet(projectId, applicationId, workspace.revision, `proposal-undo-${applicationId}`);
      setNotice("整组采用已撤销；成员历史与撤销证据保留。");
      await onFormalChange();
    });
  }

  async function loadMore() {
    if (!nextCursor || pending) return;
    setPending(true);
    try {
      const page = await api.runtime.listProposalSets(projectId, nextCursor);
      setSets((current) => [...current, ...page.data.filter((item) => !current.some((prior) => prior.id === item.id))]);
      setNextCursor(page.nextCursor);
    } catch (reason) { setError(reason instanceof Error ? reason.message : "提案列表加载失败"); }
    finally { setPending(false); }
  }

  const previewCurrent = detail !== null && previewedSelection === JSON.stringify({ ids: selectedOperationIds, choices: selectedProposalChoices(selectedOperationIds, choices) });

  return <details className="proposal-set-panel">
    <summary>跨对象提案 · 整组审阅与采用</summary>
    <p>可把规划、正文与身份候选一起采用。正文操作采用整份候选；场景选择只控制确认事实的来源，不裁剪正文。声明依赖由作者确认。</p>
    <div className="proposal-set-section">
      <strong>创建提案</strong>
      <label>本次共同修改的目标<input value={intent} maxLength={1000} disabled={locked} onChange={(event) => setIntent(event.target.value)} placeholder="例如：同时修订主线和近章节奏" /></label>
      {availableReviews.length < 2 && <small>至少需要两份当前作品版本的已审阅候选（规划、正文或身份）。</small>}
      {availableReviews.map(({ id, label, review }) => <div key={id} className="proposal-set-review">
        <label><input type="checkbox" checked={selectedKinds.includes(id)} disabled={locked} onChange={(event) => setSelectedKinds((current) => event.target.checked ? [...current, id] : current.filter((value) => value !== id))} />{label} · {review.id}</label>
        {selectedKinds.includes(id) && availableReviews.filter((prior) => prior.id !== id && selectedKinds.includes(prior.id)).map((prior) => <label key={prior.id} className="proposal-set-dependency"><input type="checkbox" checked={(dependencies[id] ?? []).includes(prior.id)} disabled={locked} onChange={(event) => setDependencies((current) => ({ ...current, [id]: event.target.checked ? [...(current[id] ?? []), prior.id] : (current[id] ?? []).filter((value) => value !== prior.id) }))} />{label}依赖{prior.label}</label>)}
      </div>)}
      <small>一组最多选择 20 份候选，依赖不能形成循环。</small>
      <button type="button" className="secondary-button" disabled={locked || !intent.trim() || selectedReviews.length < 2 || selectedReviews.length > 20} onClick={() => void create()}>保存提案候选</button>
    </div>
    <div className="proposal-set-section">
      <strong>已保存的提案</strong>
      {sets.length === 0 ? <small>还没有跨对象提案。</small> : <label>选择提案<select value={activeId ?? ""} disabled={pending} onChange={(event) => setActiveId(event.target.value)}>{sets.map((set) => <option key={set.id} value={set.id}>{set.intent} · {set.status}</option>)}</select></label>}
      {nextCursor && <button type="button" className="text-button" disabled={pending} onClick={() => void loadMore()}>加载更早提案</button>}
      {detail && <div className="proposal-set-detail">
        <small>状态：{detail.status} · 基线版本 {detail.baseProjectRevision} · 当前版本 {workspace.revision}</small>
        {detail.operations.map((operation) => <div key={operation.operationId} className="proposal-set-operation">
          <label><input type="checkbox" checked={selectedOperationIds.includes(operation.operationId)} disabled={locked || detail.status !== "ready"} onChange={(event) => void previewSelection(event.target.checked ? [...selectedOperationIds, operation.operationId] : selectedOperationIds.filter((id) => id !== operation.operationId))} />{proposalKindLabels[operation.objectKind] ?? operation.objectKind} · {operation.objectId}{operation.dependsOn.length > 0 && <small>依赖：{operation.dependsOn.join("、")}</small>}</label>
          {operation.objectKind === "draft" && selectedOperationIds.includes(operation.operationId) && <ProposalFactChoices api={api} projectId={projectId} options={detail.reviewOptions[operation.operationId]} choices={choices[operation.operationId]} disabled={locked || detail.status !== "ready"} onChange={(next) => void previewSelection(selectedOperationIds, { ...choices, [operation.operationId]: next })} />}
        </div>)}
        {detail.selection.missingDependencies.length > 0 && <p className="proposal-set-warning">缺少声明依赖：{detail.selection.missingDependencies.map((item) => `${item.operationId} → ${item.requires}`).join("；")}</p>}
        {detail.staleOperations.length > 0 && <p className="proposal-set-warning">候选或审阅已过期：{detail.staleOperations.join("、")}</p>}
        <small>预览检查声明依赖、审阅、选择及事实来源；提交时仍会再次校验版本与写入冲突。不保证全部语义依赖已被发现。</small>
        {detail.status === "ready" && <button type="button" className="primary-button" disabled={locked || !previewCurrent || !detail.selection.applyReady} onClick={() => void apply()}>整组采用（提交时终验）</button>}
        {detail.status === "applied" && <button type="button" className="secondary-button" disabled={locked || detail.application?.resultRevision !== workspace.revision} onClick={() => void undo()}>整组撤销</button>}
        {detail.status === "applied" && detail.application?.resultRevision !== workspace.revision && <small>作品已有后续变更，不能直接撤销此组。</small>}
      </div>}
    </div>
    {error && <p className="proposal-set-error" role="alert">{error}</p>}
    {notice && <p role="status">{notice}</p>}
  </details>;
}

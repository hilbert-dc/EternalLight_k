import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { CreativeWorkspace, NovelCodexApi, PropertyRuleDefinition, PropertyRulesSaveRequest, EventRuleDefinition } from "./types";
import { EventRuleSummary } from "./EventRuleFields";
import { PropertyRuleFields, PropertyRuleSummary, ruleInputError } from "./PropertyRuleFields";
import type { TaskResultDestination } from "./task-results";
import "./property-rules.css";

type Props = {
  api?: NovelCodexApi; projectId: string; refreshToken: number; onFormalChange: () => Promise<void>;
  onDirtyChange: (dirty: boolean) => void;
  onSaveHandlerChange: (handler: (() => Promise<boolean>) | null) => void;
  onDiscardHandlerChange: (handler: (() => boolean) | null) => void;
  requestedResult?: { requestId: string; result: Extract<TaskResultDestination, { kind: "propertyRules" }> } | null;
  agentRunning?: boolean;
};
type Edit = { rules: PropertyRuleDefinition[]; eventRules: EventRuleDefinition[]; revision: number; candidateId: string | null; request: PropertyRulesSaveRequest | null; savedId: string | null };

export function PropertyRulesPanel(props: Props) {
  return <PropertyRulesSession key={props.projectId} {...props} />;
}

function PropertyRulesSession({ api, projectId, refreshToken, onFormalChange, onDirtyChange, onSaveHandlerChange, onDiscardHandlerChange, requestedResult, agentRunning = false }: Props) {
  const owner = useMemo(() => ({ active: false, writing: false, read: null as Promise<boolean> | null, queued: false,
    workspace: null as CreativeWorkspace | null, readError: null as string | null, epoch: 0, preparation: null as object | null }), [api]);
  const [data, setData] = useState<{ owner: typeof owner; workspace: CreativeWorkspace } | null>(null);
  const [reading, setReading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [readError, setReadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [edit, setEdit] = useState<Edit | null>(null);
  const editRef = useRef<Edit | null>(null);
  const controls = useRef({ requestedResult, agentRunning });
  controls.current = { requestedResult, agentRunning };
  const preparationAttempts = useRef(new Map<string, string | null>());

  function changeEdit(next: Edit | null) { editRef.current = next; setEdit(next); }
  function reload(): Promise<boolean> {
    if (!api || !owner.active) return Promise.resolve(false);
    if (owner.read) { owner.queued = true; return owner.read; }
    owner.read = (async () => {
      let success = false;
      do {
        owner.queued = false;
        setReading(true);
        try {
          const workspace = await api.runtime.getCreativeWorkspace(projectId);
          if (!owner.active) return false;
          if (workspace.project.id !== projectId) throw new Error("读取结果属于其他作品，不能判断当前规则。");
          const artifact = workspace.artifacts.propertyRules;
          if (!artifact) throw new Error("工作区缺少当前规则契约，不能判断规则状态。");
          for (const revision of [artifact.candidate, artifact.accepted]) {
            if (revision) {
              const invalid = ruleInputError(revision.payload.definitions, revision.payload.eventRules);
              if (invalid) throw new Error(`规则修订不可解析：${invalid}`);
            }
          }
          owner.workspace = workspace;
          owner.readError = null;
          setData({ owner, workspace }); setReadError(null);
          success = true;
        } catch (reason) {
          if (!owner.active) return false;
          owner.readError = reason instanceof Error ? reason.message : "读取规则失败";
          setReadError(owner.readError); success = false;
        }
      } while (owner.active && owner.queued);
      if (owner.active) setReading(false);
      return success;
    })().finally(() => { owner.read = null; });
    return owner.read;
  }
  useLayoutEffect(() => {
    owner.active = true; owner.epoch += 1;
    changeEdit(null); setBusy(false); setReadError(null); setError(null); setNotice(null);
    void reload();
    return () => { owner.active = false; owner.epoch += 1; if (owner.preparation) { owner.preparation = null; owner.writing = false; } };
  }, [owner]);
  const signal = useRef({ owner, refreshToken });
  useEffect(() => {
    const before = signal.current; signal.current = { owner, refreshToken };
    if (before.owner === owner && before.refreshToken !== refreshToken) void reload();
  }, [owner, refreshToken]);

  async function prepareRequestedReview() {
    const request = controls.current.requestedResult;
    if (!api || !request || !owner.active || owner.writing) return;
    if (controls.current.agentRunning || editRef.current) { setError("请等待当前创作结束，并保存或放弃规则编辑后，再检查这份规则结果。"); return; }
    const epoch = owner.epoch; const token = {};
    const current = () => owner.active && owner.epoch === epoch && owner.preparation === token
      && controls.current.requestedResult?.requestId === request.requestId;
    const available = () => current() && !controls.current.agentRunning && !editRef.current;
    owner.preparation = token; owner.writing = true; setBusy(true); setError(null); setNotice(null);
    try {
      if (!await reload() || !current()) return;
      if (!available()) throw new Error("创作或规则编辑状态已变化，请结束后重新检查。");
      const workspace = owner.workspace!;
      if (request.result.kind !== "propertyRules" || request.result.projectId !== projectId
        || workspace.artifacts.propertyRules.candidate?.id !== request.result.referenceId) {
        throw new Error("这份对话结果已不是当前规则候选，请从对话打开最新结果。");
      }
      if (workspace.artifacts.propertyRules.locked) throw new Error("当前规则已锁定，不能建立检查。");
      const revisionId = request.result.referenceId;
      const attemptKey = `${workspace.revision}:${revisionId}`;
      let reviewId = workspace.reviews.find(review => review.artifactKind === "propertyRules"
        && review.artifactRevisionId === revisionId && review.projectRevision === workspace.revision && review.status === "ready")?.id;
      if (!reviewId) {
        if (preparationAttempts.current.has(attemptKey)) throw new Error("检查请求已提交，但尚未读到对应检查。请核对结果，不要重复建立检查。");
        if (!available()) return;
        preparationAttempts.current.set(attemptKey, null);
        if (preparationAttempts.current.size > 8) preparationAttempts.current.delete(preparationAttempts.current.keys().next().value!);
        const receipt = await api.runtime.createReview(projectId, "propertyRules", revisionId, [], {});
        if (!current()) return;
        if (receipt.artifactKind !== "propertyRules" || receipt.artifactRevisionId !== revisionId
          || receipt.projectRevision !== workspace.revision || receipt.status !== "ready") throw new Error("检查回执与当前规则不一致，请重新核对。");
        reviewId = receipt.id; preparationAttempts.current.set(attemptKey, reviewId);
      }
      if (!await reload() || !current()) return;
      if (!available()) throw new Error("创作或规则编辑状态已变化，请结束后重新检查。");
      const fresh = owner.workspace!;
      if (fresh.artifacts.propertyRules.locked || fresh.artifacts.propertyRules.candidate?.id !== revisionId || !fresh.reviews.some(review => review.id === reviewId
        && review.artifactKind === "propertyRules" && review.artifactRevisionId === revisionId
        && review.projectRevision === fresh.revision && review.status === "ready")) throw new Error("规则或作品版本已变化，请重新打开当前结果检查。");
      setNotice("规则检查已准备，请查看检查结果后决定是否采用；尚未修改正式规则或确认事实。");
    } catch (reason) {
      if (current()) setError(reason instanceof Error ? reason.message : "规则检查结果未确认，请重新核对。");
    } finally {
      if (owner.preparation === token) { owner.preparation = null; owner.writing = false; if (owner.active && owner.epoch === epoch) setBusy(false); }
    }
  }
  useEffect(() => {
    const before = owner.preparation;
    if (requestedResult) void prepareRequestedReview();
    const token = owner.preparation;
    return () => {
      if (token && token !== before && owner.preparation === token) {
        owner.preparation = null; owner.writing = false;
        if (owner.active) setBusy(false);
      }
    };
  }, [owner, requestedResult?.requestId]);

  function discard(): boolean {
    if (!owner.active || owner.writing) return false;
    changeEdit(null);
    setNotice("仅放弃本地输入；已保存或结果未知的候选不会被撤销。");
    void reload(); return true;
  }
  async function save(): Promise<boolean> {
    const session = editRef.current;
    if (!api || !owner.active || owner.writing || controls.current.agentRunning) return false;
    if (!session) return true;
    const validation = ruleInputError(session.rules, session.eventRules);
    if (validation) { setError(validation); return false; }
    if (!session.request && (owner.read || owner.readError || owner.workspace?.artifacts.propertyRules.locked)) return false;
    if (!session.request && (owner.workspace?.revision !== session.revision
      || (owner.workspace?.artifacts.propertyRules.candidate?.id ?? null) !== session.candidateId)) {
      setError("作品或候选版本已变化，不能保存失效的编辑基线。"); return false;
    }
    session.request ??= { payload: { definitions: structuredClone(session.rules), eventRules: structuredClone(session.eventRules) }, expectedRevision: session.revision,
      expectedCandidateRevisionId: session.candidateId, idempotencyKey: `property-rules-${crypto.randomUUID()}` };
    owner.writing = true; setBusy(true); setError(null); setNotice(null); setEdit({ ...session });
    try {
      if (!session.savedId) {
        const receipt = await api.runtime.savePropertyRules(projectId, session.request);
        if (!owner.active) return false;
        session.savedId = receipt.revisionId;
      }
      if (!await reload() || !owner.active) return false;
      if (owner.workspace?.artifacts.propertyRules.candidate?.id !== session.savedId) {
        setError("保存回执已确认，但当前候选已被其他修改替换。请放弃本地输入后查看当前版本。"); return false;
      }
      changeEdit(null); setNotice("规则已保存为候选，尚未采用，也没有确认任何属性值。");
      return true;
    } catch (reason) {
      if (owner.active) setError(`保存结果未确认：${reason instanceof Error ? reason.message : "请使用原请求重试或放弃本地输入"}`);
      return false;
    } finally {
      owner.writing = false;
      if (owner.active) { setBusy(false); if (editRef.current) setEdit({ ...editRef.current }); }
    }
  }
  useLayoutEffect(() => {
    onDirtyChange(!!edit || busy);
    onSaveHandlerChange(save); onDiscardHandlerChange(discard);
    return () => { onDirtyChange(false); onSaveHandlerChange(null); onDiscardHandlerChange(null); };
  }, [owner, edit, busy, onDirtyChange, onSaveHandlerChange, onDiscardHandlerChange]);

  async function mutate(action: () => Promise<unknown>, success: string, formal: boolean) {
    if (!api || !owner.active || owner.writing || controls.current.agentRunning || owner.read || owner.readError || editRef.current || !owner.workspace || owner.workspace.artifacts.propertyRules.locked) return;
    owner.writing = true; setBusy(true); setError(null); setNotice(null);
    let succeeded = false;
    try {
      await action(); succeeded = true;
      if (!owner.active) return;
      setNotice(success);
    } catch (reason) {
      if (owner.active) setError(`操作结果未确认：${reason instanceof Error ? reason.message : "请重新读取，不要自动重复操作"}`);
    }
    try {
      if (!owner.active) return;
      await reload();
      if (owner.active && succeeded && formal) await onFormalChange();
    } catch (reason) {
      if (owner.active && succeeded) setError(`操作已返回成功，但作品界面刷新失败：${reason instanceof Error ? reason.message : "请重新读取"}`);
    } finally { owner.writing = false; if (owner.active) setBusy(false); }
  }

  const workspace = data?.owner === owner ? data.workspace : null;
  const artifact = workspace?.artifacts.propertyRules;
  const candidate = artifact?.candidate;
  const accepted = artifact?.accepted;
  const readyReviews = workspace?.reviews.filter((review) => review.artifactKind === "propertyRules" && review.artifactRevisionId === candidate?.id && review.status === "ready") ?? [];
  const pending = readyReviews.find((review) => review.projectRevision === workspace?.revision) ?? readyReviews[0];
  const latest = workspace?.commits.reduce<CreativeWorkspace["commits"][number] | null>((best, item) => !best || item.result_revision > best.result_revision ? item : best, null);
  const undo = latest?.operation_kind === "accept" && latest.receipt.artifactKind === "propertyRules"
    && latest.result_revision === workspace?.revision && !latest.proposalApplicationId ? latest : null;
  const blocked = !api || !workspace || reading || !!readError || busy || agentRunning || !!artifact?.locked || !!edit;
  const changed = !!edit && (workspace?.revision !== edit.revision || (candidate?.id ?? null) !== edit.candidateId);
  return <section className="property-rules-panel" aria-label="作者属性规则" data-discussion-snapshot={workspace?.headSnapshotId}
    data-discussion-revision={candidate?.id ?? ""} data-discussion-object="artifact-propertyRules">
    <header><div><h3>世界属性与事件规则</h3><p>可选的作者裁决。结构规则不确认事实；说明不是可执行约束，例外仍需解释。</p></div>
      <button type="button" disabled={!api || reading || busy} onClick={() => void reload()}>重新读取规则</button></header>
    {!api ? <p role="alert">本地 Runtime 尚未连接。</p> : !workspace && !readError ? <p role="status">正在读取规则…</p> : null}
    {readError && <p role="alert">读取失败：{readError}。{workspace ? "保留上次结果，不能据此继续采用。" : "尚不能判断是否有规则。"}</p>}
    {requestedResult && <p><button type="button" disabled={!api || busy || agentRunning || !!edit} onClick={() => void prepareRequestedReview()}>核对对话中的规则检查</button></p>}
    {artifact?.locked && <p>规则已锁定，不能修改或采用。</p>}
    {accepted ? <details open data-discussion-revision={accepted.id}><summary>已采用 · v{accepted.version}</summary><PropertyRuleSummary rules={accepted.payload.definitions} /><EventRuleSummary rules={accepted.payload.eventRules} /></details>
      : workspace && <p>尚无已采用规则。模型的属性声明不等于作者规则。</p>}
    {candidate && candidate.id !== accepted?.id && <details open><summary>待审阅候选 · v{candidate.version}</summary>
      <PropertyRuleSummary rules={candidate.payload.definitions} />
      <EventRuleSummary rules={candidate.payload.eventRules} />
      {candidate.source === "agent" && <p>模型生成的规则提案：尚未生效，采用不等于确认任何属性值。</p>}
      {pending ? <><p>审阅绑定：{pending.artifactRevisionId}，不包含事实确认。</p>
        {pending.checks.map((check) => <p key={check.id}>{check.passed ? "通过" : "未通过"}：{check.message}</p>)}
        <button type="button" disabled={blocked || pending.projectRevision !== workspace?.revision || pending.checks.some((check) => check.blocking && !check.passed)} onClick={() => void mutate(() => api!.runtime.acceptReview(projectId, pending.id, workspace!.revision, {}), "已采用规则，原主张及其确认状态没有被改写。", true)}>采用规则</button>
        {pending.projectRevision !== workspace?.revision && <p>作品版本已变化，请重新建立规则审阅。</p>}
      </> : null}
      {(!pending || pending.projectRevision !== workspace?.revision) && <button type="button" disabled={blocked} onClick={() => void mutate(() => api!.runtime.createReview(projectId, "propertyRules", candidate.id, [], {}), "已建立规则审阅，尚未采用。", false)}>建立规则审阅</button>}
    </details>}
    {undo && <button type="button" disabled={blocked} onClick={() => void mutate(() => api!.runtime.undoCommit(projectId, undo.id, workspace!.revision), "已撤销最近一次规则采用。", true)}>撤销最近规则采用</button>}
    {!edit ? <button type="button" disabled={blocked} onClick={() => {
      if (owner.writing || controls.current.agentRunning || owner.read || owner.readError || !owner.workspace || owner.workspace.artifacts.propertyRules.locked) return;
      const current = owner.workspace.artifacts.propertyRules;
      const payload = (current.candidate ?? current.accepted)?.payload;
      changeEdit({ rules: payload ? structuredClone(payload.definitions) : [], eventRules: payload ? structuredClone(payload.eventRules) : [], revision: owner.workspace.revision,
        candidateId: current.candidate?.id ?? null, request: null, savedId: null });
    }}>编辑规则候选</button> : <div className="property-rule-editor">
      <p>保存只建立候选；清空正式规则需明确移除结构与事件规则后审阅采用。属性修改不会自动删除事件条件。</p>
      {changed && <p role="alert">作品或候选版本已变化。未提交的新编辑不能覆盖当前版本；放弃后重新打开。</p>}
      {edit.request && <p>请求已冻结。{edit.savedId ? "保存回执已收到，下一步只读核对。" : "结果尚未确认，重试保持相同载荷与幂等键。"}</p>}
      <PropertyRuleFields rules={edit.rules} disabled={busy || !!edit.request || changed || !!artifact?.locked} onChange={(rules) => {
        const current = editRef.current; if (!current || owner.writing || current.request) return;
        changeEdit({ ...current, rules });
      }} />
      <EventRuleSummary rules={edit.eventRules} disabled={busy || !!edit.request || changed || !!artifact?.locked} onRemove={(index) => {
        const current = editRef.current; if (!current || owner.writing || current.request) return;
        changeEdit({ ...current, eventRules: current.eventRules.filter((_, position) => position !== index) });
      }} />
      <div className="property-rule-actions"><button type="button" disabled={busy || (!edit.request && (changed || !!readError || reading || !!artifact?.locked))} onClick={() => void save()}>{edit.savedId ? "核对保存结果" : edit.request ? "重试原保存请求" : "保存规则候选"}</button>
        <button type="button" disabled={busy} onClick={discard}>放弃本地规则输入</button></div>
    </div>}
    {error && <p role="alert">{error}</p>}{notice && <p role="status">{notice}</p>}
  </section>;
}

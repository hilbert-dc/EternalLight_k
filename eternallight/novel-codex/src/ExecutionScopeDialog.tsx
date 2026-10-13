import { useEffect, useRef, useState } from "react";
import type { ExecutionScopeConfirmation, ExecutionScopePreview, ExecutionScopeRequest, NovelCodexApi, ProposedAction } from "./types";
import { DesiredOutcomes } from "./DesiredOutcomes";
import { compactTaskName, taskOperationName } from "./task-operation-presentation";
import "./execution-scope.css";

export function ExecutionScopeDialog({ projectId, initialAction, actions, api, blocked, onClose, onStart, onComplete, onError }: {
  projectId: string; initialAction: ProposedAction; actions: ProposedAction[]; api: NovelCodexApi["agent"]; blocked: boolean;
  onClose: () => void; onStart: () => void; onComplete: (result: ExecutionScopeConfirmation) => Promise<void>; onError: (message: string) => void;
}) {
  const choices = actions.filter(action => action.sourceTaskId === initialAction.sourceTaskId && action.intentRevision === initialAction.intentRevision);
  const [selected, setSelected] = useState([initialAction.id]);
  const [maxActions, setMaxActions] = useState(1);
  const [maxTaskAttempts, setMaxTaskAttempts] = useState(8);
  const [preview, setPreview] = useState<ExecutionScopePreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState("");
  const [receipt, setReceipt] = useState("");
  const dialog = useRef<HTMLElement>(null);
  const alive = useRef(true), inFlight = useRef(false), epoch = useRef(0);
  const source = initialAction.sourceTaskId;
  const request: ExecutionScopeRequest = { actionIds: selected, maxActions, maxTaskAttempts, expectedIntentRevision: initialAction.intentRevision };
  const selectionKey = JSON.stringify(choices.map(action => [action.id, action.inputRevision, action.intentRevision, action.target, action.instruction]));
  useEffect(() => {
    alive.current = true;
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.focus();
    return () => { alive.current = false; epoch.current += 1; previous?.focus(); };
  }, []);
  useEffect(() => { epoch.current += 1; setPreview(null); }, [selectionKey, blocked]);
  function change(edit: () => void) { epoch.current += 1; setPreview(null); setError(""); edit(); }
  async function readPreview() {
    if (inFlight.current || blocked || !selected.length || saved) return;
    const ticket = ++epoch.current;
    inFlight.current = true; setBusy(true); setError(""); setPreview(null);
    try {
      const result = await api.previewExecutionScope(projectId, source, request);
      if (!alive.current || epoch.current !== ticket) return;
      if (result.projectId !== projectId || result.sourceTaskId !== source || !/^[a-f0-9]{64}$/.test(result.previewDigest)
        || result.request.actionIds.join(",") !== selected.join(",") || result.request.maxActions !== maxActions
        || result.request.maxTaskAttempts !== maxTaskAttempts || result.request.expectedIntentRevision !== request.expectedIntentRevision) throw new Error("预览与当前作品或选择不符，请重新读取");
      setPreview(result);
    } catch (reason) { if (alive.current && epoch.current === ticket) setError(reason instanceof Error ? reason.message : "范围预览失败"); }
    finally { inFlight.current = false; if (alive.current) setBusy(false); }
  }
  async function confirm() {
    if (inFlight.current || blocked || !preview || saved) return;
    inFlight.current = true; setBusy(true); setConfirming(true); setError("");
    const frozen = preview;
    onStart();
    try {
      const result = await api.confirmExecutionScope(projectId, source, frozen.request, frozen.previewDigest);
      if (result.permit.id !== `scope-${frozen.previewDigest}` || result.permit.scope.projectId !== projectId
        || result.permit.scope.sourceTaskId !== source) throw new Error("确认回执归属不符，请核对工作任务");
      if (alive.current) { setSaved(true); setReceipt("范围已保存；生成内容仍是候选，不会自动采用。"); }
      await onComplete(result);
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : "确认或执行失败";
      if (alive.current) { setPreview(null); setError(message); } onError(message);
      // A lost host response may follow a committed domain transaction. Never blindly re-authorize.
      try {
        const { scope } = await api.readExecutionScope(projectId, source);
        if (scope?.id === `scope-${frozen.previewDigest}` && scope.scope.projectId === projectId && scope.scope.sourceTaskId === source) {
          if (alive.current) { setSaved(true); setReceipt("已核对：范围已保存，执行未完成或结果待核对。请在创作任务中继续，不要重复确认。"); }
          await onComplete({ permit: scope, work: null });
        }
      } catch { if (alive.current) { setReceipt("尚不能核对保存结果，请先查看创作任务；本窗口不会自动重试。"); setSaved(true); } }
    } finally { inFlight.current = false; if (alive.current) { setBusy(false); setConfirming(false); } }
  }
  return <div className="modal-backdrop" role="presentation"><section ref={dialog} tabIndex={-1} className="project-dialog execution-scope-dialog" role="dialog" aria-modal="true" aria-labelledby="scope-title" aria-describedby="scope-description" onKeyDown={event => {
    if (event.key === "Escape") onClose();
    if (event.key === "Tab") {
      const controls = Array.from(dialog.current?.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled)") ?? []);
      const first = controls[0], last = controls.at(-1);
      if (!first) { event.preventDefault(); dialog.current?.focus(); }
      else if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog.current)) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    }
  }}><h2 id="scope-title">确认本轮创作范围</h2><p id="scope-description">仅覆盖本次对话的精确操作与目标。保存后可按实际需要继续检查；后续精确匹配的目标可在名额内执行，范围外修改仍需要你确认。分析也消耗原预算和累计尝试，不会正式采用正文、解锁场景或增加模型额度。关闭窗口不会停止已提交任务；执行中可返回对话查看进度、暂停或停止。</p>
    <fieldset disabled={busy || saved || blocked}><legend>允许的候选操作与目标</legend>{choices.map(action => {
      const name = compactTaskName(taskOperationName(action.operation, action.target));
      return <label className="scope-choice" key={action.id}><input type="checkbox" checked={selected.includes(action.id)} onChange={event => change(() => {
      const next = event.target.checked ? [...selected, action.id] : selected.filter(id => id !== action.id);
      setSelected(next); setMaxActions(current => Math.max(current, next.length));
    })} /><span>{action.instruction}<small title={name.fullLabel} aria-label={name.fullLabel}>{name.label}</small>{action.operation === "generateChapter" && <DesiredOutcomes outcomes={action.target.desiredOutcomes} />}<details><summary>精确目标</summary><pre>{JSON.stringify(action.target, null, 2)}</pre></details></span></label>;
    })}
      <div className="scope-limits"><label>动作名额（含已选择操作）<input aria-label="动作名额" type="number" min={Math.max(1, selected.length)} max={3} value={maxActions} onChange={event => change(() => setMaxActions(Number(event.target.value)))} /></label><label>累计尝试上限（含原分析、失败和建模）<input aria-label="累计尝试上限" type="number" min={2} max={16} value={maxTaskAttempts} onChange={event => change(() => setMaxTaskAttempts(Number(event.target.value)))} /></label></div>
    </fieldset>
    {preview && <div className="scope-preview" aria-label="冻结范围预览"><strong>作者原话</strong><p>{preview.authorIntent}</p><dl><dt>执行用量策略</dt><dd>{preview.remainingRequests === null ? "不设请求上限" : `${preview.remainingRequests} 次请求`} · {preview.remainingTokens === null ? "不设 token 上限" : `${preview.remainingTokens.toLocaleString()} tokens`}</dd><dt>截止时间</dt><dd>{preview.deadlineAt === null ? "执行至完成或主动停止" : new Date(preview.deadlineAt * 1000).toLocaleString()}</dd><dt>作品 / 意图 / 工作版本</dt><dd>{preview.inputRevision} / {preview.request.expectedIntentRevision} / {preview.workVersion}</dd></dl>{preview.targets.map((item, index) => item.operation === "generateChapter" && <DesiredOutcomes key={index} outcomes={item.target.desiredOutcomes} />)}<details><summary>冻结的精确操作与目标</summary><pre>{JSON.stringify(preview.targets, null, 2)}</pre></details></div>}
    {error && <p className="dialog-error" role="alert">{error}</p>}{receipt && <p role="status">{receipt}</p>}
    <div className="project-dialog-actions"><button className="secondary-button" type="button" onClick={onClose}>{confirming ? "查看执行过程" : "关闭"}</button><button className="secondary-button" type="button" disabled={busy || blocked || saved || !selected.length} onClick={() => void readPreview()}>预览范围</button><button className="primary-button" type="button" aria-label="确认范围并继续" disabled={busy || blocked || saved || !preview} onClick={() => void confirm()}>{busy ? "处理中…" : "确认范围并继续"}</button></div>
  </section></div>;
}

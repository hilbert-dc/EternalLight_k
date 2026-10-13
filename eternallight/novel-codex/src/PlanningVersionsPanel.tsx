import { useEffect, useMemo, useRef, useState } from "react";
import { PlanningRevisionPreview } from "./PlanningRevisionPreview";
import type { CreativeArtifactKind, CreativeWorkspace, NovelCodexApi, OutlineNode } from "./types";
import "./planning-versions.css";

interface Props {
  api?: NovelCodexApi;
  projectId: string;
  kind: CreativeArtifactKind;
  blocked: boolean;
  onFormalChange: () => Promise<void>;
  onOpenEditor?: () => void;
  outlineNodeId?: string | null;
}
const names = { brief: "创作方向", outline: "全书大纲", plan: "近章规划" };
type Operation = "select" | "clear" | "lock";

export function PlanningVersionsPanel({ api, projectId, kind, blocked, onFormalChange, onOpenEditor, outlineNodeId = null }: Props) {
  const owner = useMemo(() => ({ api, projectId, kind, outlineNodeId }), [api, projectId, kind, outlineNodeId]);
  const current = useRef(owner);
  current.current = owner;
  const blockedRef = useRef(blocked);
  blockedRef.current = blocked;
  const serial = useRef(0);
  const flight = useRef<{ owner: typeof owner } | null>(null);
  const [busyOwner, setBusyOwner] = useState<typeof owner | null>(null);
  const [state, setState] = useState<{ owner: typeof owner; workspace: CreativeWorkspace | null; ready: boolean; error: string | null; notice: string | null }>(
    { owner, workspace: null, ready: false, error: null, notice: null },
  );
  const visible = state.owner === owner ? state : null;
  const workspace = visible?.workspace;
  const artifact = workspace?.artifacts[kind];
  const scopedOutline = kind === "outline" && Boolean(outlineNodeId);
  const savedNodes = (artifact?.candidate ?? artifact?.accepted)?.payload?.nodes;
  const scopeMatches = Array.isArray(savedNodes) ? (savedNodes as OutlineNode[]).filter(node => (node.id ?? node.clientKey) === outlineNodeId) : [];
  const scopeAvailable = !scopedOutline || scopeMatches.length === 1 && scopeMatches[0].kind === "volume";
  const busy = busyOwner === owner;
  const disabled = blocked || busy || !visible?.ready || !api || !artifact || !scopeAvailable;

  async function read() {
    if (!api) throw new Error("作品接口尚未就绪。");
    const result = await api.runtime.getCreativeWorkspace(projectId);
    if (result.project.id !== projectId || !result.artifacts[kind]) throw new Error("读取结果不属于当前作品与规划类型。");
    return result;
  }
  async function reload() {
    if (current.current !== owner || flight.current?.owner === owner) return;
    const ticket = ++serial.current;
    setState(previous => ({ owner, workspace: previous.owner === owner ? previous.workspace : null, ready: false, error: null, notice: null }));
    try {
      const result = await read();
      if (current.current === owner && serial.current === ticket) setState({ owner, workspace: result, ready: true, error: null, notice: null });
    } catch (error) {
      if (current.current === owner && serial.current === ticket) setState(previous => ({ ...previous, ready: false, error: error instanceof Error ? error.message : "读取失败，请重新读取并核对。" }));
    }
  }
  useEffect(() => {
    current.current = owner;
    void reload();
    return () => { if (current.current === owner) { current.current = { ...owner }; ++serial.current; } };
  }, [owner]);

  async function perform(operation: Operation) {
    if (current.current !== owner || blockedRef.current || disabled || flight.current?.owner === owner || !api || !workspace || !artifact) return;
    const token = { owner };
    flight.current = token;
    setBusyOwner(owner);
    ++serial.current;
    setState(previous => ({ ...previous, error: null, notice: null }));
    let submitted = false;
    let verified = false;
    try {
      let verify: (result: CreativeWorkspace) => boolean;
      const selection = artifact.workingSelection?.selectionRevision ?? 0;
      if (operation === "lock") {
        if (!artifact.accepted) throw new Error("尚无已采用内容可锁定。");
        const locked = !artifact.locked;
        const receipt = await api.runtime.setArtifactLock(projectId, kind, locked, workspace.revision);
        submitted = true;
        if (receipt.artifactKind !== kind || receipt.locked !== locked || !Number.isSafeInteger(receipt.resultRevision) || receipt.resultRevision < workspace.revision) throw new Error("锁定回执与请求不一致。");
        verify = result => result.revision >= receipt.resultRevision && result.artifacts[kind].locked === locked;
      } else {
        const revisionId = operation === "select" ? artifact.candidate?.id : null;
        if (operation === "select" && !revisionId) throw new Error("尚无已保存稿可指定。");
        const receipt = revisionId
          ? await api.runtime.setWorkingBaseline(projectId, kind, kind, revisionId, selection)
          : await api.runtime.clearWorkingBaseline(projectId, kind, kind, selection);
        submitted = true;
        if (receipt.objectKind !== kind || receipt.objectId !== kind || receipt.revisionId !== revisionId || receipt.selectionRevision !== selection + 1) throw new Error("后续创作依据回执与请求不一致。");
        verify = result => result.artifacts[kind].workingSelection?.selectionRevision === receipt.selectionRevision
          && result.artifacts[kind].workingSelection?.revisionId === revisionId
          && (result.artifacts[kind].working?.id ?? null) === revisionId;
      }
      if (current.current !== owner) return;
      const result = await read();
      if (current.current !== owner) return;
      if (!verify(result)) throw new Error("当前来源与操作回执不一致。");
      verified = true;
      setState({ owner, workspace: result, ready: true, error: null, notice: "已保存并核对。" });
      await onFormalChange();
    } catch (error) {
      if (current.current !== owner) return;
      const reason = error instanceof Error ? error.message : "操作失败。";
      setState(previous => ({ ...previous, ready: verified, error: verified ? null : `${reason} ${submitted ? "操作已提交，但结果尚未确认。" : "保存结果尚未确认。"} 请重新读取并核对后再操作。`, notice: verified ? `操作已保存并核对，但作品视图更新失败：${reason}` : null }));
    } finally {
      if (flight.current === token) flight.current = null;
      if (current.current === owner) setBusyOwner(null);
    }
  }

  return <section className="planning-versions-panel" aria-label={`${scopedOutline ? "本卷卷纲" : names[kind]}修改记录`} aria-busy={busy}>
    {onOpenEditor && <button className="secondary-button" onClick={onOpenEditor}>返回阅读与编辑</button>}
    <div className="planning-records-toolbar">
      <p className="planning-versions-current" role="status">{artifact ? artifact.candidate || artifact.accepted ? `当前内容已保存 · ${(artifact.candidate ?? artifact.accepted)?.id === artifact.accepted?.id ? "已采用" : "尚未采用"}` : "尚无已保存内容" : "读取已保存内容"}</p>
      <button type="button" className="planning-records-refresh" aria-label="重新读取并核对" title="重新读取并核对" disabled={busy || !api} onClick={() => void reload()}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><path d="M20 7v5h-5M20 12a8 8 0 1 0-2 5M20 7l-3-3" /></svg>
      </button>
    </div>
    {scopedOutline && <p>这里只查看本卷内容；写作设置中的后续依据与锁定操作作用于整份大纲。</p>}
    {artifact && !scopeAvailable && <p role="alert">当前保存内容中无法唯一定位本卷，请返回全书大纲核对；写作设置暂不可修改。</p>}
    {visible?.error && <p role="alert">{visible.error}</p>}
    {visible?.notice && <p role="status">{visible.notice}</p>}
    {!artifact && !visible?.error && <p role="status">正在读取规划来源…</p>}
    {artifact && <>
      {artifact.locked && <p role="status">已采用内容已锁定；如需解除锁定，请展开写作设置。</p>}
      {artifact.working && artifact.working.id !== (artifact.candidate ?? artifact.accepted)?.id && <p role="status">后续创作使用另一份作者指定的内容，不是当前已保存稿；这不会正式采用该内容。</p>}
      {(artifact.candidate ?? artifact.accepted) && <section className="planning-record-content" aria-label="当前已保存内容" data-discussion-revision={(artifact.candidate ?? artifact.accepted)?.id}>
        <PlanningRevisionPreview kind={kind} payload={(artifact.candidate ?? artifact.accepted)!.payload} outlineNodeId={outlineNodeId} />
      </section>}
      {artifact.accepted && artifact.candidate && artifact.accepted.id !== artifact.candidate.id && <details className="planning-versions-comparison">
        <summary>当前已保存内容与已采用内容</summary><p>分别展示两份完整内容，不是增量修改或逐行差异，也不包含编辑器中未保存的修改。</p>
        <section data-discussion-revision={artifact.candidate.id}><h3>当前已保存内容</h3><PlanningRevisionPreview kind={kind} payload={artifact.candidate.payload} outlineNodeId={outlineNodeId} /></section>
        <section data-discussion-revision={artifact.accepted.id}><h3>已采用内容</h3><PlanningRevisionPreview kind={kind} payload={artifact.accepted.payload} outlineNodeId={outlineNodeId} /></section>
      </details>}
      {artifact.working && artifact.working.id !== artifact.candidate?.id && artifact.working.id !== artifact.accepted?.id && <details data-discussion-revision={artifact.working.id}><summary>查看后续创作参考内容</summary><PlanningRevisionPreview kind={kind} payload={artifact.working.payload} outlineNodeId={outlineNodeId} /></details>}
      <details className="planning-writing-settings" key={`${projectId}:${kind}:${outlineNodeId ?? ""}`}><summary>写作设置</summary>
      <dl className="planning-versions-sources">{([
        ["已采用内容", artifact.accepted, artifact.accepted ? "已采用" : "尚未采用"],
        ["已保存稿", artifact.candidate, artifact.candidate ? artifact.candidate.id === artifact.accepted?.id ? "与已采用内容相同" : "已有已保存稿" : "尚无已保存稿"],
        ["后续创作依据", artifact.working, artifact.working ? "使用作者指定的已保存版本" : artifact.accepted ? "使用已采用内容" : "尚无已采用内容或指定依据"],
      ] as const).map(([label, revision, description]) => <div key={label}><dt>{label}</dt><dd>{description}{revision && <small>v{revision.version} · {revision.id}</small>}</dd></div>)}</dl>
      {artifact.working && !artifact.accepted && <p>取消指定依据后，尚无已采用内容或指定规划可作为后续创作依据。</p>}
      <div className="planning-versions-actions">
        <button className="secondary-button" disabled={disabled || !artifact.candidate || artifact.candidate.id === artifact.working?.id} onClick={() => void perform("select")}>将已保存稿用于后续创作</button>
        <button className="secondary-button" disabled={disabled || !artifact.workingSelection?.revisionId} onClick={() => void perform("clear")}>取消指定依据</button>
        <button className="secondary-button" disabled={disabled || !artifact.accepted} onClick={() => void perform("lock")}>{artifact.locked ? "解除锁定" : "锁定已采用内容"}</button>
      </div>
      </details>
    </>}
  </section>;
}

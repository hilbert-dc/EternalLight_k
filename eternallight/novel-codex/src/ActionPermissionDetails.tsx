import { useEffect, useMemo, useState } from "react";
import type { ActionPermission, NovelCodexApi } from "./types";
import "./action-permission.css";

interface Props {
  api: NovelCodexApi | undefined;
  projectId: string;
  workTaskId: string;
  sourceTaskId: string;
  actionId: string;
  version: number;
}

export function ActionPermissionDetails({ api, projectId, workTaskId, sourceTaskId, actionId, version }: Props) {
  const scope = useMemo(() => ({ api, projectId, workTaskId, sourceTaskId, actionId, version }),
    [api, projectId, workTaskId, sourceTaskId, actionId, version]);
  const [open, setOpen] = useState(false);
  const [retry, setRetry] = useState(0);
  const [state, setState] = useState<{ scope: typeof scope; permission: ActionPermission | null; error: string | null } | null>(null);
  useEffect(() => {
    if (!open) return;
    let active = true;
    setState(null);
    void (async () => {
      try {
        if (!api) throw new Error("当前环境没有许可读取接口");
        const { permission } = await api.agent.readActionPermission(projectId, sourceTaskId, actionId);
        if (permission && ((!['exactAction', 'finiteScope'].includes(permission.kind)
          || (permission.kind === "finiteScope" && !permission.scopePermitId)) || permission.actionId !== actionId
          || permission.workTaskId !== workTaskId || permission.evidence.projectId !== projectId
          || permission.evidence.sourceTaskId !== sourceTaskId)) throw new Error("许可归属不匹配，请重新读取");
        if (active) setState({ scope, permission, error: null });
      } catch (error) {
        if (active) setState({ scope, permission: null, error: error instanceof Error ? error.message : "读取许可失败" });
      }
    })();
    return () => { active = false; };
  }, [scope, open, retry, api, projectId, sourceTaskId, actionId, workTaskId]);
  const visible = state?.scope === scope ? state : null;
  return <details className="action-permission-details" onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary>查看作者许可依据</summary>
    {open && <div>
      <p>这里只展示当时的授权记录，不代表现在可以执行，也不会恢复或重新批准任务。</p>
      {!visible ? <p role="status">读取许可中…</p> : visible.error ? <div role="alert">
        <p>{visible.error}</p><button type="button" className="secondary-button" onClick={() => setRetry((value) => value + 1)}>重新读取许可</button>
      </div> : !visible.permission ? <p>此动作没有作者许可记录。</p> : <>
        <dl><dt>授权类型</dt><dd>{visible.permission.kind === "finiteScope" ? "来自已确认的有限操作范围，不包含正式采用" : "仅限这一项候选动作，不包含正式采用"}</dd>
          <dt>授权时间</dt><dd>{new Date(visible.permission.grantedAt * 1000).toLocaleString()}</dd>
          <dt>意图版本</dt><dd>{visible.permission.evidence.intentRevision}</dd>
          <dt>作品版本</dt><dd>{visible.permission.evidence.inputRevision}</dd>
          <dt>作者原话</dt><dd style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{visible.permission.evidence.authorIntent}</dd>
          <dt>已批准的动作要求</dt><dd>{visible.permission.evidence.instruction}</dd></dl>
        <details><summary>精确操作与目标</summary><pre style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>
          {JSON.stringify({ operation: visible.permission.evidence.operation, target: visible.permission.evidence.target }, null, 2)}
        </pre></details>
      </>}
    </div>}
  </details>;
}

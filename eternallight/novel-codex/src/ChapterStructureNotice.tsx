import { useEffect, useMemo, useRef, useState } from "react";
import type { ChapterStructureChange, NovelCodexApi } from "./types";
import "./chapter-structure.css";

export type StructureCheckFailure = { projectId: string; changeId: string | null; message: string };

// Displays durable unclaimed checks. Execution belongs to the host consumer;
// mounting, polling and retrying a read never create a model task.
export function ChapterStructureNotice({ api, projectId, refreshToken, available, busy, dirty, failures }: {
  api: NovelCodexApi; projectId: string; refreshToken: number; available: boolean; busy: boolean; dirty: boolean;
  failures: StructureCheckFailure[];
}) {
  const scope = useMemo(() => ({ api, projectId }), [api, projectId]);
  const owner = useRef(scope); owner.current = scope;
  const [readToken, setReadToken] = useState(0);
  const [state, setState] = useState<{ scope: typeof scope; items: ChapterStructureChange[]; error: string | null }>({ scope, items: [], error: null });
  const [retryScope, setRetryScope] = useState<typeof scope | null>(null);
  useEffect(() => {
    let active = true;
    void api.runtime.listChapterStructureChanges(projectId, { cursor: null, limit: 1, assessmentState: "pending" }).then(page => {
      if (!active) return;
      if (page.projectId !== projectId || page.items.some(item => item.projectId !== projectId)) throw new Error("检查记录不属于当前作品");
      setState({ scope, items: page.items.filter(item => item.assessment.state === "pending"), error: null });
    }).catch(reason => { if (active) setState({ scope, items: [], error: reason instanceof Error ? reason.message : "暂时无法读取衔接检查" }); });
    return () => { active = false; };
  }, [api, projectId, scope, refreshToken, readToken]);
  const current = state.scope === scope ? state.items[0] : null;
  const error = state.scope === scope ? state.error : null;
  const retrying = retryScope === scope;
  const held = failures.find(item => item.projectId === projectId && item.changeId === current?.id);
  const scanFailure = failures.find(item => item.projectId === projectId && item.changeId === null);
  if (error || (!current && scanFailure)) return <div className="chapter-structure-notice" role="status">衔接检查状态暂时无法读取。<button type="button" onClick={() => setReadToken(value => value + 1)}>重新读取</button></div>;
  if (!current) return null;
  const reason = held ? "检查尚未启动，可以重试。" : !available ? "模型可用后自动开始。" : dirty ? "保存当前编辑后自动开始。" : "创作引擎空闲后自动开始。";
  return <div className="chapter-structure-notice" role="status">
    <span>第 {current.chapterNumber} 章已删除，故事衔接待检查。{reason}</span>
    {held && <><small>{held.message}</small><button type="button" disabled={retrying || busy || dirty || !available} onClick={async () => {
      setRetryScope(scope);
      try { await api.agent.retryChapterStructureCheck(projectId, current.id); }
      catch (reason) { if (owner.current === scope) setState({ scope, items: [], error: reason instanceof Error ? reason.message : "检查未能启动" }); }
      finally { if (owner.current === scope) { setRetryScope(null); setReadToken(value => value + 1); } }
    }}>{retrying ? "正在重试…" : "重试检查"}</button></>}
  </div>;
}

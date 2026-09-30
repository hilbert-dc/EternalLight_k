import { useEffect, useRef, useState } from "react";
import type { CommitmentPage, NovelCodexApi } from "./types";

const statusLabels = { open: "开放", partiallyFulfilled: "部分兑现", fulfilled: "已兑现", abandoned: "已放弃" };

export function CommitmentCatalog({ api, projectId, blocked, refreshToken, onOpen }: {
  api: NovelCodexApi; projectId: string; blocked: boolean; refreshToken: number; onOpen: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [page, setPage] = useState<CommitmentPage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [trimmed, setTrimmed] = useState(0);
  const ticket = useRef(0);
  const running = useRef(false);
  const latestPage = useRef(page);
  latestPage.current = page;
  const current = useRef(projectId);
  current.current = projectId;
  async function load(cursor: string | null = null) {
    if (blocked || running.current) return;
    const sequence = ++ticket.current;
    running.current = true; setBusy(true); setError(null);
    if (cursor === null) { setPage(null); setTrimmed(0); }
    try {
      const next = await api.runtime.listCommitments(projectId, cursor, 20);
      if (ticket.current !== sequence || current.current !== projectId) return;
      const before = latestPage.current;
      if (cursor !== null && (!before || before.snapshotId !== next.snapshotId)) throw new Error("承诺目录快照不一致，请刷新");
      const rows = cursor === null ? next.data : [...before!.data, ...next.data];
      const removed = Math.max(0, rows.length - 200);
      if (removed) setTrimmed((count) => count + removed);
      setPage({ ...next, data: rows.slice(-200) });
    } catch (reason) { if (ticket.current === sequence) setError(String(reason)); }
    finally { if (ticket.current === sequence) { running.current = false; setBusy(false); } }
  }
  useEffect(() => {
    ticket.current += 1; running.current = false; setBusy(false); setPage(null); setError(null); setTrimmed(0);
    if (open && !blocked) void load();
    return () => { ticket.current += 1; running.current = false; };
  }, [api, projectId, refreshToken, open, blocked]);
  return <details className="commitment-catalog" onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary>全作品独立承诺</summary>
    {open && <div aria-busy={busy}>
      <p>包含其他章节和旧提取报告建立的承诺。目录固定一个快照；打开时重新核对当前版本。</p>
      <button className="secondary-button" disabled={blocked || busy} onClick={() => void load()}>刷新承诺目录</button>
      {blocked && <p role="status">请先结束当前编辑或执行，再读取目录。</p>}
      {busy && <p role="status">正在读取承诺目录…</p>}
      {error && <p role="alert">{error}</p>}
      {page && <><small className="commitment-source">目录快照：{page.snapshotId}</small>
        {page.data.length === 0 && <p>此快照尚无独立承诺；提案批次不计入独立对象。</p>}
        <ul>{page.data.map((item) => <li key={item.id}>
          <button className="text-button" disabled={blocked || busy} onClick={() => onOpen(item.id)}>{item.summary}</button>
          <small>{item.confirmationState === "confirmed" ? "作者已确认" : "未确认候选"} · {statusLabels[item.status]}{!item.sourceCurrent && " · 正文证据已变化"}</small>
        </li>)}</ul>
        {trimmed > 0 && <p>为控制界面内存，已收起早期 {trimmed} 项加载结果；对象未删除。刷新目录可从首批重新查看。</p>}
        {page.nextCursor && <><button className="secondary-button" disabled={blocked || busy} onClick={() => void load(page.nextCursor)}>加载更多承诺</button><small>后续批次尚未加载，不代表全作品完整。</small></>}
      </>}
    </div>}
  </details>;
}

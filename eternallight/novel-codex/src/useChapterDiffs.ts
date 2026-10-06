import { useEffect, useState } from "react";
import type { ChapterDiff, NovelCodexApi } from "./types";

type DiffRead = { key: string; retry: number; diffs: ChapterDiff[]; error: string | null };
export function useChapterDiffs(api: NovelCodexApi | undefined, projectId: string, chapterId: string | undefined, open: boolean) {
  const [read, setRead] = useState<DiffRead | null>(null);
  const [retry, setRetry] = useState(0);
  const key = JSON.stringify([projectId, chapterId]);
  useEffect(() => {
    if (!open || !projectId || !chapterId) { setRead(null); return; }
    let cancelled = false;
    setRead(null);
    if (!api) { setRead({ key, retry, diffs: [], error: "桌面连接未就绪。" }); return; }
    void api.runtime.getDiffs(projectId, chapterId).then(result => {
      if (!cancelled) setRead({ key, retry, diffs: result.diffs, error: null });
    }).catch(cause => {
      if (!cancelled) setRead({ key, retry, diffs: [], error: cause instanceof Error ? cause.message : "修改对照读取失败" });
    });
    return () => { cancelled = true; };
  }, [api, projectId, chapterId, open, key, retry]);
  const current = open && read?.key === key && read.retry === retry ? read : null;
  return { diffs: current?.diffs ?? [], error: current?.error ?? null,
    loading: open && Boolean(projectId && chapterId) && !current,
    reload: () => setRetry(value => value + 1) };
}

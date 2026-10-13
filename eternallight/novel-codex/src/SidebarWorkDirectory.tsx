import { useEffect, useMemo, useRef, useState } from "react";
import { WorkDirectory } from "./WorkDirectory";
import { displayChapters, savedChapterTitle, type ChapterTitlePreview } from "./chapter-title";
import type { useWorkCatalog } from "./useWorkCatalog";
import type { Chapter, NovelCodexApi, OutlineNode, WorkVolumeSource } from "./types";

export type SidebarChapterCreationRequest = {
  projectId: string; volumeId: string | null; volumeTitle: string | null; requestId: string;
};
type OutlineSource = { source: WorkVolumeSource; title: string };
type Props = {
  api?: NovelCodexApi; projectId: string; chapters: Chapter[]; selectedChapterId: string | null;
  refreshToken: number; enabled: boolean; busy: boolean; dirty: boolean;
  onSelectChapter: (chapter: Chapter) => void;
  onCreateChapter: (request: SidebarChapterCreationRequest) => void;
  onDeleteChapter?: (chapterId: string) => void;
  chapterDeletionDisabled?: boolean;
  directory: ReturnType<typeof useWorkCatalog>;
  titlePreview?: ChapterTitlePreview | null;
};

export function SidebarWorkDirectory({ api, projectId, chapters, selectedChapterId, refreshToken, enabled, busy, dirty, onSelectChapter, onCreateChapter, onDeleteChapter, chapterDeletionDisabled = false, directory, titlePreview = null }: Props) {
  const owner = useMemo(() => ({}), [api, projectId, enabled]);
  const current = useRef(owner); current.current = owner;
  const [retry, setRetry] = useState(0);
  const [sources, setSources] = useState<{ owner: object; entries: OutlineSource[]; titles: { id: string; title: string }[]; error: string | null; loading: boolean }>({ owner, entries: [], titles: [], error: null, loading: true });
  useEffect(() => {
    if (!api || !enabled) return;
    let cancelled = false;
    setSources(previous => ({ owner, entries: previous.owner === owner ? previous.entries : [], titles: previous.owner === owner ? previous.titles : [], error: null, loading: true }));
    void api.runtime.getCreativeWorkspace(projectId).then(workspace => {
      if (cancelled || current.current !== owner) return;
      if (workspace.project.id !== projectId) throw new Error("目录内容不属于当前作品。");
      const saved = workspace.artifacts.outline?.candidate ?? workspace.artifacts.outline?.accepted;
      const nodes = (saved?.payload?.nodes ?? []) as OutlineNode[];
      const entries = saved ? nodes.filter(node => node.kind === "volume" && (node.id ?? node.clientKey)).map(node => ({
        source: { outlineRevisionId: saved.id, nodeId: (node.id ?? node.clientKey)! }, title: node.title,
      })) : [];
      const titles = workspace.chapters.map(chapter => ({ id: chapter.id, title: savedChapterTitle(chapter) }));
      setSources({ owner, entries, titles, error: null, loading: false });
    }).catch(reason => {
      if (!cancelled && current.current === owner) setSources(previous => ({ owner, entries: previous.owner === owner ? previous.entries : [], titles: previous.owner === owner ? previous.titles : [], loading: false, error: reason instanceof Error ? reason.message : "目录内容读取失败。" }));
    });
    return () => { cancelled = true; };
  }, [api, projectId, enabled, owner, refreshToken, retry]);
  if (!enabled) return null;
  const outline = sources.owner === owner ? sources : null;
  const blocked = busy || directory.busy;
  return <section className="sidebar-work-directory" aria-label="作品卷章目录">
    {directory.error && <p role="alert">{directory.error}<button type="button" disabled={blocked} onClick={() => void directory.refresh()}>重新读取目录</button></p>}
    {outline?.error && <p role="alert">目录内容读取失败：{outline.error}<button type="button" disabled={blocked} onClick={() => setRetry(value => value + 1)}>重新读取目录内容</button></p>}
    {(!outline || outline.loading) && <small role="status">正在读取目录内容…</small>}
    <WorkDirectory key={projectId} catalog={directory.catalog} readError={directory.error} chapters={displayChapters(chapters, outline?.titles ?? [], titlePreview?.projectId === projectId ? titlePreview : null)}
      selectedChapterId={selectedChapterId} disabled={blocked || dirty} navigationDisabled={false}
      chapterCreationDisabled={blocked} outlineSources={outline?.entries ?? []}
      onSelectChapter={chapter => { const original = chapters.find(item => item.id === chapter.id); if (original) onSelectChapter(original); }}
      onDeleteChapter={onDeleteChapter}
      chapterDeletionDisabled={chapterDeletionDisabled}
      onCreateVolume={(title, source) => directory.update({ type: "createVolume", title, ...(source ? { source } : {}) })}
      onPlaceChapter={(chapterId, volumeId) => directory.update({ type: "placeChapter", chapterId, volumeId })}
      onCreateChapter={volumeId => {
        if (blocked || !directory.catalog || directory.error || current.current !== owner) return;
        const volume = volumeId ? directory.catalog.volumes.find(item => item.id === volumeId) : null;
        if (volumeId && !volume) return;
        onCreateChapter({ projectId, volumeId, volumeTitle: volume?.title ?? null, requestId: crypto.randomUUID() });
      }} />
  </section>;
}

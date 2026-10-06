import { useCallback, useEffect, useId, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { DirectoryMenu, type DirectoryMenuAnchor } from "./DirectoryMenu";
import type { Chapter, WorkCatalog, WorkVolumeSource } from "./types";
import "./work-directory.css";

type Props = {
  catalog: WorkCatalog | null;
  readError?: string | null;
  chapters: Chapter[];
  selectedChapterId: string | null;
  disabled: boolean;
  navigationDisabled?: boolean;
  chapterCreationDisabled?: boolean;
  outlineSources?: { source: WorkVolumeSource; title: string }[];
  onSelectChapter: (chapter: Chapter) => void;
  onCreateVolume: (title: string, source?: WorkVolumeSource) => Promise<void>;
  onCreateChapter: (volumeId: string | null) => void;
  onPlaceChapter: (chapterId: string, volumeId: string | null) => Promise<void>;
  onDeleteChapter?: (chapterId: string) => void;
  chapterDeletionDisabled?: boolean;
};

export function WorkDirectory({ catalog, readError, chapters, selectedChapterId, disabled, navigationDisabled = false, chapterCreationDisabled = disabled, outlineSources = [], onSelectChapter, onCreateVolume, onCreateChapter, onPlaceChapter, onDeleteChapter, chapterDeletionDisabled = false }: Props) {
  const menuId = useId();
  const [open, setOpen] = useState(true);
  const [creating, setCreating] = useState(false);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const revealedChapter = useRef<string | null>(null);
  const [organizing, setOrganizing] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const launcher = useRef<HTMLElement | null>(null);
  const [menu, setMenu] = useState<{ anchor: DirectoryMenuAnchor; volumeId: string | null; chapterId?: string } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const openOrganizer = (create: boolean) => { launcher.current = menu?.anchor.trigger ?? document.activeElement as HTMLElement; setCreating(create); setOrganizing(true); };
  useEffect(() => {
    if (!organizing) return;
    dialog.current?.showModal();
    dialog.current?.querySelector<HTMLInputElement>(creating ? "input" : "select")?.focus({ preventScroll: true });
    return () => { dialog.current?.close(); if (launcher.current?.isConnected) launcher.current.focus({ preventScroll: true }); };
  }, [organizing]);
  const menuEvents = (volumeId: string | null, chapterId?: string) => ({
    onContextMenu: (event: React.MouseEvent<HTMLElement>) => { event.preventDefault(); event.stopPropagation(); setMenu({ volumeId, chapterId, anchor: { trigger: event.currentTarget, x: event.clientX, y: event.clientY } }); },
    onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => { if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) { event.preventDefault(); event.stopPropagation(); const rect = event.currentTarget.getBoundingClientRect(); setMenu({ volumeId, chapterId, anchor: { trigger: event.currentTarget, x: rect.left, y: rect.bottom } }); } },
  });
  const [title, setTitle] = useState("");
  const [selectedSource, setSelectedSource] = useState<WorkVolumeSource | null>(null);
  const [placementChapter, setPlacementChapter] = useState("");
  const [placementVolume, setPlacementVolume] = useState("");
  const [pending, setPending] = useState(false);
  useEffect(() => { if (pending || (menu?.volumeId && !catalog?.volumes.some(item => item.id === menu.volumeId)) || (menu?.chapterId && !chapters.some(item => item.id === menu.chapterId))) setMenu(null); }, [catalog, chapters, menu, pending]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const flight = useRef<object | null>(null);
  const mounted = useRef(false);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; flight.current = null; }; }, []);
  const volumes = [...(catalog?.volumes ?? [])].sort((left, right) => left.position - right.position);
  const blocked = disabled || pending || !catalog || Boolean(readError);
  const chapterBlocked = chapterCreationDisabled || pending || !catalog || Boolean(readError);
  const placementOf = (chapterId: string) => {
    const matches = catalog?.placements.filter(item => item.chapterId === chapterId) ?? [];
    return matches.length === 1 && volumes.some(item => item.id === matches[0].volumeId) ? matches[0].volumeId : null;
  };
  const sortedChapters = [...(catalog ? chapters : [])].sort((left, right) => left.number - right.number);
  const chapterNumberDigits = sortedChapters.reduce((digits, chapter) => Math.max(digits, String(chapter.number).length), 1);
  const currentVolume = selectedChapterId ? placementOf(selectedChapterId) : null;
  useEffect(() => {
    if (!catalog) return;
    const key = `${selectedChapterId ?? ""}:${currentVolume ?? ""}`;
    if (revealedChapter.current === key) return;
    revealedChapter.current = key;
    if (currentVolume) setExpanded(previous => ({ ...previous, [currentVolume]: true }));
  }, [selectedChapterId, currentVolume, catalog]);
  const chooseChapter = (chapter: Chapter) => onSelectChapter(chapter);
  const chapterRow = (chapter: Chapter) => <li key={chapter.id} className="work-directory-chapter-row">
    <button type="button" data-directory-chapter-id={chapter.id} {...menuEvents(placementOf(chapter.id), chapter.id)} aria-current={chapter.id === selectedChapterId ? "page" : undefined} disabled={navigationDisabled || pending} onClick={() => chooseChapter(chapter)}><span>第 {chapter.number} 章</span><strong title={chapter.title}>{chapter.title}</strong></button>
    {onDeleteChapter && <button type="button" className="work-directory-chapter-menu" aria-label={`第 ${chapter.number} 章操作`} title="章节操作" aria-haspopup="menu" aria-expanded={menu?.chapterId === chapter.id} aria-controls={menu?.chapterId === chapter.id ? menuId : undefined} disabled={navigationDisabled || pending} onClick={event => { const rect = event.currentTarget.getBoundingClientRect(); setMenu({ chapterId: chapter.id, volumeId: placementOf(chapter.id), anchor: { trigger: event.currentTarget, x: rect.left, y: rect.bottom } }); }}>⋯</button>}
  </li>;
  const mutate = async (action: () => Promise<void>, success: string) => {
    if (blocked || flight.current) return false;
    const ticket = {}; flight.current = ticket; setPending(true); setError(null); setNotice(null);
    try { await action(); if (!mounted.current || flight.current !== ticket) return false; setNotice(success); return true; }
    catch (reason) { if (mounted.current && flight.current === ticket) setError(reason instanceof Error ? reason.message : "目录修改失败，请核对后重试。"); return false; }
    finally { if (mounted.current && flight.current === ticket) { flight.current = null; setPending(false); } }
  };
  return <div className="work-directory-shell"><details className="work-directory" open={open} onToggle={event => setOpen(event.currentTarget.open)}>
    <summary className="nav-item" {...menuEvents(null)}><span className="nav-icon directory-chevron" aria-hidden="true">›</span><span className="directory-label">作品目录</span>{catalog && <span className="nav-count">{volumes.length > 0 ? `${volumes.length} 卷 · ` : ""}{chapters.length} 章</span>}</summary>
    <div className="work-directory-content" aria-busy={pending}>
      {!catalog && <p role="status">{readError ? "目录读取失败，暂不能核对卷章；请重新读取目录。" : "正在读取作品目录…"}</p>}
      {error && <p role="alert">{error}</p>}{notice && <p role="status">{notice}</p>}
      {catalog && <nav aria-label="作品章节目录" className="work-directory-tree" style={{ "--directory-number-width": `calc(3em + ${chapterNumberDigits}ch)` } as CSSProperties}>
        {volumes.map(item => {
          const rows = sortedChapters.filter(chapter => placementOf(chapter.id) === item.id);
          return <div key={item.id} className="work-directory-volume-row"><details data-volume-id={item.id} className="work-directory-volume" open={expanded[item.id] ?? false} onToggle={event => { const value = event.currentTarget.open; setExpanded(previous => previous[item.id] === value ? previous : { ...previous, [item.id]: value }); }}>
            <summary {...menuEvents(item.id)}><strong title={item.title}><span>{item.title}</span></strong><span>{rows.length} 章</span></summary>
            <ol>{rows.map(chapterRow)}</ol>
            {!rows.length && <p className="work-directory-empty-volume">本卷还没有章节。</p>}
          </details><button type="button" className="work-directory-add" aria-label={`在${item.title}新增章节`} title="在本卷新增章节" disabled={chapterBlocked} onClick={() => onCreateChapter(item.id)}>+</button></div>;
        })}
        {sortedChapters.some(chapter => placementOf(chapter.id) === null) && <section className="work-directory-other">
          <ol>{sortedChapters.filter(chapter => placementOf(chapter.id) === null).map(chapterRow)}</ol>
        </section>}
        {!sortedChapters.length && !volumes.length && <p>还没有章节，右键或点击＋创建卷和章节。</p>}
      </nav>}
    </div>
  </details>
  <button type="button" className={`work-directory-add work-directory-root-add${!chapters.length && !volumes.length ? " is-empty" : ""}`} aria-haspopup="menu" aria-expanded={Boolean(menu && !menu.chapterId && menu.volumeId === null)} aria-controls={menu && !menu.chapterId && menu.volumeId === null ? menuId : undefined} aria-label="作品目录操作" title="新增卷或章节" onClick={event => { const rect = event.currentTarget.getBoundingClientRect(); setMenu({ volumeId: null, anchor: { trigger: event.currentTarget, x: rect.left, y: rect.bottom } }); }}>+</button>
  {menu && <DirectoryMenu id={menuId} anchor={menu.anchor} onClose={closeMenu} items={menu.chapterId ? [
    { label: "删除章节", danger: true, disabled: chapterDeletionDisabled || navigationDisabled || pending || !catalog || Boolean(readError) || !onDeleteChapter, run: () => { if (menu.chapterId && chapters.some(chapter => chapter.id === menu.chapterId)) onDeleteChapter?.(menu.chapterId); } },
  ] : menu.volumeId ? [
    { label: "在本卷新增章节", disabled: chapterBlocked, run: () => { if (!chapterBlocked && catalog?.volumes.some(item => item.id === menu.volumeId)) onCreateChapter(menu.volumeId); } },
  ] : [
    { label: "新增章节", disabled: chapterBlocked, run: () => onCreateChapter(null) },
    { label: "新增卷", disabled: blocked, run: () => openOrganizer(true) },
    { label: "整理目录", run: () => openOrganizer(false) },
  ]} />}
  {createPortal(<dialog ref={dialog} className="work-directory-dialog" aria-labelledby="directory-organize-title" onCancel={event => { event.preventDefault(); if (!pending) setOrganizing(false); }}>
    <header><h2 id="directory-organize-title">整理目录</h2><button type="button" disabled={pending} aria-label="关闭整理目录" onClick={() => setOrganizing(false)}>×</button></header>
    {error && <p role="alert">{error}</p>}{notice && <p role="status">{notice}</p>}
    <div className="work-directory-organize">
      <p>需要分卷时，在这里新增卷或调整章节归属。</p>
      <div className="work-directory-actions"><button type="button" disabled={blocked} aria-expanded={creating} onClick={() => setCreating(value => !value)}>新增卷</button></div>
      {creating && <form className="work-directory-create" onSubmit={event => { event.preventDefault(); if (!title.trim()) return; const source = selectedSource ? { ...selectedSource } : undefined; void mutate(() => onCreateVolume(title.trim(), source), "卷已保存到作品目录。").then(saved => { if (saved && mounted.current) { setCreating(false); setTitle(""); setSelectedSource(null); dialog.current?.querySelector<HTMLButtonElement>(".work-directory-actions button")?.focus({ preventScroll: true }); } }); }}>
        {outlineSources.length > 0 && <label><span>卷纲来源</span><select value={selectedSource ? JSON.stringify(selectedSource) : ""} disabled={blocked} onChange={event => { const selected = outlineSources.find(item => JSON.stringify(item.source) === event.target.value); setSelectedSource(selected ? { ...selected.source } : null); if (selected) { setTitle(selected.title); } }}><option value="">新建独立卷</option>{outlineSources.map(item => <option key={`${item.source.outlineRevisionId}:${item.source.nodeId}`} value={JSON.stringify(item.source)}>{item.title}</option>)}</select></label>}
        <label><span>卷名称</span><input value={title} readOnly={Boolean(selectedSource)} disabled={blocked} maxLength={160} onChange={event => setTitle(event.target.value)} /></label>
        {selectedSource && <p>关联后直接使用同一份卷纲；这里只建立卷章目录，不复制卷纲内容。</p>}
        <button type="submit" disabled={blocked || !title.trim()}>{pending ? "正在保存…" : "保存新卷"}</button>
      </form>}
      {catalog && chapters.length > 0 && <details className="work-directory-placement"><summary>调整章节所属卷</summary><form onSubmit={event => { event.preventDefault(); if (!chapters.some(chapter => chapter.id === placementChapter)) return; void mutate(() => onPlaceChapter(placementChapter, placementVolume || null), "章节归属已保存。"); }}>
        <label><span>章节</span><select value={placementChapter} disabled={blocked} onChange={event => { setPlacementChapter(event.target.value); setPlacementVolume(placementOf(event.target.value) ?? ""); }}><option value="">选择章节</option>{[...chapters].sort((a, b) => a.number - b.number).map(chapter => <option key={chapter.id} value={chapter.id}>第 {chapter.number} 章 · {chapter.title}</option>)}</select></label>
        <label><span>所属卷</span><select value={placementVolume} disabled={blocked} onChange={event => setPlacementVolume(event.target.value)}><option value="">未分卷</option>{volumes.map(item => <option key={item.id} value={item.id}>{item.title}</option>)}</select></label>
        <button type="submit" disabled={blocked || !placementChapter}>保存章节归属</button>
      </form></details>}
      </div>
  </dialog>, document.body)}
  </div>;
}

import { useLayoutEffect, useRef } from "react";
import { PlanningVersionsPanel } from "./PlanningVersionsPanel";
import { DiffView } from "./StoryViews";
import { useChapterDiffs } from "./useChapterDiffs";
import type { Chapter, NovelCodexApi } from "./types";
import "./module-records.css";

export type ModuleRecordSource = {
  projectId: string;
  view: "brief" | "outline" | "plans" | "chapters";
  chapterId: string | null;
  outlineNodeId: string | null;
  volumeId: string | null;
  label: string;
};

export function ModuleRecordsPopover({ source, api, chapter, dirty, blocked, opener, container, onClose, onFormalChange }: {
  source: ModuleRecordSource; api?: NovelCodexApi; chapter?: Chapter; dirty: boolean; blocked: boolean;
  opener: HTMLElement | null; container: HTMLElement | null; onClose: () => void; onFormalChange: () => Promise<void>;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const explicitClose = useRef(false);
  const chapterDiffs = useChapterDiffs(api, source.projectId, source.chapterId ?? undefined, source.view === "chapters");
  const close = () => { explicitClose.current = true; onCloseRef.current(); };
  useLayoutEffect(() => {
    const element = panel.current!;
    let frame = 0;
    const place = () => {
      const rect = container?.getBoundingClientRect();
      const narrow = innerWidth <= 640 || !rect || rect.width < 320;
      const width = Math.min(440, narrow ? innerWidth - 24 : rect.width - 24);
      const toolbar = container?.querySelector(".content-header, .chapter-page-toolbar")?.getBoundingClientRect();
      const top = Math.max(12, Math.min(innerHeight - 140, Math.max(rect?.top ?? 12, toolbar?.bottom ?? 12) + 8));
      const left = narrow ? innerWidth - width - 12 : Math.max(12, Math.min(innerWidth - width - 12, rect.right - width - 12));
      Object.assign(element.style, { left: `${left}px`, top: `${top}px`, width: `${width}px`,
        maxHeight: `${Math.max(120, innerHeight - top - 12)}px`, visibility: "visible" });
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(() => { frame = 0; place(); }); };
    const toggle = (event: Event) => {
      if ((event as ToggleEvent).newState === "closed") onCloseRef.current();
    };
    place();
    element.addEventListener("toggle", toggle);
    element.showPopover();
    closeButton.current?.focus({ preventScroll: true });
    const observer = new ResizeObserver(schedule);
    if (container) observer.observe(container);
    window.addEventListener("resize", schedule);
    window.addEventListener("scroll", schedule, true);
    return () => {
      observer.disconnect(); cancelAnimationFrame(frame);
      window.removeEventListener("resize", schedule);
      window.removeEventListener("scroll", schedule, true);
      element.removeEventListener("toggle", toggle);
      const outside = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      if (element.matches(":popover-open")) element.hidePopover();
      if (explicitClose.current && opener?.isConnected) opener.focus({ preventScroll: true });
      else if (outside?.isConnected && !element.contains(outside)) outside.focus({ preventScroll: true });
      else if (document.activeElement === opener) opener?.blur();
    };
  }, []);
  return <div ref={panel} id="module-modification-records" popover="auto" role="dialog"
    className="module-records-popover" aria-label={`${source.label} · 修改记录`}
    data-discussion-document="" data-discussion-project={source.projectId} data-discussion-view={source.view}
    data-discussion-label={`${source.label} · 修改记录`} data-discussion-dirty="false"
    onKeyDown={event => { if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); } }}>
    <header><div><h2>修改记录</h2><p>{source.label}</p></div>
      <button ref={closeButton} type="button" aria-label="关闭修改记录" onClick={close}>×</button></header>
    <div className="module-records-body">
      {dirty && <p className="module-records-notice" role="status">编辑区有未保存修改，这里只展示已保存内容。{source.view !== "chapters" && "保存后才能调整写作设置。"}</p>}
      {source.view === "chapters" ? chapter?.id === source.chapterId
        ? <DiffView {...chapterDiffs} chapter={chapter} />
        : <p role="alert">当前章节已变化，请关闭后从章节重新打开修改记录。</p>
        : source.view === "outline" && source.volumeId && !source.outlineNodeId
          ? <p role="alert">本卷尚未关联卷纲，暂时没有对应的修改记录。请返回大纲关联本卷后再查看。</p>
          : <PlanningVersionsPanel api={api} projectId={source.projectId} kind={source.view === "plans" ? "plan" : source.view}
            outlineNodeId={source.outlineNodeId} blocked={blocked || dirty} onFormalChange={onFormalChange} />}
    </div>
  </div>;
}

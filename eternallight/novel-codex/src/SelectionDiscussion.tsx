import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { DiscussionExcerpt, ViewId } from "./types";
import "./selection-discussion.css";

const labels: Record<ViewId, string> = { brief: "创作简报", outline: "大纲", plans: "章纲", chapters: "章节",
  characters: "人物资料", world: "世界与规则", plot: "伏笔与悬念", timeline: "时间线" };
const excluded = 'button, select, [data-discussion-exclude], .conversation-rail, .conversation-panel, .conversation-task-records, .planning-reading-actions, .content-header, .chapter-page-toolbar, .story-reference-controls, .account-panel';
type Capture = { excerpt: DiscussionExcerpt; element: HTMLElement; rect: DOMRect; current: () => boolean };

function closeSourceOverlay(element: HTMLElement) {
  const dialog = element.closest("dialog");
  if (dialog?.open) dialog.close();
  const popover = element.closest<HTMLElement>("[popover]");
  if (popover?.matches(":popover-open")) popover.hidePopover();
}

function captureSelection(projectId: string, view: ViewId): Capture | null {
  const active = document.activeElement;
  let element: HTMLElement, text: string, start: number, end: number, rect: DOMRect, current: () => boolean;
  if (active instanceof HTMLTextAreaElement || active instanceof HTMLInputElement) {
    if (active instanceof HTMLInputElement && !["text", "search", "url", "email", "tel"].includes(active.type)) return null;
    if (active instanceof HTMLInputElement && active.type === "search") return null;
    start = active.selectionStart ?? 0; end = active.selectionEnd ?? 0;
    text = active.value.slice(start, end); element = active; rect = active.getBoundingClientRect();
    const value = active.value;
    current = () => active.isConnected && active.value === value;
  } else {
    const selection = document.getSelection();
    if (!selection || selection.isCollapsed || selection.rangeCount !== 1) return null;
    const range = selection.getRangeAt(0).cloneRange();
    element = range.commonAncestorContainer instanceof HTMLElement ? range.commonAncestorContainer : range.commonAncestorContainer.parentElement!;
    if (!element) return null;
    const source = element.closest<HTMLElement>("[data-discussion-document]");
    if (!source || !source.contains(range.startContainer) || !source.contains(range.endContainer)) return null;
    // A cross-section selection must never capture unrelated controls or chat text.
    if (Array.from(source.querySelectorAll(excluded)).some(node => range.intersectsNode(node))) return null;
    text = range.toString();
    const prefix = document.createRange(); prefix.selectNodeContents(source); prefix.setEnd(range.startContainer, range.startOffset);
    start = prefix.toString().length; end = start + text.length;
    rect = range.getBoundingClientRect();
    current = () => element.isConnected && range.toString() === text;
  }
  const documentRoot = element.closest<HTMLElement>("[data-discussion-document]");
  if (!documentRoot || element.closest(excluded) || !text.trim()) return null;
  const rootProject = documentRoot.dataset.discussionProject;
  if (rootProject !== projectId) return null;
  const sourceView = (documentRoot.dataset.discussionView ?? view) as ViewId;
  if (!Object.hasOwn(labels, sourceView)) return null;
  const identity = element.closest<HTMLElement>('[data-discussion-label], [data-outline-node-id], [data-outline-reading-node], [data-scene-id], [data-discussion-locator]');
  const fieldLabel = element.getAttribute("aria-label") ?? element.closest("label")?.querySelector("span, strong")?.textContent ?? "";
  const locator = [identity?.dataset.discussionLocator ?? identity?.dataset.outlineNodeId ?? identity?.dataset.outlineReadingNode ?? identity?.dataset.sceneId ?? "",
    fieldLabel].filter(Boolean).join(" · ").slice(0, 240);
  const metadata = element.closest<HTMLElement>("[data-discussion-revision]") ?? documentRoot;
  const unsaved = (element instanceof HTMLTextAreaElement || element instanceof HTMLInputElement)
    && documentRoot.dataset.discussionDirty === "true";
  const label = [identity?.dataset.discussionLabel ?? documentRoot.dataset.discussionLabel ?? labels[sourceView],
    fieldLabel].filter(Boolean).join(" · ").replace(/[\p{Cc}]/gu, " ").slice(0, 240);
  return { element, rect, current, excerpt: { id: `excerpt-${crypto.randomUUID()}`, projectId, view: sourceView,
    label, text, locator: locator.replace(/[\p{Cc}]/gu, " "), state: unsaved ? "unsaved" : "displayed",
    snapshotId: element.closest<HTMLElement>("[data-discussion-snapshot]")?.dataset.discussionSnapshot || null,
    revisionId: metadata.dataset.discussionRevision || null,
    objectId: element.closest<HTMLElement>("[data-discussion-object]")?.dataset.discussionObject || null, start, end } };
}

/** One capture boundary for every document, including native form controls and rendered text. */
export function SelectionDiscussion({ projectId, view, disabled, onAdd, onNew }: {
  projectId: string; view: ViewId; disabled: boolean; onAdd: (excerpt: DiscussionExcerpt) => boolean;
  onNew?: (excerpt: DiscussionExcerpt) => Promise<boolean>;
}) {
  const [capture, setCapture] = useState<Capture | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const addRef = useRef(onAdd); addRef.current = onAdd;
  const newRef = useRef(onNew); newRef.current = onNew;
  const [pending, setPending] = useState(false);
  const pendingRef = useRef(false);
  const scopeRef = useRef({ projectId, view }); scopeRef.current = { projectId, view };
  // A failed creation owns its captured selection beyond the parent's transient
  // busy state. Promise settlement and the disabled=false effect can occur in
  // either order; neither is a user dismissal of that selection.
  const creationSelection = useRef<{ projectId: string; view: ViewId; capture: Capture } | null>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const captureRef = useRef(capture); captureRef.current = capture;
  useEffect(() => {
    const retained = creationSelection.current;
    if (retained?.projectId === projectId && retained.view === view) {
      setCapture(retained.capture);
    } else {
      creationSelection.current = null;
      setCapture(null); setNotice(null);
    }
    if (!projectId || disabled) return;
    let frame = 0;
    const refresh = (event: Event) => {
      if (pendingRef.current) return;
      // Keep the entry reachable after Tab or pointer focus moves from the document.
      if (document.activeElement?.closest(".selection-discussion-entry")) return;
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (pendingRef.current) return;
        creationSelection.current = null;
        const selected = captureSelection(projectId, view);
        if (selected && event instanceof PointerEvent && selected.element === event.target
          && (selected.element instanceof HTMLTextAreaElement || selected.element instanceof HTMLInputElement)) {
          selected.rect = new DOMRect(event.clientX, event.clientY, 0, 0);
        }
        setCapture(selected); setNotice(null);
      });
    };
    const add = (candidate: Capture | null) => {
      if (pendingRef.current) return;
      if (!candidate?.current()) { creationSelection.current = null; setNotice("内容已变化，请重新选择。"); setCapture(null); return; }
      if (addRef.current(candidate.excerpt)) { creationSelection.current = null; closeSourceOverlay(candidate.element); setCapture(null); setNotice(null); }
    };
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !pendingRef.current) { creationSelection.current = null; setCapture(null); setNotice(null); }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "l") {
        const selected = captureSelection(projectId, view);
        if (selected) { event.preventDefault(); add(selected); }
      }
    };
    const hide = () => { if (!pendingRef.current) { creationSelection.current = null; setCapture(null); setNotice(null); } };
    document.addEventListener("selectionchange", refresh);
    document.addEventListener("select", refresh, true);
    document.addEventListener("pointerup", refresh);
    document.addEventListener("keyup", refresh);
    document.addEventListener("input", hide, true);
    document.addEventListener("keydown", keyboard);
    window.addEventListener("scroll", hide, true); window.addEventListener("resize", hide);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener("selectionchange", refresh); document.removeEventListener("select", refresh, true);
      document.removeEventListener("pointerup", refresh); document.removeEventListener("keyup", refresh);
      document.removeEventListener("input", hide, true); document.removeEventListener("keydown", keyboard);
      window.removeEventListener("scroll", hide, true); window.removeEventListener("resize", hide);
    };
  }, [projectId, view, disabled]);
  async function createDiscussion() {
    const selected = captureRef.current;
    const create = newRef.current;
    if (!create || pendingRef.current) return;
    if (!selected?.current()) { creationSelection.current = null; setCapture(null); setNotice("内容已变化，请重新选择。"); return; }
    const scope = { projectId, view };
    creationSelection.current = { ...scope, capture: selected };
    pendingRef.current = true; setPending(true); setNotice(null);
    const excerpt = Object.freeze({ ...selected.excerpt });
    const isCurrent = () => mounted.current && scopeRef.current.projectId === scope.projectId && scopeRef.current.view === scope.view;
    try {
      if (await create(excerpt)) {
        if (isCurrent()) { creationSelection.current = null; closeSourceOverlay(selected.element); setCapture(null); setNotice(null); }
      } else if (isCurrent()) { setCapture(selected); setNotice("未能创建对话，选区已保留，可以重试。"); }
    } catch (cause) {
      if (isCurrent()) { setCapture(selected); setNotice(cause instanceof Error ? cause.message : "创建对话失败，选区已保留，可以重试。"); }
    } finally {
      pendingRef.current = false;
      if (mounted.current) setPending(false);
    }
  }
  if (!capture || (disabled && !pending) || capture.excerpt.projectId !== projectId || !capture.element.isConnected) return notice ? <p role="status">{notice}</p> : null;
  const left = Math.max(8, Math.min(innerWidth - (onNew ? 360 : 200), capture.rect.left));
  const top = capture.rect.bottom + 8 < innerHeight - 48 ? capture.rect.bottom + 8 : Math.max(8, capture.rect.top - 44);
  return createPortal(<div className={`selection-discussion-entry${pending ? " is-pending" : ""}`} style={{ left, top }} aria-busy={pending}>
    <button type="button" disabled={pending || disabled} title="选中文字后按 ⌘/Ctrl+L" onMouseDown={event => event.preventDefault()} onClick={() => {
      if (pendingRef.current) return;
      const selected = captureRef.current;
      if (!selected?.current()) { creationSelection.current = null; setCapture(null); setNotice("内容已变化，请重新选择。"); return; }
      if (addRef.current(selected.excerpt)) { creationSelection.current = null; closeSourceOverlay(selected.element); setCapture(null); setNotice(null); }
    }}>加入对话 <kbd>⌘/Ctrl+L</kbd></button>
    {onNew && <button type="button" className="selection-discussion-new" disabled={pending || disabled} aria-label="新建对话讨论" onMouseDown={event => event.preventDefault()} onClick={() => void createDiscussion()}>{pending ? "创建中…" : "新建对话讨论"}</button>}
    {notice && <p className="selection-discussion-notice" role="status">{notice}</p>}
  </div>, capture.element.closest("[popover], dialog") ?? document.body);
}

export function DiscussionExcerptCards({ excerpts, disabled, readOnly, onRemove }: {
  excerpts: readonly DiscussionExcerpt[]; disabled: boolean; readOnly: boolean; onRemove?: (id: string) => void;
}) {
  if (!excerpts.length) return null;
  return <section className="discussion-excerpts" aria-label="引用内容">
    {excerpts.map(item => <article key={item.id} className="discussion-excerpt">
      <header><span>{item.label}</span>{onRemove && <button type="button" aria-label={`移除引用：${item.label}`} disabled={disabled} onClick={() => onRemove(item.id)}>×</button>}</header>
      <small title={item.revisionId ?? undefined}>{item.state === "unsaved" ? "未保存的编辑内容" : "选中时的显示内容"}{item.revisionId ? " · 已记录来源版本" : ""}</small>
      <details><summary><span>查看引用内容</span><em>{item.text.length} 字符</em></summary><blockquote>{item.text}</blockquote></details>
    </article>)}
    {readOnly && <p className="discussion-excerpt-notice" role="status">引用包含未保存内容；保存来源后可应用修改。</p>}
  </section>;
}

import { useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
import type { NovelCodexApi } from "./types";

type ReadingState = { open: Map<string, boolean>; scrollTop: number; follow: boolean;
  anchor: { key: string; offset: number } | null };
const projectsByApi = new WeakMap<NovelCodexApi, Map<string, ReadingState>>();

export function useConversationReading(api: NovelCodexApi | undefined, projectId: string,
  panelRef: RefObject<HTMLElement | null>, enabled: boolean) {
  const memory = useMemo(() => {
    let projects = api && projectsByApi.get(api);
    if (!projects) { projects = new Map(); if (api) projectsByApi.set(api, projects); }
    const state = projects.get(projectId) ?? { open: new Map(), scrollTop: 0, follow: true, anchor: null };
    projects.delete(projectId); projects.set(projectId, state);
    while (projects.size > 16) projects.delete(projects.keys().next().value!);
    return state;
  }, [api, projectId]);
  const [state, setState] = useState<{ owner: ReadingState; reading: boolean } | null>(null);
  const jumpRef = useRef<() => void>(() => {});
  useLayoutEffect(() => {
    const panel = panelRef.current;
    const scroller = panel?.closest(".conversation-rail-scroll");
    if (!enabled || !panel || !(scroller instanceof HTMLElement)) return;
    let frame = 0, applying = false, restoredTop: number | null = null;
    const rows = () => [...panel.querySelectorAll<HTMLElement>("[data-conversation-row]")];
    const capture = () => {
      memory.scrollTop = scroller.scrollTop;
      const top = scroller.getBoundingClientRect().top;
      const row = rows().find(node => node.getBoundingClientRect().bottom > top);
      memory.anchor = row ? { key: row.dataset.conversationRow!, offset: row.getBoundingClientRect().top - top } : null;
    };
    const renderReading = () => {
      const reading = !memory.follow && scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight > 32;
      setState(previous => previous?.owner === memory && previous.reading === reading ? previous : { owner: memory, reading });
    };
    const restore = () => {
      applying = true;
      for (const detail of panel.querySelectorAll<HTMLDetailsElement>("details[data-conversation-detail]")) {
        const key = detail.dataset.conversationDetail!;
        if (memory.open.has(key) && detail.open !== memory.open.get(key)) detail.open = memory.open.get(key)!;
      }
      if (memory.follow) { scroller.scrollTop = scroller.scrollHeight; capture(); }
      else {
        const row = memory.anchor && rows().find(node => node.dataset.conversationRow === memory.anchor!.key);
        const desired = row ? scroller.scrollTop + row.getBoundingClientRect().top
          - scroller.getBoundingClientRect().top - memory.anchor!.offset : memory.scrollTop;
        scroller.scrollTop = desired;
        // A short loading placeholder may clamp the browser to zero. Keep the
        // intended reading anchor until the durable reply can contain it.
        if (Math.abs(scroller.scrollTop - desired) < 1) capture();
      }
      restoredTop = scroller.scrollTop; renderReading(); applying = false;
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(() => { frame = 0; restore(); }); };
    const scroll = () => {
      if (applying || restoredTop === scroller.scrollTop) { restoredTop = null; return; }
      memory.follow = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight <= 32;
      capture(); renderReading();
    };
    const userIntent = () => { restoredTop = null; memory.follow = false; capture(); renderReading(); };
    const editors = "input, textarea, select, [contenteditable]:not([contenteditable='false']), [role='textbox']";
    const interactive = (target: EventTarget | null) => target instanceof Element
      && Boolean(target.closest(`${editors}, button, a`));
    const openingSummary = (target: EventTarget | null) => {
      if (!(target instanceof Element) || interactive(target)) return false;
      const summary = target.closest("summary");
      return Boolean(summary && panel.contains(summary) && summary.parentElement instanceof HTMLDetailsElement
        && !summary.parentElement.open);
    };
    const pointerIntent = (event: PointerEvent) => {
      if (!event.defaultPrevented && event.button === 0 && openingSummary(event.target)) userIntent();
    };
    const keyIntent = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      if (!event.ctrlKey && !event.metaKey && !event.altKey && ["Enter", " "].includes(event.key) && openingSummary(event.target)) {
        userIntent(); return;
      }
      if (interactive(event.target) || (event.target instanceof Element && event.target.closest("summary"))) return;
      if (!event.altKey && (["ArrowUp", "PageUp", "Home"].includes(event.key)
        || (event.key === " " && event.shiftKey && !event.ctrlKey && !event.metaKey))) userIntent();
    };
    const wheelIntent = (event: WheelEvent) => {
      if (!event.defaultPrevented && !event.ctrlKey && event.deltaY < 0) userIntent();
    };
    const selectionIntent = () => {
      const selection = document.getSelection();
      if (!document.activeElement?.closest(editors) && selection && !selection.isCollapsed
        && selection.anchorNode && selection.focusNode && panel.contains(selection.anchorNode) && panel.contains(selection.focusNode)) userIntent();
    };
    const toggle = (event: Event) => {
      const detail = event.target;
      if (!(detail instanceof HTMLDetailsElement) || !detail.dataset.conversationDetail) return;
      memory.open.set(detail.dataset.conversationDetail, detail.open);
      while (memory.open.size > 128) memory.open.delete(memory.open.keys().next().value!);
      schedule();
    };
    jumpRef.current = () => { memory.follow = true; restore(); };
    restore();
    const mutation = new MutationObserver(schedule);
    mutation.observe(panel, { childList: true, subtree: true, characterData: true });
    const resize = new ResizeObserver(schedule); resize.observe(panel);
    scroller.addEventListener("scroll", scroll); scroller.addEventListener("wheel", wheelIntent);
    scroller.addEventListener("pointerdown", pointerIntent); scroller.addEventListener("keydown", keyIntent);
    document.addEventListener("selectionchange", selectionIntent);
    panel.addEventListener("toggle", toggle, true);
    return () => {
      if (frame) cancelAnimationFrame(frame); mutation.disconnect(); resize.disconnect();
      scroller.removeEventListener("scroll", scroll); scroller.removeEventListener("wheel", wheelIntent);
      scroller.removeEventListener("pointerdown", pointerIntent); scroller.removeEventListener("keydown", keyIntent);
      document.removeEventListener("selectionchange", selectionIntent);
      panel.removeEventListener("toggle", toggle, true); jumpRef.current = () => {};
    };
  }, [enabled, memory, panelRef]);
  return { reading: enabled && state?.owner === memory && state.reading, jumpToLatest: () => jumpRef.current() };
}

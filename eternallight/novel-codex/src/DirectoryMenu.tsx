import { useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

type Item = { label: string; disabled?: boolean; danger?: boolean; run: () => void };
export type DirectoryMenuAnchor = { trigger: HTMLElement; x: number; y: number };
export function DirectoryMenu({ id, anchor, items, onClose }: { id: string; anchor: DirectoryMenuAnchor; items: Item[]; onClose: () => void }) {
  const menu = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: anchor.x, top: anchor.y });
  useLayoutEffect(() => {
    const element = menu.current!;
    const rect = element.getBoundingClientRect();
    setPosition({ left: Math.max(8, Math.min(anchor.x, innerWidth - rect.width - 8)), top: Math.max(8, Math.min(anchor.y, innerHeight - rect.height - 8)) });
    element.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus({ preventScroll: true });
    const outside = (event: PointerEvent) => { if (!element.contains(event.target as Node)) onClose(); };
    const scroll = (event: Event) => { if (!element.contains(event.target as Node)) onClose(); };
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("scroll", scroll, true);
    window.addEventListener("resize", onClose);
    return () => { document.removeEventListener("pointerdown", outside, true); document.removeEventListener("scroll", scroll, true); window.removeEventListener("resize", onClose); };
  }, [anchor, onClose]);
  return createPortal(<div id={id} ref={menu} className="directory-menu" role="menu" aria-label="目录操作" style={{ left: position.left, top: position.top }} onKeyDown={event => {
    const buttons = [...menu.current!.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Escape") { event.preventDefault(); onClose(); if (anchor.trigger.isConnected) anchor.trigger.focus({ preventScroll: true }); }
    else if (event.key === "Tab") onClose();
    else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault(); const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : (index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
      buttons[next]?.focus({ preventScroll: true });
    }
  }}>{items.map(item => <button key={item.label} type="button" role="menuitem" className={item.danger ? "directory-menu-danger" : undefined} disabled={item.disabled} onClick={() => { onClose(); if (anchor.trigger.isConnected) anchor.trigger.focus({ preventScroll: true }); item.run(); }}>{item.label}</button>)}</div>, document.body);
}

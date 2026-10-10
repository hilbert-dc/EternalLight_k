import { useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

type Item = { id?: string; label: string; ariaLabel?: string; title?: string; description?: string; checked?: boolean; disabled?: boolean; danger?: boolean; run: () => void };
export type DirectoryMenuAnchor = { trigger: HTMLElement; x: number; y: number; placement?: "above" };
export function DirectoryMenu({ id, anchor, items, onClose, label = "目录操作" }: { id: string; anchor: DirectoryMenuAnchor; items: Item[]; onClose: () => void; label?: string }) {
  const menu = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: anchor.x, top: anchor.y });
  useLayoutEffect(() => {
    const element = menu.current!;
    const rect = element.getBoundingClientRect();
    const top = anchor.placement === "above" ? anchor.y - rect.height - 8 : anchor.y;
    setPosition({ left: Math.max(8, Math.min(anchor.x, innerWidth - rect.width - 8)), top: Math.max(8, Math.min(top, innerHeight - rect.height - 8)) });
    element.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus({ preventScroll: true });
    const outside = (event: PointerEvent) => { if (!element.contains(event.target as Node)) onClose(); };
    const triggerPosition = anchor.trigger.getBoundingClientRect();
    const scroll = (event: Event) => {
      if (element.contains(event.target as Node)) return;
      const current = anchor.trigger.getBoundingClientRect();
      if (!anchor.trigger.isConnected || Math.abs(current.left - triggerPosition.left) > 0.5 || Math.abs(current.top - triggerPosition.top) > 0.5) onClose();
    };
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("scroll", scroll, true);
    window.addEventListener("resize", onClose);
    return () => { document.removeEventListener("pointerdown", outside, true); document.removeEventListener("scroll", scroll, true); window.removeEventListener("resize", onClose); };
  }, [anchor, onClose]);
  return createPortal(<div id={id} ref={menu} className="directory-menu" role="menu" aria-label={label} style={{ left: position.left, top: position.top }} onKeyDown={event => {
    const buttons = [...menu.current!.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Escape") { event.preventDefault(); onClose(); if (anchor.trigger.isConnected) anchor.trigger.focus({ preventScroll: true }); }
    else if (event.key === "Tab") { anchor.trigger.focus({ preventScroll: true }); if (event.shiftKey) event.preventDefault(); onClose(); }
    else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault(); const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : (index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
      buttons[next]?.focus({ preventScroll: true });
    }
  }}>{items.map(item => <button key={item.id ?? item.ariaLabel ?? item.label} type="button" role={item.checked === undefined ? "menuitem" : "menuitemradio"} aria-checked={item.checked} aria-label={item.ariaLabel ?? item.label} title={item.title} className={item.danger ? "directory-menu-danger" : undefined} disabled={item.disabled} onClick={() => { onClose(); if (anchor.trigger.isConnected) anchor.trigger.focus({ preventScroll: true }); item.run(); }}><span>{item.label}{item.description && <small className="conversation-menu-description">{item.description}</small>}</span>{item.checked && <span aria-hidden="true">✓</span>}</button>)}</div>, document.body);
}

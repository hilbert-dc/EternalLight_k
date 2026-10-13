import { useRef, type PointerEvent as ReactPointerEvent } from "react";

export const PANE_KEY = "novel-codex:layout-panes";

export type LayoutPanes = { sidebar: number; rail: number; inspector: number };

export function clampPane(value: number, min: number, max: number) { return Math.min(max, Math.max(min, Math.round(value))); }

export function loadPanes(): LayoutPanes {
  const fallback = { sidebar: 248, rail: 420, inspector: 245 };
  try {
    const parsed = JSON.parse(localStorage.getItem(PANE_KEY) ?? "") as Partial<LayoutPanes>;
    return {
      sidebar: clampPane(Number(parsed.sidebar) || fallback.sidebar, 72, 480),
      rail: clampPane(Number(parsed.rail) || fallback.rail, 300, 840),
      inspector: clampPane(Number(parsed.inspector) || fallback.inspector, 88, 720),
    };
  } catch {
    return fallback;
  }
}

export function PaneSash({ orientation, label, value, min, max, nextValue, onResize }: { orientation: "vertical" | "horizontal"; label: string; value: number; min: number; max: number; nextValue: (start: number, dx: number, dy: number) => number; onResize: (value: number) => void }) {
  const valueRef = useRef(value);
  const nextRef = useRef(nextValue);
  const resizeRef = useRef(onResize);
  valueRef.current = value;
  nextRef.current = nextValue;
  resizeRef.current = onResize;
  const apply = (start: number, dx: number, dy: number) => resizeRef.current(clampPane(nextRef.current(start, dx, dy), min, max));
  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const sash = event.currentTarget;
    const start = valueRef.current;
    const originX = event.clientX;
    const originY = event.clientY;
    sash.setPointerCapture(event.pointerId);
    sash.classList.add("dragging");
    const previousCursor = document.body.style.cursor;
    const previousSelect = document.body.style.userSelect;
    document.body.style.cursor = orientation === "vertical" ? "col-resize" : "row-resize";
    document.body.style.userSelect = "none";
    const move = (next: PointerEvent) => apply(start, next.clientX - originX, next.clientY - originY);
    const end = () => {
      sash.classList.remove("dragging");
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousSelect;
      sash.removeEventListener("pointermove", move);
      sash.removeEventListener("pointerup", end);
      sash.removeEventListener("pointercancel", end);
    };
    sash.addEventListener("pointermove", move);
    sash.addEventListener("pointerup", end);
    sash.addEventListener("pointercancel", end);
  };
  return <div className="pane-sash" role="separator" aria-orientation={orientation} aria-label={label} aria-valuemin={min} aria-valuemax={max} aria-valuenow={value} tabIndex={0} onPointerDown={onPointerDown} onKeyDown={(event) => { const step = event.shiftKey ? 48 : 16; const dx = event.key === "ArrowRight" ? step : event.key === "ArrowLeft" ? -step : 0; const dy = event.key === "ArrowDown" ? step : event.key === "ArrowUp" ? -step : 0; if (!dx && !dy) return; event.preventDefault(); apply(value, dx, dy); }} />;
}

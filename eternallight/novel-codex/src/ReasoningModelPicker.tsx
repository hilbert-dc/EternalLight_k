import { useEffect, useLayoutEffect, useId, useRef, useState, type CSSProperties, type ReactNode } from "react";

type Reasoning = { effort: string; defaultEffort: string; options: { effort: string; label: string; description: string }[] };
export function ReasoningModelPicker({ model, reasoning, effort, disabled, onChange, modelPicker }: {
  model: string; reasoning: Reasoning; effort: string; disabled: boolean;
  onChange?: (effort: string) => void; modelPicker: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const [preview, setPreview] = useState<number | null>(null);
  const root = useRef<HTMLDivElement>(null), trigger = useRef<HTMLButtonElement>(null), slider = useRef<HTMLInputElement>(null);
  const dragging = useRef(false), draft = useRef<number | null>(null);
  const restoreFocus = useRef(false);
  const id = useId();
  const popover = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (!open || !root.current || !popover.current) return;
    const anchor = root.current, panel = popover.current;
    const rail = anchor.closest<HTMLElement>(".conversation-rail");
    const position = () => {
      const bounds = rail?.getBoundingClientRect();
      const left = Math.max(12, (bounds?.left ?? 0) + 12);
      const right = Math.min(window.innerWidth - 12, (bounds?.right ?? window.innerWidth) - 12);
      panel.style.width = `${Math.min(256, Math.max(0, right - left))}px`;
      const x = anchor.getBoundingClientRect().left;
      panel.style.left = `${Math.max(left, Math.min(x, right - panel.offsetWidth)) - x}px`;
      panel.style.right = "auto";
    };
    position();
    const observer = new ResizeObserver(position);
    observer.observe(anchor); if (rail) observer.observe(rail);
    window.addEventListener("resize", position);
    return () => { observer.disconnect(); window.removeEventListener("resize", position); };
  }, [open]);
  const selected = Math.max(0, reasoning.options.findIndex(option => option.effort === effort));
  const index = preview ?? selected;
  const option = reasoning.options[index];
  const fraction = reasoning.options.length > 1 ? index / (reasoning.options.length - 1) : 0;
  const resetPreview = () => { dragging.current = false; draft.current = null; setPreview(null); };
  useEffect(() => {
    resetPreview();
    if (!disabled && restoreFocus.current) {
      if (open && document.activeElement === document.body) slider.current?.focus({ preventScroll: true });
      restoreFocus.current = false;
    }
  }, [model, effort, disabled]);
  useEffect(() => {
    if (!open) return;
    slider.current?.focus({ preventScroll: true });
    const outside = (event: PointerEvent) => { if (event.target instanceof Node && !root.current?.contains(event.target)) { resetPreview(); setOpen(false); } };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);
  const commit = (value: number) => {
    restoreFocus.current = document.activeElement === slider.current;
    resetPreview();
    const next = reasoning.options[value];
    if (!disabled && next && next.effort !== effort) onChange?.(next.effort);
  };
  const close = () => { resetPreview(); setOpen(false); trigger.current?.focus({ preventScroll: true }); };
  if (!option) return <>{modelPicker}</>;
  return <div ref={root} className="composer-reasoning-picker" onKeyDown={event => {
    if (event.key === "Escape" && open) { event.preventDefault(); event.stopPropagation(); close(); }
  }} onBlur={event => {
    if (event.relatedTarget && !event.currentTarget.contains(event.relatedTarget)) { resetPreview(); setOpen(false); }
  }}>
    <button ref={trigger} type="button" className="composer-option" aria-label={`模型与推理强度：${model}，${reasoning.options[selected].label}`}
      aria-haspopup="dialog" aria-expanded={open} aria-controls={open ? id : undefined} onClick={() => setOpen(value => !value)}>
      <span className="reasoning-trigger-selection" hidden={open}><span className="composer-model-label">{model}</span><span className="composer-effort-label">{reasoning.options[selected].label}</span></span>
      {open && <span className="reasoning-trigger-prompt">选择强度</span>}
      <svg className="composer-icon" viewBox="0 0 16 16" aria-hidden="true"><path d="m4.5 6 3.5 3.5L11.5 6" /></svg>
    </button>
    {open && <div ref={popover} id={id} role="dialog" aria-label="模型与推理强度" className="composer-reasoning-popover">
      <div className="reasoning-heading">
        <svg className="reasoning-lightning" viewBox="0 0 24 24" aria-hidden="true"><path d="m13 2-9 12h7l-1 8 10-13h-7z" /></svg>
        <strong>{option.label}</strong>
        <button type="button" className="reasoning-reset" aria-label="恢复默认推理强度" title="恢复默认推理强度" disabled={disabled || effort === reasoning.defaultEffort}
          onClick={() => commit(reasoning.options.findIndex(item => item.effort === reasoning.defaultEffort))}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 4v6h6M4.8 9a8 8 0 1 1-.3 6" /></svg>
        </button>
      </div>
      <div className="reasoning-model-link">{modelPicker}</div>
      <div className="reasoning-slider" style={{ "--reasoning-fill": `calc(${fraction * 100}% + ${27 - fraction * 54}px)` } as CSSProperties}>
        <div className="reasoning-track" aria-hidden="true"><div className="reasoning-ticks">{reasoning.options.map(item => <i key={item.effort} />)}</div></div>
        <input ref={slider} type="range" aria-label="推理强度" aria-valuetext={option.label} title={option.description}
          min={0} max={Math.max(0, reasoning.options.length - 1)} step={1} value={index} disabled={disabled || reasoning.options.length < 2}
          onPointerDown={event => { if (event.isTrusted) event.currentTarget.setPointerCapture(event.pointerId); dragging.current = true; draft.current = selected; }}
          onChange={event => { const value = Number(event.target.value); draft.current = value; if (dragging.current) setPreview(value); else commit(value); }}
          onPointerUp={() => { if (dragging.current) commit(draft.current ?? selected); }}
          onPointerCancel={resetPreview} />
      </div>
    </div>}
  </div>;
}

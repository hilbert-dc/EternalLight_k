import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import type { WorkTaskSummary } from "./types";
import { workStepName } from "./work-step-presentation";
import "./composer-retry-control.css";

function savedStatus(task: WorkTaskSummary) {
  const saved = task.deliveryReport.evidence.filter(item => item.kind !== "completionReport"
    && item.state === "recorded" && item.referenceId).length;
  return saved ? `${saved} 项成果已保存` : "保存结果待核对";
}

export function ComposerRetryControl({ scope, choices, disabled, singleAction, label, onSelect, onRetry, agentAvailable }: {
  scope: string; choices: WorkTaskSummary[]; disabled: boolean; singleAction: boolean; label: string;
  onSelect?: (task: WorkTaskSummary) => void; onRetry?: () => void;
  agentAvailable: boolean;
}) {
  const [opened, setOpened] = useState<{ scope: string; versions: string } | null>(null);
  const container = useRef<HTMLDivElement>(null), trigger = useRef<HTMLButtonElement>(null), menu = useRef<HTMLDivElement>(null);
  const id = useId();
  const versions = JSON.stringify(choices.map(task => [task.id, task.version, task.resumeMode]));
  const current = useRef({ scope, versions, disabled, choices, onSelect });
  current.current = { scope, versions, disabled, choices, onSelect };
  const multiple = choices.length > 1 || choices.length === 1 && !singleAction;
  const visible = !disabled && opened?.scope === scope && opened.versions === versions;
  const close = () => { setOpened(null); if (trigger.current?.isConnected && !trigger.current.disabled) trigger.current.focus({ preventScroll: true }); };
  useEffect(() => { setOpened(null); }, [scope, versions, disabled]);
  useLayoutEffect(() => { if (visible) menu.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus({ preventScroll: true }); }, [visible]);
  useEffect(() => {
    if (!visible) return;
    const outside = (event: PointerEvent) => { if (event.target instanceof Node && !container.current?.contains(event.target)) setOpened(null); };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [visible]);
  if (!choices.length && !onRetry) return null;
  return <div ref={container} className="composer-retry-control" onBlur={event => {
    if (!event.currentTarget.contains(event.relatedTarget)) setOpened(null);
  }} onKeyDown={event => {
    if (event.key === "Escape" && visible) { event.preventDefault(); event.stopPropagation(); close(); }
  }}>
    <button ref={trigger} type="button" className="composer-retry" aria-label={multiple ? "选择要继续的任务" : label}
      title={multiple ? "选择要继续的任务" : label} disabled={disabled} aria-haspopup={multiple ? "menu" : undefined}
      aria-expanded={multiple ? visible : undefined} aria-controls={visible ? id : undefined} onClick={() => {
        if (disabled) return;
        if (multiple) setOpened(visible ? null : { scope, versions }); else onRetry?.();
      }}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><path d="M20 7v5h-5M20 12a8 8 0 1 0-2.3 5.7" /></svg></button>
    {visible && <div ref={menu} id={id} className="composer-retry-menu" role="menu" aria-label="选择要继续的任务" onKeyDown={event => {
      if (event.key === "Tab") { trigger.current?.focus({ preventScroll: true }); if (event.shiftKey) event.preventDefault(); else setOpened(null); return; }
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      const items = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
      const index = items.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
        : (index + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
      items[next]?.focus({ preventScroll: true });
    }}>
      <p>选择原任务继续</p>
      {choices.map(task => {
        const step = task.steps.find(item => item.status === "blocked") ?? task.steps.find(item => item.status === "queued") ?? task.steps.at(-1);
        const action = task.resumeMode === "recoverSavedResult" ? "核对保存结果" : "继续执行";
        const unavailable = task.resumeMode !== "recoverSavedResult" && !agentAvailable;
        return <button type="button" role="menuitem" tabIndex={-1} key={task.id} disabled={unavailable} data-retry-task-id={task.id} onClick={() => {
          const latest = current.current;
          const candidate = latest.choices.find(item => item.id === task.id && item.version === task.version && item.resumeMode === task.resumeMode);
          if (unavailable || latest.disabled || latest.scope !== scope || latest.versions !== versions || !candidate) return;
          close(); latest.onSelect?.(candidate);
        }}><strong>{task.goal}</strong><span>{step ? workStepName(task, step).label : "当前创作任务"}</span>
          <small>{action} · {savedStatus(task)}{unavailable ? " · 模型暂不可用" : ""}</small></button>;
      })}
    </div>}
  </div>;
}

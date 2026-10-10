import { ReasoningModelPicker } from "./ReasoningModelPicker";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import type { NovelCodexApi } from "./types";

type AgentConnection = Awaited<ReturnType<NovelCodexApi["agent"]["listConnections"]>>["connections"][number];
import "./composer-model-picker.css";

function ModelConnectionPicker({ connections, provider, model, label, disabled, onSelect }: {
  connections: AgentConnection[]; provider?: string | null; model?: string | null; reasoningEffort?: string | null;
  label: string; disabled: boolean; reasoningDisabled?: boolean; onReasoningChange?: (effort: string) => void; onSelect: (connection: AgentConnection) => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const focusId = useRef<string | null>(null);
  const openingEdge = useRef<"last" | "current">("current");
  const id = useId();
  const visible = open && !disabled;
  useEffect(() => { if (disabled) setOpen(false); }, [disabled]);
  useLayoutEffect(() => {
    const container = menu.current;
    if (!visible || !container) return;
    const items = [...container.querySelectorAll<HTMLButtonElement>("[role='menuitemradio']")];
    if (container.contains(document.activeElement)) return;
    const retained = items.find(item => item.dataset.connectionId === focusId.current);
    const selected = items.find(item => item.getAttribute("aria-checked") === "true");
    const target = retained ?? (openingEdge.current === "last" ? items.at(-1) : selected ?? items[0]);
    (target ?? container).focus({ preventScroll: true });
  }, [visible, connections]);
  useEffect(() => {
    if (!visible) return;
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [visible]);
  const closeToTrigger = () => {
    setOpen(false);
    if (trigger.current?.isConnected && !trigger.current.disabled) trigger.current.focus({ preventScroll: true });
  };
  return <div ref={root} className="composer-model-picker" onBlur={event => {
    if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
  }} onKeyDown={event => {
    if (visible && event.key === "Escape") { event.preventDefault(); event.stopPropagation(); closeToTrigger(); }
  }}>
    <button ref={trigger} type="button" className="composer-option" aria-label={`选择模型：${label}`} title={`选择模型：${label}`}
      aria-haspopup="menu" aria-expanded={visible} aria-controls={visible ? id : undefined} disabled={disabled}
      onClick={() => { focusId.current = null; openingEdge.current = "current"; setOpen(value => !value); }}
      onKeyDown={event => {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault(); focusId.current = null; openingEdge.current = event.key === "ArrowUp" ? "last" : "current";
          if (visible && menu.current) {
            const items = [...menu.current.querySelectorAll<HTMLButtonElement>("[role='menuitemradio']")];
            const target = event.key === "ArrowUp" ? items.at(-1) : items.find(item => item.getAttribute("aria-checked") === "true") ?? items[0];
            (target ?? menu.current).focus({ preventScroll: true });
          } else setOpen(true);
        }
      }}><span className="composer-model-label">{label}</span><svg className="composer-icon" viewBox="0 0 16 16" aria-hidden="true"><path d="m4.5 6 3.5 3.5L11.5 6" /></svg></button>
    {visible && <div ref={menu} id={id} className="composer-model-menu" role="menu" aria-label="选择模型连接" tabIndex={-1}
      onKeyDown={event => {
        if (event.key === "Tab") {
          // Anchor native tab order on the persistent trigger before removing the menu.
          // Otherwise Chromium may restart at the document when the focused item unmounts.
          trigger.current?.focus({ preventScroll: true });
          if (event.shiftKey) event.preventDefault(); else setOpen(false);
          return;
        }
        if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
        event.preventDefault();
        const items = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("[role='menuitemradio']")];
        if (!items.length) return;
        const current = items.indexOf(document.activeElement as HTMLButtonElement);
        const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
          : (current + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
        items[next].focus({ preventScroll: true });
      }}>
      {connections.length === 0 ? <span className="composer-model-empty">没有已配置的模型连接</span> : connections.map(connection => {
        const selected = connection.provider === provider && connection.model === model;
        const status = connection.status === "verified" ? "可用" : connection.status === "failed" ? "验证失败" : "需要验证";
        return <button type="button" role="menuitemradio" aria-checked={selected} tabIndex={-1} key={connection.id}
          data-connection-id={connection.id} className={selected ? "selected" : ""}
          onFocus={event => {
            focusId.current = connection.id;
            const container = menu.current, item = event.currentTarget;
            if (!container) return;
            const bounds = container.getBoundingClientRect(), row = item.getBoundingClientRect();
            if (row.top < bounds.top) container.scrollTop -= bounds.top - row.top;
            else if (row.bottom > bounds.bottom) container.scrollTop += row.bottom - bounds.bottom;
          }} onClick={() => {
            if (disabled) return;
            const current = connections.find(item => item.id === connection.id);
            if (!current) return;
            closeToTrigger(); onSelect(current);
          }}><span><strong>{connection.model}</strong><small>{connection.provider} · {connection.selectedProtocol ?? connection.protocol}</small>
            <small>{connection.collaborationStatus === "ready" ? "复杂任务自动协作" : connection.collaborationStatus === "failed" ? "协作验证失败 · 单智能体执行" : "协作待验证"}</small></span>
          <em className={`model-status ${connection.status}`}>{selected ? `当前 · ${status}` : status}</em></button>;
      })}
    </div>}
  </div>;
}

export function ComposerModelPicker(props: Parameters<typeof ModelConnectionPicker>[0]) {
  const connection = props.connections.find(item => item.provider === props.provider && item.model === props.model);
  if (!connection?.reasoning) return <ModelConnectionPicker {...props} />;
  return <ReasoningModelPicker model={props.label} reasoning={connection.reasoning}
    effort={props.reasoningEffort ?? connection.reasoning.effort}
    disabled={Boolean(props.reasoningDisabled || !props.onReasoningChange)}
    onChange={props.onReasoningChange}
    modelPicker={<ModelConnectionPicker {...props} />} />;
}

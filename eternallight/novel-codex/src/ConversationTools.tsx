import { useCallback, useEffect, useId, useRef, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import { DirectoryMenu, type DirectoryMenuAnchor } from "./DirectoryMenu";
import type { WorkTaskSummary } from "./types";
import { workStepName } from "./work-step-presentation";
import { compactTaskName } from "./task-operation-presentation";
export function ConversationTools({ scope, container, searchContainer, inspectorOpen, onInspector, savingCount, currentSavingCount, onRecovery, pauseChoices = [], pauseDisabled = false, onPauseTask }: {
  scope: string; container: RefObject<HTMLElement | null>; searchContainer: HTMLElement | null;
  inspectorOpen: boolean; onInspector: () => void; savingCount: number; currentSavingCount: number; onRecovery: () => void;
  pauseChoices?: WorkTaskSummary[]; pauseDisabled?: boolean; onPauseTask?: (task: WorkTaskSummary, command: "pause" | "pauseAfterStep") => void;
}) {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState(-1);
  const [matches, setMatches] = useState<HTMLElement[]>([]);
  const [targets, setTargets] = useState({ result: false, interaction: false });
  const input = useRef<HTMLInputElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const [anchor, setAnchor] = useState<DirectoryMenuAnchor | null>(null);
  const [pauseAnchor, setPauseAnchor] = useState<{ anchor: DirectoryMenuAnchor; scope: string; versions: string } | null>(null);
  const pauseVersions = JSON.stringify(pauseChoices.map(task => [task.projectId, task.threadId, task.id, task.controlRevision, task.status]));
  const current = useRef({ scope, pauseVersions, pauseChoices, pauseDisabled, onPauseTask });
  current.current = { scope, pauseVersions, pauseChoices, pauseDisabled, onPauseTask };
  const closeMenu = useCallback(() => setAnchor(null), []);
  const closePauseMenu = useCallback(() => setPauseAnchor(null), []);
  const menuId = useId();
  const pauseMenuId = useId();
  const visiblePause = !pauseDisabled && pauseAnchor?.scope === scope && pauseAnchor.versions === pauseVersions;
  useEffect(() => { setQuery(""); setOpen(false); setPosition(-1); setAnchor(null); setPauseAnchor(null); }, [scope]);
  useEffect(() => { setPauseAnchor(null); }, [pauseVersions, pauseDisabled]);
  useEffect(() => { if (open) input.current?.focus(); }, [open, searchContainer]);
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if (!event.defaultPrevented && (event.metaKey || event.ctrlKey) && !event.altKey && event.key.toLowerCase() === "f"
        && container.current?.closest(".conversation-rail")?.contains(document.activeElement)) {
        event.preventDefault(); setOpen(true); input.current?.focus();
      }
    };
    window.addEventListener("keydown", shortcut);
    return () => window.removeEventListener("keydown", shortcut);
  }, [container]);
  useEffect(() => {
    const root = container.current;
    if (!root) return;
    const update = () => {
      const nextTargets = { result: !!root.querySelector(".task-result-card"), interaction: !!root.querySelector('.user-input-card, .conversation-proposals, .transcript-error, [data-work-task-id] [role="alert"]') };
      setTargets(previous => previous.result === nextTargets.result && previous.interaction === nextTargets.interaction ? previous : nextTargets);
      const needle = query.trim().toLocaleLowerCase();
      const nodes = needle ? [...root.querySelectorAll<HTMLElement>(".conversation-markdown p, .conversation-markdown li, .conversation-markdown h1, .conversation-markdown h2, .conversation-markdown h3, .conversation-markdown td, .conversation-markdown th, .conversation-markdown pre, .conversation-request-preview, .conversation-work-heading, .task-result-card > strong")]
        .filter(node => {
          const text = node.matches(".conversation-work-heading")
            ? `${node.textContent ?? ""} ${node.getAttribute("aria-label") ?? ""}` : node.textContent ?? "";
          return text.toLocaleLowerCase().includes(needle);
        }) : [];
      const leaves = nodes.filter(node => !nodes.some(other => other !== node && node.contains(other)));
      setMatches(previous => previous.length === leaves.length && previous.every((node, index) => node === leaves[index]) ? previous : leaves);
    };
    update(); const observer = new MutationObserver(update); observer.observe(root, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ["aria-label"] });
    return () => observer.disconnect();
  }, [scope, query, container]);
  function reveal(node?: HTMLElement) {
    if (!node) return;
    node.dispatchEvent(new Event("conversation:reveal", { bubbles: true }));
    let parent = node.parentElement;
    while (parent && parent !== container.current) { if (parent instanceof HTMLDetailsElement) parent.open = true; parent = parent.parentElement; }
    node.scrollIntoView({ block: "center", behavior: "instant" });
    node.tabIndex = -1; node.focus({ preventScroll: true });
  }
  const move = (direction: number) => { if (!matches.length) return; const next = position < 0 ? (direction > 0 ? 0 : matches.length - 1) : (position + direction + matches.length) % matches.length; setPosition(next); reveal(matches[next]); };
  const closeSearch = () => { setOpen(false); trigger.current?.focus({ preventScroll: true }); };
  return <><div className="conversation-tools">
    {currentSavingCount > 0 && <button type="button" className="conversation-saving-alert" aria-label="核对本次保存" title="本次保存结果尚未确认" onClick={onRecovery}>保存待确认</button>}
    <button type="button" ref={trigger} className="conversation-tools-trigger" aria-label="对话操作" title="对话操作" aria-haspopup="menu" aria-expanded={Boolean(anchor)} aria-controls={anchor ? menuId : undefined}
      onClick={event => { const button = event.currentTarget, rect = button.getBoundingClientRect(); setAnchor(anchor ? null : { trigger: button, x: rect.right - 180, y: rect.bottom + 4 }); }}>⋯</button>
    {anchor && <DirectoryMenu id={menuId} anchor={anchor} onClose={closeMenu} label="对话操作" items={[
      { label: "查找消息", run: () => setOpen(true) },
      ...(targets.result ? [{ label: "定位成果", run: () => reveal(container.current?.querySelector<HTMLElement>(".task-result-card") ?? undefined) }] : []),
      ...(targets.interaction ? [{ label: "查看待处理事项", run: () => reveal(container.current?.querySelector<HTMLElement>('.user-input-card, .conversation-proposals, .transcript-error, [data-work-task-id] [role="alert"]') ?? undefined) }] : []),
      { label: inspectorOpen ? "收起细节" : "任务细节", run: onInspector },
      ...(pauseChoices.length && onPauseTask ? [{ label: "暂停创作任务…", disabled: pauseDisabled,
        run: () => setPauseAnchor({ anchor, scope, versions: pauseVersions }) }] : []),
      ...(savingCount ? [{ label: "保存记录", description: `${savingCount} 项保存结果尚未确认`, run: onRecovery }] : []),
    ]} />}
    {visiblePause && pauseAnchor && <DirectoryMenu id={pauseMenuId} anchor={pauseAnchor.anchor} onClose={closePauseMenu} label="选择要暂停的任务"
      items={pauseChoices.flatMap(task => {
        const step = task.steps.find(item => item.status === "running") ?? task.steps.find(item => item.status === "queued");
        const apply = (command: "pause" | "pauseAfterStep") => {
          const latest = current.current;
          const selected = latest.pauseChoices.find(item => item.id === task.id && item.projectId === task.projectId && item.threadId === task.threadId
            && item.controlRevision === task.controlRevision && item.status === "running");
          if (latest.scope === scope && latest.pauseVersions === pauseVersions && !latest.pauseDisabled && selected) latest.onPauseTask?.(selected, command);
        };
        return [{ id: task.id, label: `暂停：${compactTaskName(task.goal).label}`, ariaLabel: `暂停：${task.goal}`, title: task.goal,
          description: `${step ? workStepName(task, step).label : "当前创作任务"} · 已保存内容保留`, run: () => {
          apply("pause");
        } }, ...(!task.pauseAfterStep ? [{ id: `${task.id}:after-step`, label: `本步结束后暂停：${compactTaskName(task.goal).label}`,
          ariaLabel: `本步结束后暂停：${task.goal}`, description: "允许已开始的步骤收尾，不再启动后续步骤", run: () => apply("pauseAfterStep") }] : [])];
      })} />}
  </div>{open && searchContainer && createPortal(<div className="conversation-find"><input ref={input} type="search" aria-label="查找当前对话消息" placeholder="搜索已加载消息" value={query} onChange={event => { setQuery(event.target.value); setPosition(-1); }} onKeyDown={event => { if (event.nativeEvent.isComposing) return; if (event.key === "Enter") { event.preventDefault(); move(event.shiftKey ? -1 : 1); } if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); closeSearch(); } }} />
      <span role="status">{matches.length ? position < 0 ? `${matches.length} 处匹配` : `${Math.min(position + 1, matches.length)} / ${matches.length}` : "无匹配"}</span>
      <button type="button" disabled={!matches.length} onClick={() => move(-1)}>上一处</button><button type="button" disabled={!matches.length} onClick={() => move(1)}>下一处</button><button type="button" aria-label="关闭查找" onClick={closeSearch}>×</button></div>, searchContainer)}</>;
}

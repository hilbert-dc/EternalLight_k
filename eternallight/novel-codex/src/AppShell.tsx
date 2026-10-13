import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ProjectSummary, RuntimeState, ViewId } from "./types";

export const navGroups: { label: string; items: { id: ViewId; label: string; icon: string }[] }[] = [
  { label: "故事", items: [{ id: "brief", label: "作品概况", icon: "◫" }, { id: "chapters", label: "作品目录", icon: "▤" }, { id: "characters", label: "角色", icon: "♙" }] },
  { label: "规划", items: [{ id: "outline", label: "全书大纲", icon: "≡" }, { id: "plans", label: "近章规划", icon: "⋮" }] },
  { label: "参考", items: [{ id: "plot", label: "伏笔与悬念", icon: "⌁" }, { id: "timeline", label: "时间线", icon: "◷" }, { id: "world", label: "世界与规则", icon: "◇" }] },
];

export function Sidebar({ conversationNavigation, outlineDirectory, directory, activeView, state, onNavigate, onCreateProject, runtimeConnection, busy }: { conversationNavigation?: ReactNode; outlineDirectory?: ReactNode; directory?: ReactNode; activeView: ViewId; state: RuntimeState; onNavigate: (view: ViewId) => void; onCreateProject: () => void; runtimeConnection: ReactNode; busy: boolean }) {
  return <aside className="sidebar"><div className="sidebar-heading"><span>作品</span><button className="tiny-button" title="新建作品" onClick={onCreateProject} disabled={busy}>＋</button></div>{conversationNavigation}<nav className="nav-groups">{navGroups.map((group) => <div className="nav-group" key={group.label}><div className="nav-label">{group.label}</div>{group.items.map((item) => item.id === "outline" && outlineDirectory ? <div key={item.id}>{outlineDirectory}</div> : item.id === "chapters" && directory ? <div key={item.id} className="sidebar-chapter-navigation">{directory}</div> : <button className={`nav-item ${activeView === item.id ? "active" : ""}`} key={item.id} aria-label={item.label} aria-current={activeView === item.id ? "page" : undefined} title={item.label} onClick={() => onNavigate(item.id)}><span className="nav-icon">{item.icon}</span><span>{item.label}</span>{item.id === "chapters" && <span className="nav-count">{state.novelEdition.chapters.length}</span>}</button>)}</div>)}</nav><div className="sidebar-footer">{runtimeConnection}</div></aside>;
}

export function ProjectSwitcher({ projects, activeProjectId, busy, onSelect, onCreate, onInformation }: { projects: ProjectSummary[]; activeProjectId: string; busy: boolean; onSelect: (projectId: string) => void; onCreate: () => void; onInformation?: () => void }) {
  const [open, setOpen] = useState(false);
  const active = projects.find((project) => project.id === activeProjectId);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") setOpen(false); };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
  useEffect(() => { if (busy) setOpen(false); }, [busy]);
  return <div className="project-switcher-wrap"><button className="project-switcher" data-project-information-trigger aria-expanded={open} disabled={busy} onClick={() => setOpen((value) => !value)}><span className="project-dot" /><span title={active?.title}>{active?.title ?? "选择作品"}</span><span className="muted">⌄</span></button>{open && <div className="project-menu" role="menu" aria-label="选择作品">{onInformation && <button type="button" className="project-menu-item" role="menuitem" onClick={() => { setOpen(false); onInformation(); }}>作品信息</button>}<div className="project-menu-heading"><span>我的作品</span><button className="project-create-menu" onClick={() => { setOpen(false); onCreate(); }}>＋ 新建</button></div>{projects.map((project) => <button className={`project-menu-item ${project.id === activeProjectId ? "selected" : ""}`} role="menuitem" key={project.id} onClick={() => { setOpen(false); onSelect(project.id); }}><span className="project-menu-mark">{project.id === activeProjectId ? "✓" : "▧"}</span><span><strong>{project.title}</strong><small>{project.logline || "暂无简介"}</small></span></button>)}</div>}</div>;
}

export function UnsavedChangesDialog({ saving, onSave, onDiscard, onStay }: { saving: boolean; onSave: () => void; onDiscard: () => void; onStay: () => void }) {
  const stayRef = useRef<HTMLButtonElement | null>(null);
  useEffect(() => { stayRef.current?.focus(); }, []);
  return <div className="modal-backdrop" role="presentation"><section className="project-dialog unsaved-dialog" role="dialog" aria-modal="true" aria-labelledby="unsaved-dialog-title" aria-describedby="unsaved-dialog-description"><div className="project-dialog-header"><div><span className="eyebrow">尚未保存</span><h2 id="unsaved-dialog-title">当前创作修改还没有确认保存</h2></div></div><p id="unsaved-dialog-description" className="project-dialog-description">保存会写入当前创作编辑内容；也可以明确放弃本地输入，或留在当前页面继续处理。放弃不会撤回可能已经保存的候选。</p><div className="project-dialog-actions three-way-actions"><button className="text-button danger" type="button" disabled={saving} onClick={onDiscard}>放弃修改并离开</button><button ref={stayRef} className="secondary-button" type="button" disabled={saving} onClick={onStay}>留在当前页面</button><button className="primary-button" type="button" disabled={saving} onClick={onSave}>{saving ? "正在保存…" : "保存并继续"}</button></div></section></div>;
}

export function CommandPalette({ onClose, onFocusComposer, onReview, onReconnect, onTheme }: { onClose: () => void; onFocusComposer: () => void; onReview: () => void; onReconnect: () => void; onTheme: () => void }) {
  const commands = [{ label: "聚焦创作输入", hint: "⌘↵", action: onFocusComposer }, { label: "查看作品目录", hint: "", action: onReview }, { label: "恢复创作引擎连接", hint: "", action: onReconnect }, { label: "切换主题", hint: "", action: onTheme }];
  return <div className="command-palette-backdrop" role="presentation" onMouseDown={(event) => { if (event.currentTarget === event.target) onClose(); }}><section className="command-palette" role="dialog" aria-modal="true" aria-label="命令面板"><div className="command-palette-heading"><strong>命令面板</strong><button onClick={onClose} aria-label="关闭命令面板">Esc</button></div><div className="command-palette-list">{commands.map((command) => <button key={command.label} onClick={command.action}><span>✦</span><strong>{command.label}</strong><small>{command.hint}</small></button>)}</div></section></div>;
}

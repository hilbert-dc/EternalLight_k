import { useEffect, useRef, useState, type ReactNode } from "react";
import { newDraftScene } from "./creative-product-logic";
import type { DraftRevision } from "./types";
import "./chapter-document.css";

type Scene = DraftRevision["sceneGroups"][number];
type Bookmark = { sceneKey: string; start: number; end: number; direction: "forward" | "backward" | "none"; viewportOffset: number };
const bookmarks = new Map<string, Bookmark>();
const SESSION_LIMIT = 128;
function rememberSession<T>(cache: Map<string, T>, scope: string, value: T) {
  cache.delete(scope);
  cache.set(scope, value);
  while (cache.size > SESSION_LIMIT) cache.delete(cache.keys().next().value!);
}
const sceneKey = (scene: Scene) => scene.sceneId ?? scene.id;

export function RevisionAuthorityStatus({ accepted, candidate, working, dirty = false }: {
  accepted?: { id: string; version: number } | null;
  candidate?: { id: string; version: number } | null;
  working?: { id: string; version: number } | null;
  dirty?: boolean;
}) {
  return <div className="revision-authority-status" aria-label="版本与来源状态">
    <span>{accepted ? `正式稿 · 已采用 v${accepted.version}` : "正式稿 · 尚未采用"}</span>
    <span>{dirty ? "工作稿 · 未保存修改" : candidate && candidate.id !== accepted?.id ? `候选稿 · v${candidate.version} 待审阅` : "候选稿 · 无新候选"}</span>
    <span>{working ? `后续依据 · 工作稿 v${working.version}` : accepted ? `后续依据 · 正式稿 v${accepted.version}` : "后续依据 · 尚未指定"}</span>
  </div>;
}

export function ChapterPublicationStatus({ published, saved, dirty = false }: {
  published?: { id: string; version: number } | null;
  saved?: { id: string; version: number } | null;
  dirty?: boolean;
}) {
  const label = !published ? "未发布" : dirty || saved && saved.id !== published.id ? "有未发布修改" : "已发布";
  return <span className="chapter-publication-status" role="status" aria-live="polite" aria-label="章节发布状态">{label}</span>;
}

function StoryText({ body }: { body: string }) {
  return <div className="story-reading-text">{body.split(/\n\s*\n/u).map((paragraph, index) => <p key={index}>{paragraph}</p>)}</div>;
}

function sourcePlanSignature(refs: DraftRevision["sourcePlanRefs"] | null | undefined) {
  return refs ? JSON.stringify([refs.lineageStatus, refs.localOnly, refs.planRevisionId ?? null, refs.outlineRevisionId ?? null, refs.chapterPlanId ?? null]) : "null";
}

function sourcePlanLabel(refs: DraftRevision["sourcePlanRefs"] | null | undefined) {
  if (!refs || refs.localOnly) return "独立章节";
  return `${refs.chapterPlanId ? "关联章纲" : refs.planRevisionId ? "参考章纲" : refs.outlineRevisionId ? "参考大纲" : "独立章节"}${refs.lineageStatus === "historical" ? " · 历史版本" : ""}`;
}

function ChapterSceneDiff({ accepted, scenes, title, sourcePlanRefs }: { accepted: DraftRevision | null; scenes: Scene[]; title: string; sourcePlanRefs: DraftRevision["sourcePlanRefs"] | null }) {
  const before = accepted?.sceneGroups ?? [];
  const previous = new Map(before.map((scene, index) => [sceneKey(scene), { scene, index }]));
  const current = new Map(scenes.map((scene, index) => [sceneKey(scene), { scene, index }]));
  const changes = scenes.flatMap<{ key: string; label: string; scene: Scene; old: Scene | null; index: number }>((scene, index) => {
    const old = previous.get(sceneKey(scene));
    return !old ? [{ key: sceneKey(scene), label: "新增", scene, old: null, index }]
      : old.scene.body !== scene.body || old.index !== index || Boolean(old.scene.locked) !== Boolean(scene.locked)
        ? [{ key: sceneKey(scene), label: old.scene.body !== scene.body ? "修改" : old.index !== index ? "重排" : "保护调整", scene, old: old.scene, index }]
        : [];
  });
  const deleted = before.filter(scene => !current.has(sceneKey(scene)));
  const titleChanged = (accepted?.title ?? "") !== title;
  const sourceChanged = sourcePlanSignature(accepted?.sourcePlanRefs) !== sourcePlanSignature(sourcePlanRefs);
  const sourceChanges = [
    accepted?.sourcePlanRefs.localOnly !== sourcePlanRefs?.localOnly ? "章节关联方式已变化" : "",
    accepted?.sourcePlanRefs.chapterPlanId !== sourcePlanRefs?.chapterPlanId ? "关联的章纲章节已变化" : "",
    accepted?.sourcePlanRefs.planRevisionId !== sourcePlanRefs?.planRevisionId ? "章纲版本已变化" : "",
    accepted?.sourcePlanRefs.outlineRevisionId !== sourcePlanRefs?.outlineRevisionId ? "参考大纲版本已变化" : "",
    accepted?.sourcePlanRefs.lineageStatus !== sourcePlanRefs?.lineageStatus ? "关联状态已变化" : "",
  ].filter(Boolean);
  return <section className="chapter-scene-diff" aria-label="章节场景差异">
    <p>{accepted ? `已发布 v${accepted.version}` : "尚未发布"} → 本次编辑 · 按场景比较，包含未保存修改</p>
    {titleChanged && <article data-change-kind="标题"><h4>章节标题修改</h4><div className="chapter-diff-columns"><section><h5>已发布标题</h5><p>{accepted ? accepted.title : "尚未发布"}</p></section><section><h5>本次标题</h5><p>{title || "尚未填写"}</p></section></div></article>}
    {sourceChanged && <article data-change-kind="章纲来源"><h4>章纲关联变化</h4><p>{sourceChanges.join("；") || "章节关联记录已变化"}。</p><div className="chapter-diff-columns"><section><h5>已发布关联</h5><p>{accepted ? sourcePlanLabel(accepted.sourcePlanRefs) : "尚未发布"}</p></section><section><h5>本次关联</h5><p>{sourcePlanLabel(sourcePlanRefs)}</p></section></div></article>}
    {!changes.length && !deleted.length && !titleChanged && !sourceChanged && <p>本次编辑与已发布版本一致。</p>}
    {changes.map(change => <article key={change.key} data-diff-scene-id={change.key} data-change-kind={change.label}>
      <h4>{change.label} · 场景 {change.index + 1}</h4>
      {change.old && previous.get(change.key)?.index !== change.index && <p>位置：原场景 {previous.get(change.key)!.index + 1} → 场景 {change.index + 1}</p>}
      {change.old && Boolean(change.old.locked) !== Boolean(change.scene.locked) && <p>场景保护：{change.old.locked ? "已锁定" : "未锁定"} → {change.scene.locked ? "已锁定" : "未锁定"}</p>}
      <div className="chapter-diff-columns"><section><h5>已发布正文</h5>{change.old ? <StoryText body={change.old.body} /> : <p>无此场景</p>}</section><section><h5>本次正文</h5><StoryText body={change.scene.body} /></section></div>
    </article>)}
    {deleted.map(scene => <article key={sceneKey(scene)} data-diff-scene-id={sceneKey(scene)} data-change-kind="删除"><h4>删除 · 原场景 {before.indexOf(scene) + 1}</h4><StoryText body={scene.body} /></article>)}
  </section>;
}

export function ChapterDocumentView({ scope, scenes, accepted, title, sourcePlanRefs, onTitleChange, lockedIds, disabled, onChange, onActiveScene, onLocksChange, tools, toolsDirty, sceneDiscussion, emptyWriting }: {
  scope: string;
  scenes: Scene[];
  accepted: DraftRevision | null;
  title: string;
  sourcePlanRefs: DraftRevision["sourcePlanRefs"] | null;
  onTitleChange: (title: string) => void;
  lockedIds: string[];
  disabled: boolean;
  dirty: boolean;
  onChange: (scenes: Scene[]) => void;
  onActiveScene: (id: string) => void;
  onLocksChange?: (ids: string[]) => void;
  tools?: ReactNode;
  toolsDirty?: boolean;
  emptyWriting?: { disabled: boolean; onPrepare: () => void };
  sceneDiscussion?: { disabled: boolean; notice: string | null; onPrepare: (sceneGroupId: string) => void };
}) {
  const [versionsScope, setVersionsScope] = useState<string | null>(null);
  const editors = useRef(new Map<string, HTMLTextAreaElement>());
  const restoredScope = useRef<string | null>(null);
  const activeScope = useRef(scope);
  activeScope.current = scope;
  const remember = (scene: Scene, element: HTMLTextAreaElement) => {
    rememberSession(bookmarks, scope, { sceneKey: sceneKey(scene), start: element.selectionStart, end: element.selectionEnd,
      direction: element.selectionDirection, viewportOffset: element.getBoundingClientRect().top - (element.closest(".main-panel")?.getBoundingClientRect().top ?? 0) });
  };
  const restoreEditingPosition = () => {
    const bookmark = bookmarks.get(scope);
    const scene = bookmark && scenes.find(item => sceneKey(item) === bookmark.sceneKey);
    if (!bookmark || !scene || activeScope.current !== scope) return;
    const element = editors.current.get(sceneKey(scene));
    if (!element) return;
    element.focus({ preventScroll: true });
    element.setSelectionRange(bookmark.start, bookmark.end, bookmark.direction);
    const panel = element.closest<HTMLElement>(".main-panel");
    if (panel) panel.scrollTop += element.getBoundingClientRect().top - panel.getBoundingClientRect().top - bookmark.viewportOffset;
    onActiveScene(scene.id);
  };
  useEffect(() => { setVersionsScope(null); }, [scope]);
  useEffect(() => {
    const panel = editors.current.values().next().value?.closest<HTMLElement>(".main-panel");
    if (!panel) return;
    const rememberViewport = () => {
      if (activeScope.current !== scope || restoredScope.current !== scope || versionsScope === scope) return;
      const focusedScene = scenes.find(item => editors.current.get(sceneKey(item)) === document.activeElement);
      const bookmark = bookmarks.get(scope);
      const scene = focusedScene ?? scenes.find(item => sceneKey(item) === bookmark?.sceneKey) ?? scenes.find(item => {
        const element = editors.current.get(sceneKey(item));
        return element && element.getBoundingClientRect().bottom > panel.getBoundingClientRect().top;
      });
      if (scene) remember(scene, editors.current.get(sceneKey(scene))!);
    };
    panel.addEventListener("scroll", rememberViewport, { passive: true });
    return () => panel.removeEventListener("scroll", rememberViewport);
  }, [scope, scenes, versionsScope]);
  useEffect(() => {
    if (restoredScope.current === scope || !scenes.length) return;
    const frame = requestAnimationFrame(() => {
      if (activeScope.current !== scope) return;
      restoredScope.current = scope;
      restoreEditingPosition();
    });
    return () => cancelAnimationFrame(frame);
  }, [scope, scenes, onActiveScene]);
  const focusScene = (scene: Scene | undefined) => requestAnimationFrame(() => {
    if (scene && activeScope.current === scope) editors.current.get(sceneKey(scene))?.focus({ preventScroll: true });
  });
  const isLocked = (scene: Scene) => lockedIds.includes(scene.id);
  const move = (index: number, destination: number) => {
    const next = [...scenes];
    [next[index], next[destination]] = [next[destination], next[index]];
    onChange(next);
    focusScene(scenes[index]);
  };
  return <section className="chapter-document">
    <fieldset key={scope} className="draft-editor-wrap" data-chapter-draft-anchor disabled={disabled}>
      <legend className="sr-only">章节正文</legend>
      <label className="chapter-title"><span className="sr-only">章节标题</span><input aria-label="章节标题" value={title} disabled={disabled} onChange={event => onTitleChange(event.target.value)} placeholder="填写这一章的标题" /></label>
      {emptyWriting && <div className="chapter-empty-writing"><button type="button" aria-label="让 AI 起草本章" className="chapter-writing-entry text-button" disabled={disabled || emptyWriting.disabled} onClick={emptyWriting.onPrepare}>让 AI 起草</button><span>也可以直接开始写作。</span></div>}
      {scenes.map((scene, index) => <section key={sceneKey(scene)} data-scene-id={scene.id}>
        <label><strong className={scenes.length === 1 && !isLocked(scene) ? "sr-only" : "chapter-scene-label"}>{scenes.length === 1 ? "正文" : `场景 ${index + 1}`}{isLocked(scene) ? " · 已锁定" : ""}</strong>
          <textarea ref={element => { if (element) editors.current.set(sceneKey(scene), element); else editors.current.delete(sceneKey(scene)); }} className="draft-editor" aria-label={`场景 ${index + 1} 正文`} value={scene.body} disabled={disabled} readOnly={isLocked(scene)}
            onFocus={event => { remember(scene, event.currentTarget); onActiveScene(scene.id); }}
            onSelect={event => remember(scene, event.currentTarget)} onScroll={event => remember(scene, event.currentTarget)} onBlur={event => remember(scene, event.currentTarget)}
            onChange={event => { if (document.activeElement === event.currentTarget) remember(scene, event.currentTarget); onChange(scenes.map(item => item.id === scene.id ? { ...item, body: event.target.value } : item)); }} placeholder="从一个场景开始……" />
        </label>
      </section>)}
    </fieldset>
    <details key={`tools:${scope}`} className="chapter-manual-tools" onToggle={event => {
      if (!event.currentTarget.open && event.currentTarget.contains(document.activeElement)) {
        event.currentTarget.querySelector<HTMLElement>(":scope > summary")?.focus({ preventScroll: true });
      }
    }}><summary>手动工具{toolsDirty ? " · 有未保存修改" : ""}</summary>
      <p>也可以直接在对话中说明要分析或调整什么。这里保留精确编辑与无模型时使用的工具。</p>
      <fieldset className="chapter-scene-tools" disabled={disabled}><legend className="sr-only">场景与保护</legend>
      <details key={`scenes:${scope}`} className="chapter-scene-management"><summary>场景与保护 · {scenes.length}</summary>
      {scenes.map((scene, index) => <div key={sceneKey(scene)}><strong>场景 {index + 1}{isLocked(scene) ? " · 已锁定" : ""}</strong>
        <div className="draft-scene-actions">
          {onLocksChange && <label><input type="checkbox" aria-label={`保护场景 ${index + 1}`} checked={isLocked(scene)} onChange={event => onLocksChange(event.target.checked ? [...new Set([...lockedIds, scene.id])] : lockedIds.filter(id => id !== scene.id))} />保护此段</label>}
          {sceneDiscussion && <button type="button" aria-label={`讨论场景 ${index + 1}`} disabled={sceneDiscussion.disabled} onClick={() => sceneDiscussion.onPrepare(scene.id)}>讨论这个场景</button>}
          <button type="button" disabled={isLocked(scene) || index === 0 || isLocked(scenes[index - 1])} onClick={() => move(index, index - 1)}>上移场景 {index + 1}</button>
          <button type="button" disabled={isLocked(scene) || index === scenes.length - 1 || isLocked(scenes[index + 1])} onClick={() => move(index, index + 1)}>下移场景 {index + 1}</button>
          <button type="button" disabled={scenes.slice(index).some(isLocked)} onClick={() => { onChange(scenes.filter(item => item.id !== scene.id)); focusScene(scenes[index - 1] ?? scenes[index + 1]); }}>删除场景 {index + 1}</button>
        </div>
      </div>)}
      {sceneDiscussion?.notice && <p className="scene-discussion-notice" role="status">{sceneDiscussion.notice}</p>}
      <button type="button" className="draft-add-scene" onClick={() => { const scene = newDraftScene(); onChange([...scenes, scene]); focusScene(scene); }}>＋ 添加场景</button>
      </details>
      </fieldset>
      {tools}
    </details>
    {accepted?.sceneGroups.some(scene => scene.body.trim()) && <details key={`versions:${scope}`} className="chapter-versions" open={versionsScope === scope} onToggle={event => {
      if (activeScope.current !== scope) return;
      const opened = event.currentTarget.open;
      const returnToEditor = !opened && event.currentTarget.contains(document.activeElement);
      setVersionsScope(current => opened ? scope : current === scope ? null : current);
      if (returnToEditor) requestAnimationFrame(restoreEditingPosition);
    }}>
      <summary>已发布版本与修改对照</summary>
      {versionsScope === scope && <>
        <article className="chapter-published-reading" aria-label="已发布章节正文" data-discussion-revision={accepted?.id ?? ""}>
          {accepted ? <><small>已发布 v{accepted.version} · 只读</small><h3>{accepted.title}</h3>{accepted.sceneGroups.map((scene, index) => <section key={sceneKey(scene)}><h4>场景 {index + 1}</h4><StoryText body={scene.body} /></section>)}</> : <p>本章尚未发布。</p>}
        </article>
        <ChapterSceneDiff accepted={accepted} scenes={scenes} title={title} sourcePlanRefs={sourcePlanRefs} />
        <button type="button" className="chapter-versions-return" onClick={() => { setVersionsScope(null); requestAnimationFrame(restoreEditingPosition); }}>返回正文</button>
      </>}
    </details>}
  </section>;
}
